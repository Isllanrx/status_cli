use std::env;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

const LAUNCHERS: [&str; 5] = ["codex.exe", "codex.cmd", "codex.bat", "codex.ps1", "codex"];
const BACKUP: &str = "machine-path.bak";

pub fn dir() -> Option<PathBuf> {
    env::var_os("ProgramFiles").map(|dir| PathBuf::from(dir).join("status_cli"))
}

pub fn install(exe: &Path) -> Result<String, String> {
    let dir = dir().ok_or("codex: ProgramFiles is not set; not intercepted")?;
    let shim = dir.join(format!("codex{}", env::consts::EXE_SUFFIX));
    let done = copy(exe, &shim).and_then(|()| route(&dir));
    match done {
        Ok(()) => {
            Ok(format!("codex: runs through {} (first on the machine PATH; open a new terminal)", shim.display()))
        }
        Err(err) if err.kind() == ErrorKind::PermissionDenied => Ok(format!(
            "codex: not intercepted; run `status_cli setup` once from an elevated PowerShell (writes {} and the machine PATH)",
            dir.display()
        )),
        Err(err) => Err(format!("codex: not intercepted, {err} -> {}", dir.display())),
    }
}

pub fn doctor() -> String {
    let Some(dir) = dir() else { return "info    codex is not intercepted: ProgramFiles is not set".to_owned() };
    let first = env::var_os("PATH")
        .and_then(|path| env::split_paths(&path).find(|entry| LAUNCHERS.iter().any(|name| entry.join(name).is_file())));
    match first {
        Some(first) if same(&first.to_string_lossy(), &dir.to_string_lossy()) => {
            format!("ok      codex runs through status_cli -> {}", dir.display())
        }
        Some(first) => format!(
            "info    codex resolves to {} first; run `status_cli setup` from an elevated PowerShell, then open a new terminal",
            first.display()
        ),
        None => "info    codex is not on PATH".to_owned(),
    }
}

fn copy(exe: &Path, shim: &Path) -> io::Result<()> {
    fs::create_dir_all(shim.parent().ok_or(ErrorKind::NotFound)?)?;
    if shim.exists() && fs::remove_file(shim).is_err() {
        let stale = shim.with_extension("old");
        let _ = fs::remove_file(&stale);
        fs::rename(shim, &stale)?;
    }
    fs::copy(exe, shim).map(drop)
}

fn route(dir: &Path) -> io::Result<()> {
    let current = machine::read()?;
    let dir_text = dir.to_string_lossy();
    let placed = place(&current, &dir_text, |entry| {
        let entry = PathBuf::from(expand(entry));
        LAUNCHERS.iter().any(|name| entry.join(name).is_file())
    });
    if placed == current {
        return Ok(());
    }
    let backup = dir.join(BACKUP);
    if !backup.exists() {
        fs::write(&backup, &current)?;
    }
    machine::write(&placed)
}

fn place(path: &str, dir: &str, launcher: impl Fn(&str) -> bool) -> String {
    let mut entries: Vec<&str> = path.split(';').filter(|entry| !same(entry, dir)).collect();
    let at = entries.iter().position(|entry| !entry.trim().is_empty() && launcher(entry)).unwrap_or(0);
    entries.insert(at, dir);
    entries.join(";")
}

fn same(a: &str, b: &str) -> bool {
    let normalize = |text: &str| text.trim().trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase();
    !a.trim().is_empty() && normalize(a) == normalize(b)
}

fn expand(entry: &str) -> String {
    let mut out = String::new();
    let mut rest = entry;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else { break };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match env::var(name) {
            Ok(value) if !name.is_empty() => out.push_str(&value),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out + rest
}

#[cfg(windows)]
mod machine {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, REG_EXPAND_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegGetValueW,
        RegSetKeyValueW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    const KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn check(status: u32) -> io::Result<()> {
        if status == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(status as i32)) }
    }

    pub fn read() -> io::Result<String> {
        let (key, name) = (wide(KEY), wide("Path"));
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut size = 0u32;
        let query = |data: *mut u16, size: &mut u32| unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                std::ptr::null_mut(),
                data.cast(),
                size,
            )
        };
        check(query(std::ptr::null_mut(), &mut size))?;
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        let mut size = (buffer.len() * 2) as u32;
        check(query(buffer.as_mut_ptr(), &mut size))?;
        let len = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
        OsString::from_wide(&buffer[..len])
            .into_string()
            .map_err(|_| io::Error::other("the machine PATH is not valid Unicode; left unchanged"))
    }

    pub fn write(path: &str) -> io::Result<()> {
        let (key, name, data) = (wide(KEY), wide("Path"), wide(path));
        let bytes = u32::try_from(data.len() * 2).map_err(io::Error::other)?;
        check(unsafe {
            RegSetKeyValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), name.as_ptr(), REG_EXPAND_SZ, data.as_ptr().cast(), bytes)
        })?;
        let area = wide("Environment");
        let mut result = 0usize;
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                area.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5_000,
                &mut result,
            )
        };
        Ok(())
    }
}

#[cfg(not(windows))]
mod machine {
    use std::io;

    pub fn read() -> io::Result<String> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn write(_: &str) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIM: &str = r"C:\Program Files\status_cli";

    fn has_codex(entry: &str) -> bool {
        entry.contains("npm") || entry.contains("Codex")
    }

    #[test]
    fn the_shim_goes_right_before_the_first_codex_launcher() {
        let path = r"C:\Windows\system32;%APPDATA%\npm;C:\Tools";
        let placed = place(path, SHIM, has_codex);
        assert_eq!(placed, format!(r"C:\Windows\system32;{SHIM};%APPDATA%\npm;C:\Tools"));
        assert_eq!(place(&placed, SHIM, has_codex), placed);
    }

    #[test]
    fn a_misplaced_shim_entry_is_moved_and_the_rest_is_kept() {
        let path = format!(r"C:\Codex\bin;C:\Windows;;{SHIM}\");
        assert_eq!(place(&path, SHIM, has_codex), format!(r"{SHIM};C:\Codex\bin;C:\Windows;"));
        assert_eq!(place(r"C:\Windows", SHIM, has_codex), format!(r"{SHIM};C:\Windows"));
    }

    #[test]
    fn expands_known_variables_and_keeps_unknown_ones() {
        let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).unwrap_or_default();
        let name = if env::var("HOME").is_ok() { "HOME" } else { "USERPROFILE" };
        assert_eq!(expand(&format!("%{name}%\\npm")), format!("{home}\\npm"));
        assert_eq!(expand("%STATUS_CLI_UNSET_VAR%\\x"), "%STATUS_CLI_UNSET_VAR%\\x");
        assert_eq!(expand("C:\\plain"), "C:\\plain");
    }
}
