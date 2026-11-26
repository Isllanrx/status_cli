#[cfg(any(windows, target_os = "linux"))]
use std::path::Path;

#[cfg(any(windows, target_os = "linux"))]
const MAX_DEPTH: usize = 16;

#[cfg(any(windows, target_os = "linux"))]
fn named(path: &str, name: &str) -> bool {
    Path::new(path).file_stem().is_some_and(|stem| stem.to_string_lossy().eq_ignore_ascii_case(name))
}

#[cfg(windows)]
pub fn ancestor_started(name: &str, _now: u64) -> Option<u64> {
    let mut child = Process::open(std::process::id())?;
    let mut child_created = child.created()?;
    for _ in 0..MAX_DEPTH {
        let parent = Process::open(child.parent()?)?;
        let created = parent.created().filter(|&at| at <= child_created)?;
        if named(&parent.image()?, name) {
            return Some(created);
        }
        (child, child_created) = (parent, created);
    }
    None
}

#[cfg(windows)]
struct Process(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Process {
    fn open(pid: u32) -> Option<Self> {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() { None } else { Some(Self(handle)) }
    }

    fn parent(&self) -> Option<u32> {
        use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
        use windows_sys::Win32::System::Threading::PROCESS_BASIC_INFORMATION;
        let mut info = PROCESS_BASIC_INFORMATION::default();
        let size = size_of::<PROCESS_BASIC_INFORMATION>() as u32;
        let buffer = std::ptr::from_mut(&mut info).cast();
        let status =
            unsafe { NtQueryInformationProcess(self.0, ProcessBasicInformation, buffer, size, std::ptr::null_mut()) };
        if status != 0 {
            return None;
        }
        u32::try_from(info.InheritedFromUniqueProcessId).ok()
    }

    fn created(&self) -> Option<u64> {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::GetProcessTimes;
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        if unsafe { GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
            return None;
        }
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Some(ticks.checked_sub(116_444_736_000_000_000)? / 10_000)
    }

    fn image(&self) -> Option<String> {
        use windows_sys::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.0, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len) } == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(buffer.get(..len as usize)?))
    }
}

#[cfg(windows)]
impl Drop for Process {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(target_os = "linux")]
pub fn ancestor_started(name: &str, _now: u64) -> Option<u64> {
    let boot = std::fs::read_to_string("/proc/stat").ok()?;
    let boot_ms = boot.lines().find_map(|line| line.strip_prefix("btime "))?.trim().parse::<u64>().ok()? * 1000;
    let stat = |pid: u32| -> Option<(u32, u64, String)> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (head, rest) = text.rsplit_once(')')?;
        let mut fields = rest.split_whitespace();
        let parent = fields.nth(1)?.parse().ok()?;
        let ticks: u64 = fields.nth(17)?.parse().ok()?;
        Some((parent, boot_ms + ticks * 1000 / CLOCK_TICKS, head.split_once('(')?.1.to_owned()))
    };
    let mut pid = std::process::id();
    let mut child_started = stat(pid)?.1;
    for _ in 0..MAX_DEPTH {
        let parent = stat(pid)?.0;
        let (_, started, comm) = stat(parent)?;
        if started > child_started {
            return None;
        }
        if named(&comm, name) {
            return Some(started);
        }
        (pid, child_started) = (parent, started);
    }
    None
}

#[cfg(target_os = "linux")]
const CLOCK_TICKS: u64 = 100;

#[cfg(not(any(windows, target_os = "linux")))]
pub fn ancestor_started(_name: &str, _now: u64) -> Option<u64> {
    None
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn finds_the_start_of_a_live_ancestor() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let started = ancestor_started("cargo", now).expect("tests run under cargo");
        assert!(started <= now && now - started < 86_400_000);
        assert_eq!(ancestor_started("no-such-ancestor", now), None);
    }

    #[test]
    fn matches_the_executable_stem_without_case() {
        let path = |name: &str| Path::new("bin").join(name).to_string_lossy().into_owned();
        assert!(named(&path("agy.exe"), "agy"));
        assert!(named(&path("Claude"), "claude"));
        assert!(!named(&path("agyx"), "agy"));
    }
}
