use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use serde_json::{Map, Value, json};

const MAX_LOG_BYTES: u64 = 1 << 20;

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
    pub error: Option<String>,
}

pub fn record(path: &Path, event: Event, timer: &Timer) {
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(path, path.with_extension("1"));
    }
    let phases: Map<String, Value> = timer.phases.iter().map(|(name, us)| (name.to_string(), json!(us))).collect();
    let line = json!({
        "ts": event.now,
        "us": timer.phases.iter().map(|(_, us)| us).sum::<u64>(),
        "phases": phases,
        "host": event.host,
        "session": event.session,
        "error": event.error,
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}
