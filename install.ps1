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

& $target setup
if ($LASTEXITCODE -ne 0) { throw 'status_cli setup could not configure every host' }
Write-Host 'Done. Open a new Claude Code or agy session to see the status line.'
