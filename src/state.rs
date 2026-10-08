use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub const FILE_PREFIX: &str = "status_cli-";
const TTL: Duration = Duration::from_secs(7 * 86_400);
const TRANSITION_MS: u64 = 3_000;
const CLOCK_DRIFT_MS: u64 = 2_000;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
struct Snapshot {
    clock: Option<Seen>,
    gauges: BTreeMap<String, Trend>,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
struct Seen {
    ms: u64,
    at: u64,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
struct Trend {
    from: f64,
    to: f64,
    changed_at: u64,
}

pub struct Motion {
    pub shown: f64,
    pub growing: bool,
}

impl Trend {
    fn shown(&self, now: u64) -> f64 {
        let t = (now.saturating_sub(self.changed_at) as f64 / TRANSITION_MS as f64).min(1.0);
        let eased = 1.0 - (1.0 - t).powi(2);
        self.from + (self.to - self.from) * eased
    }
}

pub struct Session {
    file: Option<PathBuf>,
    saved: Snapshot,
    current: Snapshot,
    now: u64,
}

impl Session {
    pub fn open(dir: &Path, key: Option<&str>, now: u64) -> Self {
        let file = key.map(|k| dir.join(format!("{FILE_PREFIX}{k}")));
        let saved = file.as_deref().and_then(load);
        if file.is_some() && saved.is_none() {
            prune(dir);
        }
        let saved = saved.unwrap_or_default();
        let current = Snapshot { clock: saved.clock, gauges: saved.gauges.clone() };
        Self { file, saved, current, now }
    }

    #[cfg(test)]
    pub fn detached(now: u64) -> Self {
        Self { file: None, saved: Snapshot::default(), current: Snapshot::default(), now }
    }

    pub fn elapsed(&mut self, base_ms: u64) -> u64 {
        match self.current.clock {
            Some(seen) if seen.ms == base_ms => base_ms + self.now.saturating_sub(seen.at),
            Some(seen) if (seen.ms + self.now.saturating_sub(seen.at)).abs_diff(base_ms) < CLOCK_DRIFT_MS => base_ms,
            _ => {
                self.current.clock = Some(Seen { ms: base_ms, at: self.now });
                base_ms
            }
        }
    }

    pub fn track(&mut self, label: &str, pct: Option<f64>) -> Option<Motion> {
        let pct = pct?;
        let now = self.now;
        let trend = self.current.gauges.entry(label.to_owned()).or_insert(Trend { from: pct, to: pct, changed_at: 0 });
        if pct.round() != trend.to.round() {
            *trend = Trend { from: trend.shown(now), to: pct, changed_at: now };
        } else {
            let shift = pct - trend.to;
            trend.from += shift;
            trend.to = pct;
        }
        let shown = trend.shown(now);
        Some(Motion { shown, growing: trend.to > trend.from && shown < trend.to })
    }

    pub fn advance(&mut self, now: u64) {
        self.now = now;
    }

    pub fn save(&mut self) {
        let Some(file) = &self.file else { return };
        if self.current == self.saved {
            return;
        }
        let Ok(json) = serde_json::to_string(&self.current) else { return };
        let written = write_atomic(file, json.as_bytes()).or_else(|_| {
            fs::create_dir_all(file.parent().ok_or(io::ErrorKind::NotFound)?)?;
            write_atomic(file, json.as_bytes())
        });
        if written.is_ok() {
            self.saved = self.current.clone();
        }
    }
}

fn write_atomic(file: &Path, bytes: &[u8]) -> io::Result<()> {
    let staging = file.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&staging, bytes)?;
    let renamed = fs::rename(&staging, file).or_else(|_| fs::rename(&staging, file));
    if renamed.is_err() {
        let _ = fs::remove_file(&staging);
    }
    renamed
}

fn load(file: &Path) -> Option<Snapshot> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(FILE_PREFIX) {
            continue;
        }
        let modified = entry.metadata().and_then(|m| m.modified());
        if modified.is_ok_and(|m| SystemTime::now().duration_since(m).unwrap_or_default() > TTL) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_without_leftovers() {
        let dir = std::env::temp_dir().join(format!("status_cli-unit-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("state");
        write_atomic(&file, b"old").unwrap();
        write_atomic(&file, b"new").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clock_extrapolates_until_the_base_changes() {
        let mut s = Session::detached(1_000);
        assert_eq!(s.elapsed(60_000), 60_000);
        s.now = 126_000;
        assert_eq!(s.elapsed(60_000), 185_000);
        assert_eq!(s.elapsed(90_000), 90_000);
    }

    #[test]
    fn host_ticking_duration_keeps_the_anchor() {
        let mut s = Session::detached(0);
        s.elapsed(10_000);
        s.now = 5_000;
        assert_eq!(s.elapsed(15_200), 15_200);
        assert!(s.current.clock == Some(Seen { ms: 10_000, at: 0 }));
    }

    #[test]
    fn changes_ease_towards_the_new_value() {
        let mut s = Session::detached(0);
        let shown = |s: &mut Session, pct| s.track("x", Some(pct)).map(|m| (m.shown.round(), m.growing));
        assert_eq!(shown(&mut s, 10.0), Some((10.0, false)));
        s.now = 10_000;
        assert_eq!(shown(&mut s, 40.0), Some((10.0, true)));
        s.now = 11_500;
        assert_eq!(shown(&mut s, 40.0), Some((33.0, true)));
        s.now = 13_000;
        assert_eq!(shown(&mut s, 40.0), Some((40.0, false)));
        assert_eq!(shown(&mut s, 20.0), Some((40.0, false)));
        assert!(s.track("x", None).is_none());
    }

    #[test]
    fn sub_percent_changes_update_the_value_immediately() {
        let mut s = Session::detached(0);
        s.track("x", Some(79.1));
        s.now = 10_000;
        assert_eq!(s.track("x", Some(79.4)).map(|m| m.shown), Some(79.4));
    }
}
