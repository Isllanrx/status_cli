pub mod payload;
pub mod state;
pub mod telemetry;
pub mod time;

use std::env;
use std::path::PathBuf;
use std::process::Command;

pub fn cache_dir() -> PathBuf {
    let base = ["XDG_RUNTIME_DIR", "LOCALAPPDATA", "XDG_CACHE_HOME"]
        .into_iter()
        .find_map(env::var_os)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(env::temp_dir);
    base.join("status_cli")
}

#[cfg(windows)]
pub fn spawn_detached(scope: &str, command: impl Fn() -> Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    if command().creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn().is_err() {
        telemetry::logged(scope, command().creation_flags(flags).spawn());
    }
}

#[cfg(not(windows))]
pub fn spawn_detached(scope: &str, command: impl Fn() -> Command) {
    telemetry::logged(scope, command().spawn());
}
