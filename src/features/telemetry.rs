use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use serde_json::json;

const MAX_LOG_BYTES: u64 = 1 << 20;

pub struct Event {
    pub now: u64,
    pub took: Duration,
    pub host: Option<&'static str>,
    pub session: Option<String>,
    pub error: Option<String>,
}

pub fn record(path: &Path, event: Event) {
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(path, path.with_extension("1"));
    }
    let line = json!({
        "ts": event.now,
        "us": event.took.as_micros() as u64,
        "host": event.host,
        "session": event.session,
        "error": event.error,
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}
