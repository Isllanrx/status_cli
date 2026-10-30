$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'Isllanrx/status_cli'
$base = if ($env:STATUS_CLI_BASE_URL) { $env:STATUS_CLI_BASE_URL } else { "https://github.com/$repo/releases/latest/download" }
$binDir = if ($env:STATUS_CLI_INSTALL_DIR) { $env:STATUS_CLI_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }

$arm = $env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64'
$asset = if ($arm) { 'status_cli-aarch64-pc-windows-msvc.exe' } else { 'status_cli-x86_64-pc-windows-msvc.exe' }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading $asset"
    foreach ($file in $asset, 'SHA256SUMS') {
        Invoke-WebRequest -Uri "$base/$file" -OutFile (Join-Path $tmp $file) -UseBasicParsing
    }

    $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { ($_ -split '\s+')[1] -eq $asset } | Select-Object -First 1
    if (-not $line) { throw "no checksum published for $asset" }
    $expected = ($line -split '\s+')[0]
    $actual = (Get-FileHash (Join-Path $tmp $asset) -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "checksum mismatch for $asset" }

    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    $target = Join-Path $binDir 'status_cli.exe'
    if (Test-Path $target) {
        $old = "$target.old"
        Remove-Item $old -Force -ErrorAction SilentlyContinue
        Rename-Item $target $old
    }
    Move-Item (Join-Path $tmp $asset) $target
    Unblock-File $target
    Remove-Item "$target.old" -Force -ErrorAction SilentlyContinue
    Write-Host "Installed $(& $target --version) to $binDir"
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (($userPath -split ';') -contains $binDir)) {
    [Environment]::SetEnvironmentVariable('Path', (@($userPath, $binDir) | Where-Object { $_ }) -join ';', 'User')
    $env:Path = "$env:Path;$binDir"
    Write-Host "Added $binDir to your user PATH"
}

function Install-NerdFont {
    $version = 'v3.5.1'
    $archive = 'CascadiaCode.tar.xz'
    $fonts = 'Regular', 'Bold', 'Italic', 'BoldItalic' | ForEach-Object { "CaskaydiaCoveNerdFont-$_.ttf" }
    $fontDir = Join-Path $env:LOCALAPPDATA 'Microsoft\Windows\Fonts'
    if (Test-Path (Join-Path $fontDir $fonts[0])) { return }
    $url = "https://github.com/ryanoasis/nerd-fonts/releases/download/$version"
    $dir = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $dir | Out-Null
    try {
        foreach ($file in $archive, 'SHA-256.txt') {
            Invoke-WebRequest -Uri "$url/$file" -OutFile (Join-Path $dir $file) -UseBasicParsing
        }
        $line = Get-Content (Join-Path $dir 'SHA-256.txt') | Where-Object { ($_ -split '\s+')[1] -eq $archive } | Select-Object -First 1
        if (-not $line) { throw "no checksum published for $archive" }
        $actual = (Get-FileHash (Join-Path $dir $archive) -Algorithm SHA256).Hash
        if ($actual -ne ($line -split '\s+')[0]) { throw "checksum mismatch for $archive" }
        & tar.exe -xf (Join-Path $dir $archive) -C $dir @fonts
        if ($LASTEXITCODE -ne 0) { throw "could not extract $archive" }
        New-Item -ItemType Directory -Force -Path $fontDir | Out-Null
        $key = 'HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts'
        if (-not (Test-Path $key)) { New-Item -Path $key -Force | Out-Null }
        foreach ($font in $fonts) {
            $path = Join-Path $fontDir $font
            Copy-Item (Join-Path $dir $font) $path -Force
            $style = $font -replace '^CaskaydiaCoveNerdFont-|\.ttf$', ''
            Set-ItemProperty -Path $key -Name "CaskaydiaCove Nerd Font $style (TrueType)" -Value $path
        }
        Write-Host "Installed CaskaydiaCove Nerd Font $version to $fontDir"
    }
    finally {
        Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Set-TerminalFont {
    $face = 'CaskaydiaCove Nerd Font'
    $files = Get-Item "$env:LOCALAPPDATA\Packages\Microsoft.WindowsTerminal*_8wekyb3d8bbwe\LocalState\settings.json" -ErrorAction SilentlyContinue
    foreach ($file in $files) {
        try {
            $json = Get-Content $file.FullName -Raw | ConvertFrom-Json
            if ($null -eq $json.profiles -or $json.profiles -is [array]) { continue }
            if ($null -eq $json.profiles.defaults) { $json.profiles | Add-Member -NotePropertyName defaults -NotePropertyValue ([pscustomobject]@{}) }
            $defaults = $json.profiles.defaults
            if ($null -eq $defaults.font) { $defaults | Add-Member -NotePropertyName font -NotePropertyValue ([pscustomobject]@{}) }
            $current = $defaults.font.face
            if ($current -and $current -notmatch '^Cascadia (Code|Mono)$') { continue }
            if ($current -eq $face) { continue }
            Copy-Item $file.FullName "$($file.FullName).bak-status_cli" -Force
            $defaults.font | Add-Member -NotePropertyName face -NotePropertyValue $face -Force
            [IO.File]::WriteAllText($file.FullName, ($json | ConvertTo-Json -Depth 64), (New-Object Text.UTF8Encoding $false))
            Write-Host "Windows Terminal now uses $face (backup: $($file.Name).bak-status_cli)"
        }
        catch {
            Write-Warning "Windows Terminal settings left unchanged: $_"
        }
    }
}

if (-not $env:STATUS_CLI_NO_FONT) {
    try {
        Install-NerdFont
        Set-TerminalFont
    }
    catch {
        Write-Warning "Nerd Font not installed ($_); the status line uses plain symbols instead of icons"
    }
}

& $target setup
if ($LASTEXITCODE -ne 0) { throw 'status_cli setup could not configure every host' }
Write-Host 'Done. Open a new Claude Code or agy session to see the status line.'
