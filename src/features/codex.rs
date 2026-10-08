use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use serde_json::Value;

use crate::features::setup::shell_quote;
use crate::payload::{ContextWindow, Cost, Effort, Model, Payload, RateLimits, Window};

const TAIL_BYTES: u64 = 256 * 1024;
const RECENT_DAYS: usize = 2;
const RESCAN_MS: u64 = 5_000;

#[derive(Default)]
pub struct Reader {
    file: Option<PathBuf>,
    scanned_at: u64,
    stamp: Option<(u64, SystemTime)>,
    parsed: Option<(Payload, Option<u64>)>,
}

impl Reader {
    pub fn read(&mut self, now: u64) -> Result<Payload, Box<dyn Error>> {
        if self.file.is_none() || now.saturating_sub(self.scanned_at) >= RESCAN_MS {
            self.file = latest_session(&sessions_dir().ok_or("Codex home directory not found")?);
            self.scanned_at = now;
        }
        let file = self.file.as_deref().ok_or("no Codex session found")?;
        let metadata = fs::metadata(file)?;
        let stamp = Some((metadata.len(), metadata.modified()?));
        if self.stamp != stamp || self.parsed.is_none() {
            self.parsed = Some((parse_session(file)?, first_timestamp(file)));
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
            (Some("event_msg"), Some("token_count")) if payload.rate_limits.is_none() => {
                let limits = &body["rate_limits"];
                payload.rate_limits =
                    Some(RateLimits { five_hour: window(&limits["primary"]), seven_day: window(&limits["secondary"]) });
                let size = body["info"]["model_context_window"].as_f64();
                let used = body["info"]["last_token_usage"]["total_tokens"].as_f64();
                payload.context_window = Some(ContextWindow {
                    used_percentage: used
                        .zip(size)
                        .filter(|(_, size)| *size > 0.0)
                        .map(|(used, size)| used / size * 100.0),
                    context_window_size: size,
                });
            }
            _ => {}
        }
        if payload.model.is_some() && payload.rate_limits.is_some() {
            break;
        }
    }
    payload.session_id = file.file_stem().and_then(|stem| stem.to_str()).map(|stem| {
        let start = stem.len().saturating_sub(36);
        stem[start..].to_owned()
    });
    Ok(payload)
}

pub fn launch(codex_args: &[String], marker: &Path) -> Result<i32, Box<dyn Error>> {
    if let Some(dir) = marker.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(marker, std::process::id().to_string())?;
    if let Err(err) = split_pane(&env::current_exe()?, marker) {
        eprintln!("status_cli: {err}; run `status_cli codex --watch` in another pane");
    }
    let status = codex_command(codex_args).status();
    let _ = fs::remove_file(marker);
    Ok(status?.code().unwrap_or(1))
}

fn split_pane(exe: &Path, marker: &Path) -> Result<(), Box<dyn Error>> {
    let status = if env::var_os("TMUX").is_some() {
        let watcher = format!(
            "{} codex --watch --until {}",
            shell_quote(&exe.to_string_lossy()),
            shell_quote(&marker.to_string_lossy())
        );
        Command::new("tmux").args(["split-window", "-v", "-d", "-l", "2", &watcher]).status()?
    } else if env::var_os("WT_SESSION").is_some() {
        Command::new("wt")
            .args(["-w", "0", "split-pane", "-H", "-s", "0.1"])
            .arg(exe)
            .args(["codex", "--watch", "--until"])
            .arg(marker)
            .args([";", "move-focus", "up"])
            .status()?
    } else {
        return Err("split panes need Windows Terminal or tmux".into());
    };
    if status.success() { Ok(()) } else { Err("could not open the status pane".into()) }
}

fn codex_command(args: &[String]) -> Command {
    let mut command = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/d", "/c", "codex"]);
        cmd
    } else {
        Command::new("codex")
    };
    command.args(args);
    command
}

fn sessions_dir() -> Option<PathBuf> {
    let home = env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| {
        let user = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
        Some(PathBuf::from(user).join(".codex"))
    })?;
    Some(home.join("sessions"))
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

fn first_timestamp(file: &Path) -> Option<u64> {
    let mut line = String::new();
    BufReader::new(File::open(file).ok()?).read_line(&mut line).ok()?;
    let record: Value = serde_json::from_str(&line).ok()?;
    let timestamp = record["timestamp"].as_str().or_else(|| record["payload"]["timestamp"].as_str())?;
    parse_utc_millis(timestamp)
}

fn window(value: &Value) -> Option<Window> {
    let used = value["used_percent"].as_f64()?;
    Some(Window { used_percentage: Some(used), resets_at: value["resets_at"].as_f64() })
}

fn parse_utc_millis(timestamp: &str) -> Option<u64> {
    let field = |range: std::ops::Range<usize>| timestamp.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
    let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_utc_timestamps() {
        assert_eq!(parse_utc_millis("1970-01-02T00:00:00Z"), Some(86_400_000));
        assert_eq!(parse_utc_millis("2000-03-01T00:00:00.123Z"), Some(951_868_800_000));
        assert_eq!(parse_utc_millis("2026-10-08T05:52:12Z"), Some(1_791_438_732_000));
        assert_eq!(parse_utc_millis("garbage"), None);
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
