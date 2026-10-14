use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::payload::{Host, Payload, RateLimits, Window};
use crate::state::write_atomic;

const SAME_WINDOW_MS: u64 = 15 * 60_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Reading {
    used: f64,
    resets_at: u64,
}

type Readings = BTreeMap<String, Reading>;

pub fn share(dir: &Path, payload: &Payload, now: u64) -> Payload {
    let path = file(dir, payload.host());
    let stored: Readings =
        fs::read(&path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    let merged = merge(&stored, readings(payload, now), now);
    if merged != stored
        && let Ok(json) = serde_json::to_vec(&merged)
    {
        let _ = fs::create_dir_all(dir);
        let _ = write_atomic(&path, &json);
    }
    let mut shared = payload.clone();
    apply(&mut shared, &merged, now);
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
                out.insert(key.to_owned(), Reading { used: *used, resets_at: (resets_at * 1000.0) as u64 });
            }
        }
    }
    for (key, quota) in payload.quota.iter().flatten() {
        if let (Some(used), Some(resets_at)) = (quota.used(), quota.resets_at(now)) {
            out.insert(key.clone(), Reading { used, resets_at });
        }
    }
    out
}

fn merge(stored: &Readings, current: Readings, now: u64) -> Readings {
    let mut merged: Readings = stored.iter().filter(|(_, r)| r.resets_at > now).map(|(k, r)| (k.clone(), *r)).collect();
    for (key, reading) in current.into_iter().filter(|(_, r)| r.resets_at > now) {
        let next = match merged.get(&key) {
            Some(known) if known.resets_at > reading.resets_at + SAME_WINDOW_MS => *known,
            Some(known) if reading.resets_at <= known.resets_at + SAME_WINDOW_MS => {
                Reading { used: known.used.max(reading.used), resets_at: known.resets_at.max(reading.resets_at) }
            }
            _ => reading,
        };
        merged.insert(key, next);
    }
    merged
}

fn apply(payload: &mut Payload, merged: &Readings, now: u64) {
    if merged.is_empty() {
        return;
    }
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
    use crate::payload::parse;

    const NOW: u64 = 1_000_000_000;

    fn reading(used: f64, resets_at: u64) -> Reading {
        Reading { used, resets_at }
    }

    fn one(key: &str, r: Reading) -> Readings {
        Readings::from([(key.to_owned(), r)])
    }

    #[test]
    fn same_window_keeps_the_highest_usage() {
        let stored = one("five_hour", reading(15.0, NOW + 3_600_000));
        let merged = merge(&stored, one("five_hour", reading(10.0, NOW + 3_600_000)), NOW);
        assert_eq!(merged["five_hour"].used, 15.0);
    }

    #[test]
    fn newer_window_replaces_and_stale_window_is_ignored() {
        let stored = one("five_hour", reading(90.0, NOW + 60_000));
        let fresh = merge(&stored, one("five_hour", reading(2.0, NOW + 5 * 3_600_000)), NOW);
        assert_eq!(fresh["five_hour"], reading(2.0, NOW + 5 * 3_600_000));
        let stale = merge(&fresh, one("five_hour", reading(90.0, NOW + 60_000)), NOW);
        assert_eq!(stale["five_hour"], reading(2.0, NOW + 5 * 3_600_000));
    }

    #[test]
    fn expired_readings_are_dropped() {
        let stored = one("seven_day", reading(50.0, NOW - 1));
        assert!(merge(&stored, Readings::new(), NOW).is_empty());
    }

    #[test]
    fn terminals_converge_on_the_shared_value() {
        let dir = std::env::temp_dir().join(format!("status_cli-usage-{}", std::process::id()));
        let resets = (NOW + 3_600_000) / 1000;
        let busy = format!(r#"{{"rate_limits":{{"five_hour":{{"used_percentage":15,"resets_at":{resets}}}}}}}"#);
        let idle = format!(r#"{{"rate_limits":{{"five_hour":{{"used_percentage":10,"resets_at":{resets}}}}}}}"#);
        let busy = share(&dir, &parse(&busy), NOW);
        let idle = share(&dir, &parse(&idle), NOW);
        let fresh = share(&dir, &parse("{}"), NOW);
        let used = |p: &Payload| p.rate_limits.as_ref().unwrap().five_hour.as_ref().unwrap().used_percentage;
        assert_eq!((used(&busy), used(&idle), used(&fresh)), (Some(15.0), Some(15.0), Some(15.0)));
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
