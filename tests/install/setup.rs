use crate::common::*;
use std::fs;
use std::process::Command;

static SETUP: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn setup(home: &std::path::Path) -> std::process::Output {
    let _serial = SETUP.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    setup_unlocked(home)
}

fn setup_unlocked(home: &std::path::Path) -> std::process::Output {
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
fn setup_turns_off_the_native_codex_line_without_losing_settings() {
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
    assert!(config.contains("status_line = []"), "{config}");
    assert!(codex.join("config.toml.bak-status_cli").exists());
}

#[test]
fn setup_removes_the_old_codex_stt_shortcut() {
    let temp = TempDir::new();
    let alias = std::path::Path::new(env!("CARGO_BIN_EXE_status_cli"))
        .with_file_name(format!("codex-stt{}", std::env::consts::EXE_SUFFIX));
    let _serial = SETUP.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = fs::remove_file(&alias);
    fs::write(&alias, b"old").unwrap();
    let out = setup_unlocked(&temp.0);
    assert!(String::from_utf8(out.stdout).unwrap().contains("codex-stt: removed"));
    assert!(!alias.exists());
}

#[test]
fn a_binary_named_codex_runs_the_wrapper_and_runs_codex_directly_when_nested() {
    let temp = TempDir::new();
    let shim = temp.0.join("shim");
    fs::create_dir_all(&shim).unwrap();
    let codex = shim.join(format!("codex{}", std::env::consts::EXE_SUFFIX));
    fs::copy(env!("CARGO_BIN_EXE_status_cli"), &codex).unwrap();
    let once = Command::new(&codex).arg("--once").env("CODEX_HOME", &temp.0).env("PATH", &shim).output().unwrap();
    assert!(String::from_utf8(once.stdout).unwrap().contains("status_cli: no Codex session found"));
    let (program, args) =
        if cfg!(windows) { ("cmd", &["/d", "/c", "exit 7"][..]) } else { ("sh", &["-c", "exit 7"][..]) };
    for marker in ["STATUS_CLI_IN_CODEX", "STATUS_CLI_SHIM"] {
        let nested =
            Command::new(&codex).args(args).env(marker, &shim).env("STATUS_CLI_CODEX", program).output().unwrap();
        assert_eq!(nested.status.code(), Some(7), "{marker}");
    }
}
