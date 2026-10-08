mod common;

use common::*;
use std::fs;
use std::process::Command;

fn setup(home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_status_cli"))
        .arg("setup")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
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
    let command = settings["statusLine"]["command"].as_str().unwrap();
    if cfg!(windows) {
        assert_eq!(command, "status_cli");
    } else {
        let exe = std::path::Path::new(env!("CARGO_BIN_EXE_status_cli")).canonicalize().unwrap();
        assert_eq!(command.trim_matches('\''), exe.to_str().unwrap());
    }
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

#[test]
fn setup_configures_codex_natively_without_losing_settings() {
    let temp = TempDir::new();
    let codex = temp.0.join(".codex");
    fs::create_dir_all(&codex).unwrap();
    fs::write(codex.join("config.toml"), "model = \"gpt-6\"\n\n[tui]\nscreen_reader_detection_done = true\n").unwrap();
    let out = setup(&temp.0);
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("Codex CLI: configured"));
    let config = fs::read_to_string(codex.join("config.toml")).unwrap();
    assert!(config.starts_with("model = \"gpt-6\""), "{config}");
    assert!(config.contains("screen_reader_detection_done = true"));
    assert!(config.contains("\"five-hour-limit\""));
    assert!(codex.join("config.toml.bak-status_cli").exists());
}
