use crate::common::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MARKER: &[u8] = b"\0status_cli-update-test";

fn asset() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "aarch64") => "status_cli-aarch64-pc-windows-msvc.exe",
        ("windows", _) => "status_cli-x86_64-pc-windows-msvc.exe",
        ("linux", "aarch64") => "status_cli-aarch64-unknown-linux-musl",
        ("linux", _) => "status_cli-x86_64-unknown-linux-musl",
        _ => "status_cli-universal-apple-darwin",
    }
}

fn url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') { format!("file://{path}") } else { format!("file:///{path}") }
}

fn publish(temp: &TempDir, tag: &str, corrupt: bool) -> PathBuf {
    let release = temp.0.join("release");
    fs::create_dir_all(&release).unwrap();
    let mut binary = fs::read(env!("CARGO_BIN_EXE_status_cli")).unwrap();
    binary.extend_from_slice(MARKER);
    let digest: String = Sha256::digest(&binary).iter().map(|b| format!("{b:02x}")).collect();
    let digest = if corrupt { "0".repeat(64) } else { digest };
    fs::write(release.join(asset()), &binary).unwrap();
    fs::write(release.join("SHA256SUMS"), format!("{digest}  {}\n", asset())).unwrap();
    fs::write(release.join("latest.json"), format!(r#"{{"tag_name":"{tag}","name":"status_cli {tag}"}}"#)).unwrap();
    release
}

fn installed(temp: &TempDir) -> PathBuf {
    let bin = temp.0.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let target = bin.join(if cfg!(windows) { "status_cli.exe" } else { "status_cli" });
    fs::copy(env!("CARGO_BIN_EXE_status_cli"), &target).unwrap();
    target
}

fn update(target: &Path, release: &Path) -> Output {
    Command::new(target)
        .arg("update")
        .env("STATUS_CLI_RELEASE_URL", url(&release.join("latest.json")))
        .env("STATUS_CLI_BASE_URL", url(release))
        .output()
        .unwrap()
}

#[test]
fn update_installs_a_newer_verified_release_in_place() {
    let temp = TempDir::new();
    let (release, target) = (publish(&temp, "v99.0.0", false), installed(&temp));
    let out = update(&target, &release);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains(&format!("{} -> 99.0.0", env!("CARGO_PKG_VERSION"))), "{stdout}");
    assert!(fs::read(&target).unwrap().ends_with(MARKER));
    assert!(!target.with_extension("new").exists());
}

#[test]
fn update_keeps_the_binary_when_already_current() {
    let temp = TempDir::new();
    let (release, target) = (publish(&temp, "v0.0.1", false), installed(&temp));
    let out = update(&target, &release);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("is up to date"));
    assert!(!fs::read(&target).unwrap().ends_with(MARKER));
}

#[test]
fn update_refuses_a_checksum_mismatch() {
    let temp = TempDir::new();
    let (release, target) = (publish(&temp, "v99.0.0", true), installed(&temp));
    let out = update(&target, &release);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("checksum mismatch"));
    assert!(!fs::read(&target).unwrap().ends_with(MARKER));
}
