use std::collections::HashMap;
use std::path::Path;

const MAX_DEPTH: usize = 16;

type Table = HashMap<u32, Entry>;

struct Entry {
    parent: u32,
    name: String,
}

pub fn ancestor_started(name: &str, now: u64) -> Option<u64> {
    let (table, started) = snapshot(now)?;
    let mut pid = std::process::id();
    let own = started(pid)?;
    for _ in 0..MAX_DEPTH {
        pid = table.get(&pid)?.parent;
        let stem = Path::new(&table.get(&pid)?.name).file_stem().map(|stem| stem.to_string_lossy().into_owned());
        if stem.is_some_and(|stem| stem.eq_ignore_ascii_case(name)) {
            return started(pid).filter(|&at| at <= own);
        }
    }
    None
}

#[cfg(windows)]
fn snapshot(_now: u64) -> Option<(Table, impl Fn(u32) -> Option<u64>)> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let mut table = Table::new();
    unsafe {
        let handle = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(handle, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            table.insert(entry.th32ProcessID, Entry { parent: entry.th32ParentProcessID, name });
            more = Process32NextW(handle, &mut entry) != 0;
        }
        CloseHandle(handle);
    }
    let started = |pid| unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let ok = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(process);
        if ok { filetime_millis(created) } else { None }
    };
    Some((table, started))
}

#[cfg(windows)]
fn filetime_millis(time: windows_sys::Win32::Foundation::FILETIME) -> Option<u64> {
    let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Some(ticks.checked_sub(116_444_736_000_000_000)? / 10_000)
}

#[cfg(unix)]
fn snapshot(now: u64) -> Option<(Table, impl Fn(u32) -> Option<u64>)> {
    let out = std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid=,etime=,comm="]).output().ok()?;
    let row = |line: &str| -> Option<(u32, u64, Entry)> {
        let mut fields = line.split_whitespace();
        let pid = fields.next()?.parse().ok()?;
        let parent = fields.next()?.parse().ok()?;
        let elapsed = elapsed_seconds(fields.next()?)?;
        Some((pid, elapsed, Entry { parent, name: fields.collect::<Vec<_>>().join(" ") }))
    };
    let mut table = Table::new();
    let mut started = HashMap::new();
    for (pid, elapsed, entry) in String::from_utf8_lossy(&out.stdout).lines().filter_map(row) {
        table.insert(pid, entry);
        started.insert(pid, now.saturating_sub(elapsed * 1000));
    }
    Some((table, move |pid| started.get(&pid).copied()))
}

#[cfg(any(unix, test))]
fn elapsed_seconds(text: &str) -> Option<u64> {
    let (days, clock) = match text.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, text),
    };
    let seconds = clock.split(':').try_fold(0, |total, part| Some(total * 60 + part.parse::<u64>().ok()?))?;
    Some(days * 86_400 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ps_elapsed_time() {
        assert_eq!(elapsed_seconds("05:09"), Some(309));
        assert_eq!(elapsed_seconds("01:00:00"), Some(3_600));
        assert_eq!(elapsed_seconds("2-03:04:05"), Some(2 * 86_400 + 3 * 3_600 + 4 * 60 + 5));
        assert_eq!(elapsed_seconds("x"), None);
    }

    #[test]
    fn finds_the_start_of_a_live_ancestor() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let started = ancestor_started("cargo", now).expect("tests run under cargo");
        assert!(started <= now && now - started < 86_400_000);
        assert_eq!(ancestor_started("no-such-ancestor", now), None);
    }
}
