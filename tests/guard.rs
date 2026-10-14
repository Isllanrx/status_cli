mod common;

use common::*;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn guard(temp: &TempDir, host: &str, input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_status_cli"))
        .args(["guard", host])
        .env("XDG_RUNTIME_DIR", &temp.0)
        .env("STATUS_CLI_LANG", "en")
        .env("HOME", &temp.0)
        .env("USERPROFILE", &temp.0)
        .env_remove("CODEX_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

fn render(temp: &TempDir, session: &str, session_pct: u32) {
    let now = now_secs();
    run_raw(
        temp,
        &format!(
            r#"{{"session_id":"{session}","rate_limits":{{"five_hour":{{"used_percentage":{session_pct},"resets_at":{}}},
                "seven_day":{{"used_percentage":31,"resets_at":{}}}}}}}"#,
            now + 1800,
            now + 300_000
        ),
        &[],
    );
}

#[test]
fn claude_prompt_gets_usage_and_time_to_reset_from_the_status_line_state() {
    let temp = TempDir::new();
    render(&temp, "s1", 40);
    let out: Value =
        serde_json::from_str(&guard(&temp, "claude", r#"{"session_id":"s1","hook_event_name":"UserPromptSubmit"}"#))
            .unwrap();
    let context = out["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(context.starts_with("status_cli: session 40% (resets in 29m"), "{context}");
    assert!(context.contains("week 31% (resets in 3d"), "{context}");
}

#[test]
fn near_exhaustion_tells_the_agent_to_stop_and_blocks_subagents() {
    let temp = TempDir::new();
    render(&temp, "s2", 96);
    let prompt: Value =
        serde_json::from_str(&guard(&temp, "claude", r#"{"session_id":"s2","hook_event_name":"UserPromptSubmit"}"#))
            .unwrap();
    assert!(prompt["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("do not start new tasks"));
    let tool: Value = serde_json::from_str(&guard(
        &temp,
        "claude",
        r#"{"session_id":"s2","hook_event_name":"PreToolUse","tool_name":"Agent"}"#,
    ))
    .unwrap();
    assert_eq!(tool["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[test]
fn unknown_sessions_and_bad_input_stay_silent() {
    let temp = TempDir::new();
    assert_eq!(guard(&temp, "claude", r#"{"session_id":"none","hook_event_name":"UserPromptSubmit"}"#), "");
    assert_eq!(guard(&temp, "claude", "not json"), "");
    assert_eq!(guard(&temp, "agy", "not json"), "{}");
}

#[test]
fn setup_registers_the_guard_in_every_host_and_keeps_other_hooks() {
    let temp = TempDir::new();
    let claude = temp.0.join(".claude");
    let codex = temp.0.join(".codex");
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(temp.0.join(".gemini").join("antigravity-cli")).unwrap();
    fs::write(
        codex.join("hooks.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk"}]}]}}"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_status_cli"))
        .arg("setup")
        .env("HOME", &temp.0)
        .env("USERPROFILE", &temp.0)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("/hooks in Codex"), "{stdout}");
    let read =
        |path: std::path::PathBuf| -> Value { serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap() };
    let settings = read(claude.join("settings.json"));
    let command = settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"].as_str().unwrap();
    assert!(command.ends_with("guard claude"), "{command}");
    assert_eq!(settings["hooks"]["PreToolUse"][0]["matcher"], "Agent|Task|Workflow");
    let codex_hooks = read(codex.join("hooks.json"));
    assert_eq!(codex_hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "rtk");
    assert!(codex_hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"].as_str().unwrap().ends_with("guard codex"));
    let plugin = temp.0.join(".gemini").join("config").join("plugins").join("status_cli");
    assert_eq!(read(plugin.join("plugin.json"))["name"], "status_cli");
    assert!(read(plugin.join("hooks.json"))["status_cli-guard"]["PreInvocation"].is_array());
}
