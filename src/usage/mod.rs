pub mod guard;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::shared::payload::{Host, Payload, RateLimits, Window};
use crate::shared::state::write_atomic;

const SAME_WINDOW_MS: u64 = 15 * 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Reading {
    used: f64,
    resets_at: u64,
    #[serde(default)]
    changed_at: u64,
}

type Readings = BTreeMap<String, Reading>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Store {
    shared: Readings,
    sources: BTreeMap<String, Readings>,
}

pub fn share(dir: &Path, payload: &Payload, now: u64) -> Payload {
    let path = file(dir, payload.host());
    let stored: Store = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    let merged = merge(&stored, payload.session_key(), readings(payload, now), now);
    if merged != stored
        && let Ok(json) = serde_json::to_vec(&merged)
    {
        let _ = fs::create_dir_all(dir);
        let _ = write_atomic(&path, &json);
    }
    let mut shared = payload.clone();
    apply(&mut shared, &merged.shared, now);
    shared
}

fn file(dir: &Path, host: Host) -> PathBuf {
    let account = match host {
        Host::Claude => env::var("CLAUDE_CONFIG_DIR").ok(),
        Host::Codex => env::var("CODEX_HOME").ok(),
        Host::Agy => None,
    };
    let name = match account.filter(|dir| !dir.is_empty()) {
        Some(dir) => format!("usage-{}-{:016x}.json", host.name(), fnv1a(&dir)),
        None => format!("usage-{}.json", host.name()),
    };
    dir.join(name)
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3))
}

fn readings(payload: &Payload, now: u64) -> Readings {
    let mut out = Readings::new();
    if let Some(limits) = &payload.rate_limits {
        for (key, window) in [("five_hour", &limits.five_hour), ("seven_day", &limits.seven_day)] {
            if let Some(Window { used_percentage: Some(used), resets_at: Some(resets_at) }) = window {
                out.insert(
                    key.to_owned(),
                    Reading { used: *used, resets_at: (resets_at * 1000.0) as u64, changed_at: now },
                );
            }
        }
    }
    for (key, quota) in payload.quota.iter().flatten() {
        if let (Some(used), Some(resets_at)) = (quota.used(), quota.resets_at(now)) {
            out.insert(key.clone(), Reading { used, resets_at, changed_at: now });
        }
    }
    out
}

fn merge(stored: &Store, source: Option<String>, current: Readings, now: u64) -> Store {
    let live = |readings: &Readings| -> Readings {
        readings.iter().filter(|(_, r)| r.resets_at > now).map(|(k, r)| (k.clone(), *r)).collect()
    };
    let mut shared = live(&stored.shared);
    let mut sources: BTreeMap<String, Readings> =
        stored.sources.iter().map(|(k, r)| (k.clone(), live(r))).filter(|(_, r)| !r.is_empty()).collect();
    let previous = source.as_ref().map(|source| stored.sources.get(source).cloned().unwrap_or_default());
    let mut reported = Readings::new();
    for (key, reading) in live(&current) {
        let last = previous.as_ref().and_then(|p| p.get(&key)).filter(|old| same(old, &reading));
        let changed = previous.is_some() && last.is_none();
        let known = shared.get(&key);
        let mut next = match known {
            Some(known) if newer_window(known, &reading) => *known,
            Some(known) if !changed && !newer_window(&reading, known) => *known,
            _ => reading,
        };
        next.changed_at = match known {
            Some(known) if same(known, &next) && known.changed_at > 0 => known.changed_at,
            _ => now,
        };
        shared.insert(key.clone(), next);
        reported.insert(key, last.copied().unwrap_or(reading));
    }
    if let Some(source) = source {
        if reported.is_empty() {
            sources.remove(&source);
        } else {
            sources.insert(source, reported);
        }
    }
    Store { shared, sources }
}

fn same(a: &Reading, b: &Reading) -> bool {
    a.used == b.used && a.resets_at.abs_diff(b.resets_at) <= SAME_WINDOW_MS
}

fn newer_window(a: &Reading, b: &Reading) -> bool {
    a.resets_at > b.resets_at + SAME_WINDOW_MS
}

fn apply(payload: &mut Payload, merged: &Readings, now: u64) {
    if merged.is_empty() {
        return;
    }
    let shown: Vec<&str> = match payload.host() {
        Host::Agy => payload.quota.iter().flat_map(|quota| quota.keys().map(String::as_str)).collect(),
        _ => vec!["five_hour", "seven_day"],
    };
    payload.usage_changed_at =
        shown.iter().filter_map(|key| merged.get(*key)).map(|r| r.changed_at).filter(|at| *at > 0).max();
    match payload.host() {
        Host::Claude | Host::Codex => {
            let limits = payload.rate_limits.get_or_insert_with(RateLimits::default);
            for (key, slot) in [("five_hour", &mut limits.five_hour), ("seven_day", &mut limits.seven_day)] {
                if let Some(reading) = merged.get(key) {
                    *slot = Some(Window {
                        used_percentage: Some(reading.used),
                        resets_at: Some(reading.resets_at as f64 / 1000.0),
                    });
                }
            }
        }
        Host::Agy => {
            for (key, quota) in payload.quota.iter_mut().flatten() {
                let Some(reading) = merged.get(key) else { continue };
                quota.remaining_fraction = Some(1.0 - reading.used / 100.0);
                quota.reset_time = None;
                quota.reset_in_seconds = Some(reading.resets_at.saturating_sub(now) as f64 / 1000.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::payload::parse;

    const NOW: u64 = 1_000_000_000;

    fn reading(used: f64, resets_at: u64) -> Reading {
        Reading { used, resets_at, changed_at: NOW }
    }

    fn one(key: &str, r: Reading) -> Readings {
        Readings::from([(key.to_owned(), r)])
    }

    fn store(source: &str, key: &str, r: Reading) -> Store {
        Store { shared: one(key, r), sources: BTreeMap::from([(source.to_owned(), one(key, r))]) }
    }

    #[test]
    fn the_latest_change_wins_even_when_usage_drops() {
        let stored = store("a", "five_hour", reading(97.0, NOW + 3_600_000));
        let merged = merge(&stored, Some("a".to_owned()), one("five_hour", reading(93.0, NOW + 3_600_000)), NOW);
        assert_eq!(merged.shared["five_hour"].used, 93.0);
    }

    #[test]
    fn an_unchanged_terminal_does_not_override_a_fresher_one() {
        let mut stored = store("busy", "five_hour", reading(15.0, NOW + 3_600_000));
        stored.sources.insert("idle".to_owned(), one("five_hour", reading(10.0, NOW + 3_600_000)));
        let merged = merge(&stored, Some("idle".to_owned()), one("five_hour", reading(10.0, NOW + 3_600_000)), NOW);
        assert_eq!(merged.shared["five_hour"].used, 15.0);
        let anonymous = merge(&stored, None, one("five_hour", reading(1.0, NOW + 3_600_000)), NOW);
        assert_eq!(anonymous.shared["five_hour"].used, 15.0);
    }

    #[test]
    fn a_new_terminal_reports_a_fresh_reading() {
        let stored = store("old", "seven_day", reading(40.0, NOW + 3_600_000));
        let merged = merge(&stored, Some("new".to_owned()), one("seven_day", reading(38.0, NOW + 3_600_000)), NOW);
        assert_eq!(merged.shared["seven_day"].used, 38.0);
    }

    #[test]
    fn newer_window_replaces_and_stale_window_is_ignored() {
        let stored = store("a", "five_hour", reading(90.0, NOW + 60_000));
        let fresh = merge(&stored, Some("b".to_owned()), one("five_hour", reading(2.0, NOW + 5 * 3_600_000)), NOW);
        assert_eq!(fresh.shared["five_hour"], reading(2.0, NOW + 5 * 3_600_000));
        let stale = merge(&fresh, Some("c".to_owned()), one("five_hour", reading(90.0, NOW + 60_000)), NOW);
        assert_eq!(stale.shared["five_hour"], reading(2.0, NOW + 5 * 3_600_000));
    }

    #[test]
    fn a_reading_keeps_the_time_it_last_changed() {
        let stored = store("a", "five_hour", Reading { changed_at: NOW - 600_000, ..reading(20.0, NOW + 3_600_000) });
        let same = merge(&stored, Some("b".to_owned()), one("five_hour", reading(20.0, NOW + 3_600_000)), NOW);
        assert_eq!(same.shared["five_hour"].changed_at, NOW - 600_000);
        let moved = merge(&stored, Some("a".to_owned()), one("five_hour", reading(21.0, NOW + 3_600_000)), NOW);
        assert_eq!(moved.shared["five_hour"].changed_at, NOW);
    }

    #[test]
    fn expired_readings_and_sources_are_dropped() {
        let stored = store("a", "seven_day", reading(50.0, NOW - 1));
        assert_eq!(merge(&stored, None, Readings::new(), NOW), Store::default());
    }

    #[test]
    fn terminals_converge_on_the_latest_change() {
        let dir = std::env::temp_dir().join(format!("status_cli-usage-{}", std::process::id()));
        let resets = (NOW + 3_600_000) / 1000;
        let json = |session: &str, used: u32| {
            format!(
                r#"{{"session_id":"{session}","rate_limits":{{"five_hour":{{"used_percentage":{used},"resets_at":{resets}}}}}}}"#
            )
        };
        let used = |p: &Payload| p.rate_limits.as_ref().unwrap().five_hour.as_ref().unwrap().used_percentage;
        share(&dir, &parse(&json("idle", 10)), NOW);
        share(&dir, &parse(&json("busy", 10)), NOW);
        let busy = share(&dir, &parse(&json("busy", 15)), NOW);
        let idle = share(&dir, &parse(&json("idle", 10)), NOW);
        let fresh = share(&dir, &parse("{}"), NOW);
        assert_eq!((used(&busy), used(&idle), used(&fresh)), (Some(15.0), Some(15.0), Some(15.0)));
        let busy = share(&dir, &parse(&json("busy", 12)), NOW);
        assert_eq!(used(&busy), Some(12.0));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn agy_quota_is_shared_per_bucket() {
        let dir = std::env::temp_dir().join(format!("status_cli-usage-agy-{}", std::process::id()));
        let json = |remaining: f64| {
            format!(
                r#"{{"product":"antigravity","quota":{{"gemini-5h":{{"remaining_fraction":{remaining},"reset_in_seconds":3600}}}}}}"#
            )
        };
        share(&dir, &parse(&json(0.6)), NOW);
        let second = share(&dir, &parse(&json(0.9)), NOW);
        let remaining = second.quota.unwrap()["gemini-5h"].remaining_fraction.unwrap();
        assert!((remaining - 0.6).abs() < 1e-9);
        fs::remove_dir_all(&dir).unwrap();
    }
}
