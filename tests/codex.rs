mod common;

use common::*;
use std::fs;
use std::process::Command;

#[test]
fn codex_mode_renders_the_latest_session_with_the_same_line() {
    let temp = TempDir::new();
    let day = temp.0.join("codex").join("sessions").join("2026").join("10").join("08");
    fs::create_dir_all(&day).unwrap();
    let resets = now_secs() + 3_600;
    let session = [
        r#"{"timestamp":"2020-01-01T00:00:00Z","type":"session_meta","payload":{"id":"x"}}"#.to_owned(),
        r#"{"type":"turn_context","payload":{"model":"gpt-6-luna","effort":"medium"}}"#.to_owned(),
        format!(
            r#"{{"type":"event_msg","payload":{{"type":"token_count","info":{{"model_context_window":200000,"last_token_usage":{{"total_tokens":50000}}}},"rate_limits":{{"primary":{{"used_percent":40.0,"resets_at":{resets}}},"secondary":{{"used_percent":12.0,"resets_at":{resets}}}}}}}}}"#
        ),
    ];
    fs::write(day.join("rollout-2026-10-08T02-52-12-01a11a11-be89-7771-8ae8-9a9d44e87f4d.jsonl"), session.join("\n"))
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_status_cli"))
        .args(["codex", "--once"])
        .env("CODEX_HOME", temp.0.join("codex"))
        .env("XDG_RUNTIME_DIR", &temp.0)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap().replace('\u{2800}', "");
    let text = text.trim();
    assert!(text.starts_with("gpt-6-luna - Medium ╱ sessão "), "{text:?}");
    assert!(text.contains(" 40% ↻ ") && text.contains("semana") && text.contains(" 12%"), "{text:?}");
    assert!(text.contains("contexto") && text.contains(" 25%"), "{text:?}");
    assert!(text.contains("tempo"), "{text:?}");
}

#[test]
fn codex_mode_reports_a_missing_session() {
    let temp = TempDir::new();
    let out = Command::new(env!("CARGO_BIN_EXE_status_cli"))
        .args(["codex", "--once"])
        .env("CODEX_HOME", &temp.0)
        .output()
        .unwrap();
    assert!(String::from_utf8(out.stdout).unwrap().contains("status_cli: no Codex session found"));
}
