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
    let file = temp.0.join("status_cli").join("status_cli-a");
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
    assert!(out.text.starts_with("Gemini Pro - Planning ╱ cota "), "{:?}", out.text);
    assert!(out.text.contains(" 25% ↻"), "{:?}", out.text);
    assert!(out.text.contains("contexto ━━━━──────"), "{:?}", out.text);
    assert_clock(&out.text, "00:00");
    assert_eq!(temp.cache_files(), ["status_cli-agy-conv-1"]);
}

#[test]
fn stale_caches_are_pruned() {
    let temp = TempDir::new();
    fs::create_dir_all(temp.0.join("status_cli")).unwrap();
    let old = temp.0.join("status_cli").join("status_cli-old");
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
    for phase in ["stdin", "parse", "state_load", "features", "render", "state_save", "stdout"] {
        assert!(lines[0]["phases"][phase].as_u64().is_some(), "{phase}");
    }
    assert!(lines[1]["error"].as_str().unwrap().starts_with("key must be a string"));
}

#[test]
fn state_lives_in_a_dedicated_directory_created_on_demand() {
    let temp = TempDir::new();
    run(&temp, &claude("a", 0), None);
    assert!(temp.0.join("status_cli").join("status_cli-a").exists());
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

#[test]
fn every_claude_model_effort_and_mode_fits_and_reads_right() {
    let temp = TempDir::new();
    let models = [
        ("Opus 5.5", "Opus"),
        ("Sonnet 5.5", "Sonnet"),
        ("Haiku 5.5", "Haiku"),
        ("Fable 5.1", "Fable"),
        ("Opus 4.6 (1M context)", "Opus"),
    ];
    let efforts = [
        (None, ""),
        (Some("low"), " - Low"),
        (Some("medium"), " - Medium"),
        (Some("high"), " - High"),
        (Some("xhigh"), " - Xhigh"),
        (Some("max"), " - Max"),
    ];
    for (display, short) in models {
        for (level, label) in efforts {
            for fast in [false, true] {
                let effort = level.map_or(String::new(), |l| format!(r#","effort":{{"level":"{l}"}}"#));
                let input = format!(
                    r#"{{"session_id":"m","model":{{"display_name":"{display}"}}{effort},"fast_mode":{fast},
                        "rate_limits":{{"five_hour":{{"used_percentage":91,"resets_at":{}}}}},
                        "context_window":{{"used_percentage":12,"context_window_size":1000000}},"cost":{{"total_duration_ms":1}}}}"#,
                    now_secs() + 600
                );
                let expected = format!("{short}{label}{}", if fast { " · fast" } else { "" });
                for columns in [70, 100, 200] {
                    let out = run(&temp, &input, Some(columns));
                    assert!(out.text.starts_with(&expected), "{display} {level:?} fast={fast}: {:?}", out.text);
                    assert_eq!(out.width, columns - 8, "{display} {level:?} fast={fast} at {columns}");
                }
            }
        }
    }
}

#[test]
fn every_agy_model_and_mode_fits_and_reads_right() {
    let temp = TempDir::new();
    let models = [
        ("Gemini 3 Pro", "Gemini Pro"),
        ("Gemini 3 Flash", "Gemini Flash"),
        ("Claude Sonnet 4.5", "Claude Sonnet"),
        ("GPT-OSS 120B", "GPT-OSS"),
    ];
    let modes = [(None, ""), (Some("planning"), " - Planning"), (Some("fast"), " - Fast")];
    for (display, short) in models {
        for (mode, label) in modes {
            let mode = mode.map_or(String::new(), |m| format!(r#","execution_mode":"{m}""#));
            for columns in [70, 100, 200] {
                let input = format!(
                    r#"{{"product":"antigravity","conversation_id":"c","model":{{"id":"x","display_name":"{display}"}}{mode},
                        "quota":{{"x":{{"remaining_fraction":0.4,"reset_in_seconds":900}}}},"terminal_width":{columns}}}"#
                );
                let out = run(&temp, &input, None);
                assert!(out.text.starts_with(&format!("{short}{label}")), "{display} {mode}: {:?}", out.text);
                assert_eq!(out.width, columns - 8, "{display} {mode} at {columns}");
            }
        }
    }
}

fn setup(home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_status_cli"))
        .arg("setup")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("CLAUDE_CONFIG_DIR")
        .output()
        .unwrap()
}

#[test]
fn setup_merges_existing_settings_and_skips_missing_hosts() {
    let temp = TempDir::new();
    let claude = temp.0.join(".claude");
    fs::create_dir_all(&claude).unwrap();
    fs::write(claude.join("settings.json"), r#"{"model":"opus","statusLine":{"padding":2,"command":"old"}}"#).unwrap();
    let out = setup(&temp.0);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("Claude Code: configured") && stdout.contains("Antigravity CLI: not installed"),
        "{stdout}"
    );
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(claude.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["statusLine"]["command"], "status_cli");
    assert_eq!(settings["statusLine"]["padding"], 2);
    assert_eq!(settings["statusLine"]["refreshInterval"], 1);
    assert!(claude.join("settings.json.bak-status_cli").exists());
}

#[test]
fn setup_creates_settings_for_installed_hosts_and_refuses_broken_json() {
    let temp = TempDir::new();
    let agy = temp.0.join(".gemini").join("antigravity-cli");
    fs::create_dir_all(&agy).unwrap();
    let claude = temp.0.join(".claude");
    fs::create_dir_all(&claude).unwrap();
    fs::write(claude.join("settings.json"), "{ broken").unwrap();
    let out = setup(&temp.0);
    assert!(!out.status.success());
    assert_eq!(fs::read_to_string(claude.join("settings.json")).unwrap(), "{ broken");
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(agy.join("settings.json")).unwrap()).unwrap();
    assert_eq!(settings["statusLine"]["enabled"], true);
}

#[test]
fn version_flag_prints_the_crate_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_status_cli")).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), format!("status_cli {}", env!("CARGO_PKG_VERSION")));
}
