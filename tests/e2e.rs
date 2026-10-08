mod common;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use common::*;

fn with_home(command: &mut Command, home: &Path) {
    command
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_RUNTIME_DIR", home)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env("STATUS_CLI_LANG", "pt")
        .env("NO_COLOR", "1")
        .env("COLUMNS", "120");
}

fn host_shell(configured: &str, payload: &str, home: &Path) -> String {
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_status_cli")).parent().unwrap().to_path_buf();
    let path =
        std::env::join_paths(std::iter::once(bin_dir).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())))
            .unwrap();
    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/d", "/c", configured]);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", configured]);
        cmd
    };
    with_home(&mut command, home);
    let mut child = command.env("PATH", path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(payload.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8(output.stdout).unwrap().replace('\u{2800}', "")
}

fn configured_command(settings: &Path) -> String {
    let settings: serde_json::Value = serde_json::from_str(&fs::read_to_string(settings).unwrap()).unwrap();
    settings["statusLine"]["command"].as_str().unwrap().to_owned()
}

#[test]
fn setup_then_each_host_runs_the_configured_command_through_its_shell() {
    let temp = TempDir::new();
    let claude = temp.0.join(".claude");
    let agy = temp.0.join(".gemini").join("antigravity-cli");
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&agy).unwrap();

    let mut setup = Command::new(env!("CARGO_BIN_EXE_status_cli"));
    with_home(&mut setup, &temp.0);
    assert!(setup.arg("setup").status().unwrap().success());

    let claude_line = host_shell(
        &configured_command(&claude.join("settings.json")),
        r#"{"session_id":"e2e","model":{"display_name":"Opus 5.5"},"effort":{"level":"high"},"cost":{"total_duration_ms":61000}}"#,
        &temp.0,
    );
    assert!(claude_line.trim_start().starts_with("Opus - High ╱ sessão"), "{claude_line:?}");
    assert!(claude_line.trim_end().ends_with("00:01:01"), "{claude_line:?}");

    let agy_line = host_shell(
        &configured_command(&agy.join("settings.json")),
        r#"{"product":"antigravity","conversation_id":"e2e","model":{"display_name":"Gemini 3.8 Flash (High)","effort":"high"},
            "quota":{"gemini-5h":{"remaining_fraction":0.5,"reset_in_seconds":600}},"terminal_width":120}"#,
        &temp.0,
    );
    assert!(agy_line.trim_start().starts_with("Gemini Flash - High ╱ sessão"), "{agy_line:?}");

    let mut doctor = Command::new(env!("CARGO_BIN_EXE_status_cli"));
    with_home(&mut doctor, &temp.0);
    let report = doctor.arg("doctor").output().unwrap();
    let text = String::from_utf8(report.stdout).unwrap();
    assert!(text.contains("ok      Claude Code") && text.contains("ok      Antigravity CLI"), "{text}");
    assert!(text.contains("skip    Codex CLI is not installed"), "{text}");
}

#[test]
fn doctor_fails_when_no_host_is_configured() {
    let temp = TempDir::new();
    let mut doctor = Command::new(env!("CARGO_BIN_EXE_status_cli"));
    with_home(&mut doctor, &temp.0);
    let report = doctor.arg("doctor").output().unwrap();
    assert!(!report.status.success());
    assert!(String::from_utf8(report.stdout).unwrap().contains("missing no host is configured"));
}
