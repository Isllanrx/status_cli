#![allow(dead_code)]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!("status_cli-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn cache_files(&self) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(self.0.join("status_cli"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Output {
    pub raw: String,
    pub visible: String,
    pub text: String,
    pub width: usize,
}

pub fn run(temp: &TempDir, input: &str, columns: Option<usize>) -> Output {
    run_with(temp, input, columns, &[])
}

pub fn run_raw(temp: &TempDir, input: &str, vars: &[(&str, &str)]) -> String {
    run_with(temp, input, None, vars).raw
}

pub fn run_with(temp: &TempDir, input: &str, columns: Option<usize>, vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_status_cli"));
    cmd.env("TMPDIR", &temp.0)
        .env("TEMP", &temp.0)
        .env("TMP", &temp.0)
        .env_remove("COLUMNS")
        .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .env("XDG_RUNTIME_DIR", &temp.0)
        .env_remove("STATUS_CLI_LOG")
        .env_remove("STATUS_CLI_COLOR")
        .env_remove("STATUS_CLI_ASCII")
        .env_remove("NO_COLOR")
        .env_remove("TERM")
        .env_remove("LANG")
        .env_remove("LC_ALL")
        .env_remove("LC_CTYPE")
        .envs(vars.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if let Some(c) = columns {
        cmd.env("COLUMNS", c.to_string());
    }
    let mut child = cmd.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let raw = String::from_utf8(out.stdout).unwrap();
    let mut visible = String::new();
    let mut in_escape = false;
    for c in raw.chars() {
        match c {
            '\x1b' => in_escape = true,
            'm' if in_escape => in_escape = false,
            _ if !in_escape => visible.push(c),
            _ => {}
        }
    }
    Output { text: visible.replace('\u{2800}', ""), width: visible.chars().count(), visible, raw }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

pub fn claude(session: &str, duration_ms: u64) -> String {
    let now = now_secs();
    format!(
        r#"{{"session_id":"{session}","model":{{"display_name":"Opus 5.5"}},"effort":{{"level":"high"}},
            "rate_limits":{{"five_hour":{{"used_percentage":72,"resets_at":{}}},
                            "seven_day":{{"used_percentage":31,"resets_at":{}}}}},
            "context_window":{{"used_percentage":48,"context_window_size":200000}},
            "cost":{{"total_duration_ms":{duration_ms}}}}}"#,
        now + 5400,
        now + 300_000
    )
}

pub fn assert_clock(text: &str, hhmm: &str) {
    let clock = text.rsplit(' ').next().unwrap_or_default();
    assert!(clock.starts_with(hhmm) && clock.len() == hhmm.len() + 3, "{text:?} should show {hhmm}:ss");
}
