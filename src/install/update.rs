use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use sha2::{Digest, Sha256};

const REPO: &str = "Isllanrx/status_cli";
const MAX_SECONDS: &str = "120";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub fn run() -> Result<String, String> {
    let current = env!("CARGO_PKG_VERSION");
    let release_url = env::var("STATUS_CLI_RELEASE_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest"));
    let release: Release = serde_json::from_slice(&fetch(&release_url)?)
        .map_err(|err| format!("unreadable release from {release_url}: {err}"))?;
    let latest = release.tag_name.trim_start_matches('v');
    let latest_version = version(latest).ok_or_else(|| format!("unreadable release tag {}", release.tag_name))?;
    if version(current).is_some_and(|current| latest_version <= current) {
        return Ok(format!("status_cli {current} is up to date (latest release {latest})"));
    }
    let asset = asset(env::consts::OS, env::consts::ARCH)
        .ok_or_else(|| format!("no release asset for {} {}", env::consts::OS, env::consts::ARCH))?;
    let base = env::var("STATUS_CLI_BASE_URL")
        .unwrap_or_else(|_| format!("https://github.com/{REPO}/releases/download/{}", release.tag_name));
    let sums = String::from_utf8_lossy(&fetch(&format!("{base}/SHA256SUMS"))?).into_owned();
    let expected = checksum(&sums, asset).ok_or_else(|| format!("no checksum published for {asset}"))?;
    let binary = fetch(&format!("{base}/{asset}"))?;
    if hex(&Sha256::digest(&binary)) != expected.to_ascii_lowercase() {
        return Err(format!("checksum mismatch for {asset}"));
    }
    let target = env::current_exe().map_err(|err| format!("cannot locate the running binary: {err}"))?;
    replace(&target, &binary)?;
    Ok(format!("Updated status_cli {current} -> {latest} at {}", target.display()))
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let out = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "--max-time", MAX_SECONDS, url])
        .output()
        .map_err(|err| format!("curl is required to update: {err}"))?;
    if !out.status.success() {
        return Err(format!("download failed: {url}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

fn version(text: &str) -> Option<Vec<u64>> {
    text.split(['-', '+']).next()?.split('.').map(|part| part.parse().ok()).collect()
}

fn asset(os: &str, arch: &str) -> Option<&'static str> {
    Some(match (os, arch) {
        ("windows", "x86_64") => "status_cli-x86_64-pc-windows-msvc.exe",
        ("windows", "aarch64") => "status_cli-aarch64-pc-windows-msvc.exe",
        ("linux", "x86_64") => "status_cli-x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "status_cli-aarch64-unknown-linux-musl",
        ("macos", _) => "status_cli-universal-apple-darwin",
        _ => return None,
    })
}

fn checksum<'a>(sums: &'a str, asset: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        (fields.next()?.trim_start_matches('*') == asset).then_some(digest)
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn replace(target: &Path, binary: &[u8]) -> Result<(), String> {
    let staged = target.with_extension("new");
    let old = target.with_extension("old");
    let _ = fs::remove_file(&old);
    fs::write(&staged, binary).map_err(|err| format!("cannot write {}: {err}", staged.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("cannot mark {} executable: {err}", staged.display()))?;
    }
    let runs = Command::new(&staged).arg("--version").output().is_ok_and(|out| out.status.success());
    if !runs {
        let _ = fs::remove_file(&staged);
        return Err(format!("downloaded binary does not run on this system: {}", staged.display()));
    }
    if cfg!(windows) {
        fs::rename(target, &old).map_err(|err| format!("cannot move {} aside: {err}", target.display()))?;
    }
    if let Err(err) = fs::rename(&staged, target) {
        if cfg!(windows) {
            let _ = fs::rename(&old, target);
        }
        let _ = fs::remove_file(&staged);
        return Err(format!("cannot install {}: {err}", target.display()));
    }
    let _ = fs::remove_file(&old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(version("1.10.0") > version("1.9.3"));
        assert_eq!(version("1.2.0-rc.1"), Some(vec![1, 2, 0]));
        assert_eq!(version("latest"), None);
    }

    #[test]
    fn picks_the_published_asset_per_platform() {
        assert_eq!(asset("windows", "x86_64"), Some("status_cli-x86_64-pc-windows-msvc.exe"));
        assert_eq!(asset("linux", "aarch64"), Some("status_cli-aarch64-unknown-linux-musl"));
        assert_eq!(asset("macos", "aarch64"), Some("status_cli-universal-apple-darwin"));
        assert_eq!(asset("freebsd", "x86_64"), None);
    }

    #[test]
    fn finds_the_checksum_of_the_asset() {
        let sums = "aa  status_cli-x86_64-unknown-linux-musl\nBB *status_cli-x86_64-pc-windows-msvc.exe\n";
        assert_eq!(checksum(sums, "status_cli-x86_64-pc-windows-msvc.exe"), Some("BB"));
        assert_eq!(checksum(sums, "status_cli-universal-apple-darwin"), None);
    }
}
