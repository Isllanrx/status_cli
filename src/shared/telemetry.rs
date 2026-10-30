use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::cache_dir;

const MAX_LOG_BYTES: u64 = 1 << 20;
pub const ERROR_LOG: &str = "status_cli.log";

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub struct Timer {
    last: Instant,
    phases: Vec<(&'static str, u64)>,
}

impl Timer {
    pub fn start() -> Self {
        Self { last: Instant::now(), phases: Vec::with_capacity(8) }
    }

    pub fn mark(&mut self, phase: &'static str) {
        let now = Instant::now();
        self.phases.push((phase, now.duration_since(self.last).as_micros() as u64));
        self.last = now;
    }
}

pub struct Event {
    pub now: u64,
    pub host: Option<&'static str>,
    pub session: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub line: String,
    pub error: Option<String>,
}

pub fn record(path: &Path, event: Event, timer: &Timer) {
    let phases: Map<String, Value> = timer.phases.iter().map(|(name, us)| (name.to_string(), json!(us))).collect();
    append(
        path,
        &json!({
            "ts": event.now,
            "us": timer.phases.iter().map(|(_, us)| us).sum::<u64>(),
            "phases": phases,
            "host": event.host,
            "session": event.session,
            "model": event.model,
            "effort": event.effort,
            "line": event.line,
            "error": event.error,
        }),
    );
}

pub fn error_log() -> PathBuf {
    cache_dir().join(ERROR_LOG)
}

pub fn error(scope: &str, err: impl Display) {
    let message = format!("{scope}: {err}");
    if let Ok(mut last) = LAST_ERROR.lock() {
        if last.as_deref() == Some(message.as_str()) {
            return;
        }
        *last = Some(message.clone());
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let dir = cache_dir();
    if fs::create_dir_all(&dir).is_ok() {
        append(
            &dir.join(ERROR_LOG),
            &json!({
                "ts": now,
                "pid": std::process::id(),
                "version": env!("CARGO_PKG_VERSION"),
                "scope": scope,
                "error": err.to_string(),
            }),
        );
    }
}

pub fn logged<T, E: Display>(scope: &str, result: Result<T, E>) -> Option<T> {
    result.map_err(|err| error(scope, err)).ok()
}

pub fn read_optional(scope: &str, path: &Path) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            error(scope, format_args!("{}: {err}", path.display()));
            None
        }
    }
}

pub fn read_json<T: DeserializeOwned>(scope: &str, path: &Path) -> Option<T> {
    let bytes = read_optional(scope, path)?;
    let json = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    logged(scope, serde_json::from_slice(json).map_err(|err| format!("{}: {err}", path.display())))
}

pub fn open_optional(scope: &str, path: &Path) -> Option<File> {
    match File::open(path) {
        Ok(file) => Some(file),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => {
            error(scope, format_args!("{}: {err}", path.display()));
            None
        }
    }
}

pub fn remove(scope: &str, path: &Path) {
    if let Err(err) = fs::remove_file(path)
        && err.kind() != ErrorKind::NotFound
    {
        error(scope, format_args!("{}: {err}", path.display()));
    }
}

pub fn recent_errors(limit: usize) -> Vec<String> {
    let text = fs::read_to_string(error_log()).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(limit)..].iter().map(|line| line.to_string()).collect()
}

fn append(path: &Path, line: &Value) {
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let rotated = path.with_extension(format!("{}.1", path.extension().and_then(|e| e.to_str()).unwrap_or("log")));
        if fs::rename(path, &rotated).is_err() {
            let _ = fs::remove_file(path);
        }
    }
    let written = OpenOptions::new().create(true).append(true).open(path).and_then(|mut file| writeln!(file, "{line}"));
    if let Err(err) = written {
        eprintln!("status_cli: could not write {}: {err}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_are_not_errors_but_broken_json_is() {
        let dir = std::env::temp_dir().join(format!("status_cli-log-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_json::<Value>("test", &dir.join("absent.json")), None);
        fs::write(dir.join("broken.json"), b"{").unwrap();
        assert_eq!(read_json::<Value>("test", &dir.join("broken.json")), None);
        fs::write(dir.join("bom.json"), b"\xEF\xBB\xBF{\"a\":1}").unwrap();
        assert_eq!(read_json::<Value>("test", &dir.join("bom.json")), Some(json!({"a": 1})));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_log_rotates_instead_of_growing_forever() {
        let dir = std::env::temp_dir().join(format!("status_cli-rotate-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join(ERROR_LOG);
        fs::write(&log, vec![b'x'; MAX_LOG_BYTES as usize + 1]).unwrap();
        append(&log, &json!({"scope": "test"}));
        assert!(fs::metadata(dir.join("status_cli.log.1")).unwrap().len() > MAX_LOG_BYTES);
        assert_eq!(fs::read_to_string(&log).unwrap(), "{\"scope\":\"test\"}\n");
        fs::remove_dir_all(&dir).unwrap();
    }
}
