use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::payload::{ContextWindow, Cost, Effort, Model, Payload, RateLimits, Window};
use crate::time::parse_utc_millis;

const TAIL_BYTES: u64 = 256 * 1024;
const RECENT_DAYS: usize = 2;
const RESCAN_MS: u64 = 5_000;

const CONTEXT_BASELINE_TOKENS: f64 = 12_000.0;
const LAUNCH_TOLERANCE_MS: u64 = 2_000;
const DAY_MINUTES: u64 = 1_440;
const LAUNCH_RESCAN_MS: u64 = 1_000;

#[derive(Default)]
pub struct Reader {
    home: Option<PathBuf>,
    launch: Option<(u64, PathBuf)>,
    pending: Option<Payload>,
    file: Option<PathBuf>,
    scanned_at: u64,
    stamp: Option<(u64, SystemTime)>,
    parsed: Option<(Payload, Option<u64>)>,
}

struct Meta {
    id: Option<String>,
    started: Option<u64>,
    cwd: Option<PathBuf>,
    subagent: bool,
}

impl Reader {
    pub fn latest() -> Self {
        Self { home: codex_home(), ..Self::default() }
    }

    pub fn for_launch(since: u64, cwd: PathBuf) -> Self {
        Self { home: codex_home(), launch: Some((since, cwd)), ..Self::default() }
    }

    pub fn read(&mut self, now: u64) -> Result<Payload, Box<dyn Error>> {
        let missing = self.file.as_deref().is_none_or(|file| !file.exists());
        let interval = if self.launch.is_some() { LAUNCH_RESCAN_MS } else { RESCAN_MS };
        let due = self.scanned_at == 0 || now.saturating_sub(self.scanned_at) >= interval;
        if due && (missing || self.launch.is_none()) {
            let sessions = self.home.as_deref().ok_or("Codex home directory not found")?.join("sessions");
            self.file = match &self.launch {
                Some((since, cwd)) => launched_session(&sessions, *since, cwd),
                None => latest_session(&sessions),
            };
            self.scanned_at = now;
        }
        let Some(file) = self.file.as_deref() else {
            let (Some((since, _)), Some(home)) = (&self.launch, &self.home) else {
                return Err("no Codex session found".into());
            };
            let mut payload = self.pending.get_or_insert_with(|| before_first_message(home)).clone();
            payload.cost = Some(Cost { total_duration_ms: Some(now.saturating_sub(*since)) });
            return Ok(payload);
        };
        let metadata = fs::metadata(file)?;
        let stamp = Some((metadata.len(), metadata.modified()?));
        if self.stamp != stamp || self.parsed.is_none() {
            let meta = session_meta(file);
            let mut payload = parse_session(file)?;
            payload.session_id = meta.as_ref().and_then(|meta| meta.id.clone());
            self.parsed = Some((payload, meta.and_then(|meta| meta.started)));
            self.stamp = stamp;
        }
        let (payload, started) = self.parsed.as_ref().ok_or("no Codex session found")?;
        let mut payload = payload.clone();
        payload.cost = started.map(|started| Cost { total_duration_ms: Some(now.saturating_sub(started)) });
        Ok(payload)
    }
}

fn parse_session(file: &Path) -> Result<Payload, Box<dyn Error>> {
    let mut payload = Payload { product: Some("codex".to_owned()), ..Payload::default() };
    for record in tail_records(file)?.iter().rev() {
        let body = &record["payload"];
        match (record["type"].as_str(), body["type"].as_str()) {
            (Some("turn_context"), _) if payload.model.is_none() => {
                payload.model = body["model"].as_str().map(|model| Model {
                    id: Some(model.to_owned()),
                    display_name: Some(model.to_owned()),
                    effort: None,
                });
                payload.effort = body["effort"].as_str().map(|level| Effort { level: level.to_owned() });
            }
            (Some("event_msg"), Some("token_count")) => {
                let limits = &body["rate_limits"];
                if payload.rate_limits.is_none()
                    && limits.is_object()
                    && matches!(limits["limit_id"].as_str(), None | Some("codex"))
                {
                    payload.rate_limits = Some(rate_limits(limits));
                }
                if payload.context_window.is_none() && body["info"].is_object() {
                    payload.context_window = Some(context(&body["info"]));
                }
            }
            _ => {}
        }
        if payload.model.is_some() && payload.rate_limits.is_some() && payload.context_window.is_some() {
            break;
        }
    }
    Ok(payload)
}

fn rate_limits(limits: &Value) -> RateLimits {
    let mut out = RateLimits::default();
    for (key, primary) in [("primary", true), ("secondary", false)] {
        let Some(window) = window(&limits[key]) else { continue };
        let short = limits[key]["window_minutes"].as_u64().map_or(primary, |minutes| minutes <= DAY_MINUTES);
        if short {
            out.five_hour = Some(window);
        } else {
            out.seven_day = Some(window);
        }
    }
    out
}

fn context(info: &Value) -> ContextWindow {
    let size = info["model_context_window"].as_f64();
    let used = info["last_token_usage"]["total_tokens"].as_f64();
    let used_percentage = used.zip(size).and_then(|(used, size)| {
        let effective = size - CONTEXT_BASELINE_TOKENS;
        let remaining = (effective - (used - CONTEXT_BASELINE_TOKENS).max(0.0)) / effective;
        (effective > 0.0).then(|| (1.0 - remaining.clamp(0.0, 1.0)) * 100.0)
    });
    ContextWindow { used_percentage, context_window_size: size }
}

pub fn command(args: &[String]) -> (String, Vec<String>) {
    if let Ok(program) = env::var("STATUS_CLI_CODEX") {
        return (program, args.to_vec());
    }
    if cfg!(windows) {
        let mut wrapped = vec!["/d".to_owned(), "/c".to_owned(), "codex".to_owned()];
        wrapped.extend_from_slice(args);
        ("cmd".to_owned(), wrapped)
    } else {
        ("codex".to_owned(), args.to_vec())
    }
}

fn codex_home() -> Option<PathBuf> {
    env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| {
        let user = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
        Some(PathBuf::from(user).join(".codex"))
    })
}

fn configured_model(home: &Path) -> Option<(String, Option<String>)> {
    let config: toml_edit::DocumentMut = fs::read_to_string(home.join("config.toml")).ok()?.parse().ok()?;
    let model = config.get("model")?.as_str()?.to_owned();
    Some((model, config.get("model_reasoning_effort").and_then(|e| e.as_str()).map(str::to_owned)))
}

fn before_first_message(home: &Path) -> Payload {
    let mut payload = latest_session(&home.join("sessions"))
        .and_then(|file| parse_session(&file).ok())
        .unwrap_or_else(|| Payload { product: Some("codex".to_owned()), ..Payload::default() });
    payload.context_window = None;
    if let Some((model, effort)) = configured_model(home) {
        payload.model = Some(Model { id: Some(model.clone()), display_name: Some(model), effort: None });
        payload.effort = effort.map(|level| Effort { level });
    }
    payload
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

fn latest_session(sessions: &Path) -> Option<PathBuf> {
    let days: Vec<PathBuf> = newest_children(sessions, RECENT_DAYS)
        .iter()
        .flat_map(|year| newest_children(year, RECENT_DAYS))
        .flat_map(|month| newest_children(&month, RECENT_DAYS))
        .collect();
    days.iter()
        .flat_map(|day| fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|path| Some((fs::metadata(&path).ok()?.modified().ok()?, path)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn tail_records(file: &Path) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut handle = File::open(file)?;
    let start = handle.metadata()?.len().saturating_sub(TAIL_BYTES);
    handle.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    handle.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if start > 0 {
        lines.next();
    }
    Ok(lines.filter_map(|line| serde_json::from_str(line).ok()).collect())
}

fn session_meta(file: &Path) -> Option<Meta> {
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
    })
}

fn launched_session(sessions: &Path, since: u64, cwd: &Path) -> Option<PathBuf> {
    let since = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(since.saturating_sub(LAUNCH_TOLERANCE_MS));
    let children = |dir: &Path| newest_children(dir, usize::MAX);
    children(sessions)
        .iter()
        .flat_map(|year| children(year))
        .flat_map(|month| children(&month))
        .flat_map(|day| fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .filter_map(|path| Some((fs::metadata(&path).ok()?.modified().ok()?, path)))
        .filter(|(modified, _)| *modified >= since)
        .filter(|(_, path)| {
            session_meta(path)
                .is_some_and(|meta| !meta.subagent && meta.cwd.as_deref().is_some_and(|dir| same_dir(dir, cwd)))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let normalize = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        let text = text.trim_end_matches('/').to_owned();
        if cfg!(windows) { text.to_lowercase() } else { text }
    };
    normalize(a) == normalize(b)
}

fn window(value: &Value) -> Option<Window> {
    let used = value["used_percent"].as_f64()?;
    Some(Window { used_percentage: Some(used), resets_at: value["resets_at"].as_f64() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_shows_the_configured_model_and_account_limits_before_the_first_message() {
        let home = std::env::temp_dir().join(format!("status_cli-codex-pending-{}", std::process::id()));
        let day = home.join("sessions").join("2026").join("10").join("07");
        fs::create_dir_all(&day).unwrap();
        fs::write(home.join("config.toml"), "model = \"gpt-6-luna\"\nmodel_reasoning_effort = \"high\"\n").unwrap();
        let previous = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"model_context_window":100,"last_token_usage":{"total_tokens":90}},"rate_limits":{"primary":{"used_percent":12.0,"window_minutes":300}}}}"#;
        fs::write(day.join("rollout-old.jsonl"), previous).unwrap();
        let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64 + 3_600_000;
        let mut reader = Reader { home: Some(home.clone()), launch: Some((since, home.clone())), ..Reader::default() };
        let payload = reader.read(since + 61_000).unwrap();
        assert_eq!(payload.model.and_then(|m| m.display_name).as_deref(), Some("gpt-6-luna"));
        assert_eq!(payload.effort.map(|e| e.level).as_deref(), Some("high"));
        assert_eq!(payload.rate_limits.and_then(|l| l.five_hour).and_then(|w| w.used_percentage), Some(12.0));
        assert!(payload.context_window.is_none());
        assert_eq!(payload.cost.and_then(|c| c.total_duration_ms), Some(61_000));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn windows_are_labelled_by_their_length() {
        let limits = serde_json::json!({
            "primary": {"used_percent": 70.0, "window_minutes": 10080, "resets_at": 1},
            "secondary": {"used_percent": 5.0, "window_minutes": 300, "resets_at": 2}
        });
        let out = rate_limits(&limits);
        assert_eq!(out.five_hour.unwrap().used_percentage, Some(5.0));
        assert_eq!(out.seven_day.unwrap().used_percentage, Some(70.0));
    }

    #[test]
    fn context_matches_the_codex_formula() {
        let info = serde_json::json!({"model_context_window": 200_000, "last_token_usage": {"total_tokens": 50_000}});
        assert_eq!(context(&info).used_percentage.map(f64::round), Some(20.0));
        let small = serde_json::json!({"model_context_window": 258_400, "last_token_usage": {"total_tokens": 8_000}});
        assert_eq!(context(&small).used_percentage, Some(0.0));
    }

    #[test]
    fn only_codex_limits_are_used() {
        let dir = std::env::temp_dir().join(format!("status_cli-codex-unit-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rollout.jsonl");
        let lines = [
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"limit_id":"codex","primary":{"used_percent":40.0,"window_minutes":300}}}}"#,
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"limit_id":"codex_other","primary":{"used_percent":99.0,"window_minutes":300}}}}"#,
        ];
        fs::write(&file, lines.join("\n")).unwrap();
        let payload = parse_session(&file).unwrap();
        assert_eq!(payload.rate_limits.unwrap().five_hour.unwrap().used_percentage, Some(40.0));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_limits_stay_absent() {
        assert!(window(&serde_json::json!(null)).is_none());
        assert_eq!(
            window(&serde_json::json!({"used_percent": 4.0, "resets_at": 10})).unwrap().used_percentage,
            Some(4.0)
        );
    }
}
