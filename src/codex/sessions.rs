use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::shared::time::parse_utc_millis;

const RECENT_DAYS: usize = 2;
const LAUNCH_TOLERANCE_MS: u64 = 2_000;
const TERMINAL_ORIGINATORS: [&str; 2] = ["codex-tui", "codex_exec"];

pub struct Meta {
    pub id: Option<String>,
    pub started: Option<u64>,
    cwd: Option<PathBuf>,
    subagent: bool,
    source: Value,
    originator: Option<String>,
}

impl Meta {
    fn started_by_cli(&self) -> bool {
        let terminal = self.originator.as_deref().is_some_and(|name| TERMINAL_ORIGINATORS.contains(&name));
        !self.subagent
            && match &self.source {
                Value::Null => true,
                Value::String(source) => matches!(source.as_str(), "cli" | "exec") || terminal,
                _ => false,
            }
    }
}

pub fn meta(file: &Path) -> Option<Meta> {
    let mut line = String::new();
    BufReader::new(File::open(file).ok()?).read_line(&mut line).ok()?;
    let record: Value = serde_json::from_str(&line).ok()?;
    let meta = &record["payload"];
    let timestamp = meta["timestamp"].as_str().or_else(|| record["timestamp"].as_str());
    Some(Meta {
        id: meta["id"].as_str().map(str::to_owned),
        started: timestamp.and_then(parse_utc_millis),
        cwd: meta["cwd"].as_str().map(PathBuf::from),
        subagent: !meta["parent_thread_id"].is_null(),
        source: meta["source"].clone(),
        originator: meta["originator"].as_str().map(str::to_owned),
    })
}

pub fn latest(sessions: &Path) -> Option<PathBuf> {
    let days: Vec<PathBuf> = newest_children(sessions, RECENT_DAYS)
        .iter()
        .flat_map(|year| newest_children(year, RECENT_DAYS))
        .flat_map(|month| newest_children(&month, RECENT_DAYS))
        .collect();
    days.iter()
        .flat_map(|day| fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .filter(|path| meta(path).is_none_or(|meta| meta.started_by_cli()))
        .filter_map(|path| Some((fs::metadata(&path).ok()?.modified().ok()?, path)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

pub fn launched(sessions: &Path, since: u64, cwd: &Path) -> Option<(PathBuf, bool)> {
    let since = since.saturating_sub(LAUNCH_TOLERANCE_MS);
    let since_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(since);
    let children = |dir: &Path| newest_children(dir, usize::MAX);
    children(sessions)
        .iter()
        .flat_map(|year| children(year))
        .flat_map(|month| children(&month))
        .flat_map(|day| fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|path| Some((fs::metadata(&path).ok()?.modified().ok()?, path)))
        .filter(|(modified, _)| *modified >= since_time)
        .filter_map(|(modified, path)| {
            let meta = meta(&path)?;
            let here = meta.cwd.as_deref().is_some_and(|dir| same_dir(dir, cwd));
            let fresh = meta.started.is_some_and(|started| started >= since);
            (here && meta.started_by_cli()).then_some(((fresh, modified), path))
        })
        .max_by_key(|(key, _)| *key)
        .map(|((fresh, _), path)| (path, fresh))
}

pub fn same_dir(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        let text = text.trim_end_matches('/').to_owned();
        if cfg!(windows) { text.to_lowercase() } else { text }
    };
    normalize(a) == normalize(b)
}

fn newest_children(dir: &Path, count: usize) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort_unstable_by(|a, b| b.cmp(a));
    dirs.truncate(count);
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_sessions_are_read_and_app_or_child_sessions_are_not() {
        let dir = std::env::temp_dir().join(format!("status_cli-codex-source-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let read = |name: &str, payload: &str| {
            let file = dir.join(name);
            fs::write(&file, format!(r#"{{"type":"session_meta","payload":{payload}}}"#)).unwrap();
            meta(&file).unwrap().started_by_cli()
        };
        assert!(read("cli.jsonl", r#"{"source":"cli"}"#));
        assert!(read("exec.jsonl", r#"{"source":"exec"}"#));
        assert!(read("old.jsonl", "{}"));
        assert!(read("daemon.jsonl", r#"{"source":"vscode","originator":"codex-tui"}"#));
        assert!(!read("desktop.jsonl", r#"{"source":"vscode","originator":"Codex Desktop"}"#));
        assert!(!read("remote.jsonl", r#"{"source":"vscode","originator":"codex_cli_rs"}"#));
        assert!(!read("child.jsonl", r#"{"source":{"subagent":{}},"parent_thread_id":"p"}"#));
        assert!(!read("guardian.jsonl", r#"{"source":{"internal":"guardian"},"originator":"codex-tui"}"#));
        fs::remove_dir_all(&dir).unwrap();
    }
}
