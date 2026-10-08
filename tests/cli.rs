use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!("status_cli-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn cache_files(&self) -> Vec<String> {
        let mut names: Vec<_> =
            fs::read_dir(&self.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Output {
    raw: String,
    visible: String,
    text: String,
    width: usize,
}

fn run(temp: &TempDir, input: &str, columns: Option<usize>) -> Output {
    run_with(temp, input, columns, &[])
}

fn run_raw(temp: &TempDir, input: &str, vars: &[(&str, &str)]) -> String {
    run_with(temp, input, None, vars).raw
}

fn run_with(temp: &TempDir, input: &str, columns: Option<usize>, vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_status_cli"));
    cmd.env("TMPDIR", &temp.0)
        .env("TEMP", &temp.0)
        .env("TMP", &temp.0)
        .env_remove("COLUMNS")
        .env_remove("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .env_remove("XDG_RUNTIME_DIR")
        .env_remove("STATUS_CLI_LOG")
        .env_remove("STATUS_CLI_COLOR")
        .env_remove("STATUS_CLI_ASCII")
        .env_remove("NO_COLOR")
        .env_remove("TERM")
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

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

fn claude(session: &str, duration_ms: u64) -> String {
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

fn assert_clock(text: &str, hhmm: &str) {
    assert!(text.ends_with(hhmm), "{text:?} should end with {hhmm}");
}

#[test]
fn claude_line_shows_every_segment() {
    let temp = TempDir::new();
    let out = run(&temp, &claude("a", 3_900_000), Some(160));
    assert!(out.text.starts_with("Opus - High ╱ sessão "), "{:?}", out.text);
    assert!(out.text.contains("72% ↻ 1h29m") || out.text.contains("72% ↻ 1h30m"), "{:?}", out.text);
    assert!(out.text.contains(" 31% ╱ compactar "), "{:?}", out.text);
    assert!(out.text.contains(" 57% ╱ tempo "), "{:?}", out.text);
    assert_clock(&out.text, "01:05");
}

#[test]
fn missing_rate_limits_render_placeholders() {
    let temp = TempDir::new();
    let out = run(&temp, r#"{"model":{"display_name":"Opus 5.5"}}"#, Some(160));
    assert!(out.text.starts_with(&format!("Opus ╱ sessão {} –", "─".repeat(10))), "{:?}", out.text);
}

#[test]
fn each_session_keeps_its_own_clock() {
    let temp = TempDir::new();
    assert_clock(&run(&temp, &claude("a", 60_000), Some(160)).text, "00:01");
    assert_clock(&run(&temp, &claude("b", 7_200_000), Some(160)).text, "02:00");
    assert_eq!(temp.cache_files(), ["status_cli-a", "status_cli-b"]);
}

#[test]
fn clock_ticks_between_events_and_rebases_on_new_duration() {
    let temp = TempDir::new();
    assert_clock(&run(&temp, &claude("a", 3_540_000), Some(160)).text, "00:59");
    let file = temp.0.join("status_cli-a");
    let mut state: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    let at = state["clock"]["at"].as_u64().unwrap();
    state["clock"]["at"] = (at - 125_000).into();
    fs::write(&file, state.to_string()).unwrap();
    assert_clock(&run(&temp, &claude("a", 3_540_000), Some(160)).text, "01:01");
    assert_clock(&run(&temp, &claude("a", 600_000), Some(160)).text, "00:10");
}

#[test]
fn fills_the_usable_width_without_overflowing() {
    let temp = TempDir::new();
    for columns in [70, 80, 100, 120, 160, 220] {
        let out = run(&temp, &claude("a", 0), Some(columns));
        assert_eq!(out.width, columns - 8, "columns {columns}: {:?}", out.visible);
        assert!(!out.visible.starts_with(' '));
    }
}

#[test]
fn no_columns_means_no_padding() {
    let temp = TempDir::new();
    assert!(run(&temp, &claude("a", 0), None).visible.starts_with("Opus"));
}

#[test]
fn invalid_json_prints_a_short_error() {
    let temp = TempDir::new();
    assert!(run(&temp, "{broken", None).text.starts_with("status_cli: "));
}

#[test]
fn agy_line_uses_quota_mode_and_terminal_width() {
    let temp = TempDir::new();
    let input = r#"{"product":"antigravity","conversation_id":"conv-1",
        "model":{"id":"gemini-3-pro","display_name":"Gemini 3 Pro"},"execution_mode":"planning",
        "quota":{"gemini-3-flash":{"remaining_fraction":0.1},"gemini-3-pro":{"remaining_fraction":0.75,"reset_in_seconds":7200}},
        "context_window":{"used_percentage":40,"context_window_size":1000000},"terminal_width":150}"#;
    let out = run(&temp, input, None);
    assert_eq!(out.width, 142);
    assert!(out.text.starts_with("Gemini - Planning ╱ cota "), "{:?}", out.text);
    assert!(out.text.contains(" 25% ↻"), "{:?}", out.text);
    assert!(out.text.contains("contexto ━━━━──────"), "{:?}", out.text);
    assert_clock(&out.text, "00:00");
    assert_eq!(temp.cache_files(), ["status_cli-agy-conv-1"]);
}

#[test]
fn stale_caches_are_pruned() {
    let temp = TempDir::new();
    let old = temp.0.join("status_cli-old");
    File::create(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(8 * 86_400)).unwrap();
    run(&temp, &claude("new", 0), Some(160));
    assert!(!old.exists());
}

#[test]
fn oversized_input_is_rejected_without_crashing() {
    let temp = TempDir::new();
    let input = format!(r#"{{"model":{{"display_name":"{}"}}}}"#, "x".repeat(2 << 20));
    assert!(run(&temp, &input, None).text.starts_with("status_cli: "));
}

#[test]
fn log_records_one_json_line_per_run() {
    let temp = TempDir::new();
    let log = temp.0.join("events.jsonl");
    let log_path = log.to_str().unwrap();
    run_with(&temp, &claude("a", 0), Some(160), &[("STATUS_CLI_LOG", log_path)]);
    run_with(&temp, "{broken", None, &[("STATUS_CLI_LOG", log_path)]);
    let lines: Vec<serde_json::Value> =
        fs::read_to_string(&log).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["host"], "claude");
    assert_eq!(lines[0]["session"], "a");
    assert!(lines[0]["error"].is_null());
    assert!(lines[0]["us"].as_u64().is_some());
    assert!(lines[1]["error"].as_str().unwrap().starts_with("key must be a string"));
}

#[test]
fn runtime_dir_takes_precedence_for_the_cache() {
    let temp = TempDir::new();
    let runtime = temp.0.join("runtime");
    fs::create_dir(&runtime).unwrap();
    run_with(&temp, &claude("a", 0), None, &[("XDG_RUNTIME_DIR", runtime.to_str().unwrap())]);
    assert!(runtime.join("status_cli-a").exists());
}

#[test]
fn utf8_bom_from_windows_powershell_is_accepted() {
    let temp = TempDir::new();
    let out = run(&temp, "\u{feff}{\"model\":{\"display_name\":\"Opus 5.5\"}}\r\n", None);
    assert!(out.text.starts_with("Opus ╱ "), "{:?}", out.text);
}

#[test]
fn ascii_and_no_color_degrade_cleanly() {
    let temp = TempDir::new();
    let out = run_with(&temp, &claude("a", 0), Some(160), &[("STATUS_CLI_ASCII", "1"), ("NO_COLOR", "1")]);
    assert!(!out.raw.contains('\x1b'));
    assert!(out.visible.trim_start().starts_with("Opus - High / sessão ="), "{:?}", out.visible);
}

#[test]
fn ansi16_terminals_only_get_basic_colors() {
    let temp = TempDir::new();
    let raw = run_raw(&temp, &claude("a", 0), &[("TERM", "linux")]);
    assert!(raw.contains("\x1b[") && !raw.contains("38;2;") && !raw.contains("38;5;"), "{raw:?}");
}
