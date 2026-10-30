use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::agy::{Group, Groups};
use crate::i18n::labels;
use crate::render::style::{Cell, Frame, Gauge, LABEL, SOFT, duration, heat, paint, title};
use crate::render::terminal::caps;
use crate::shared::payload::{self, Host, Payload};
use crate::shared::state::{Motion, Session};

const FIVE_HOURS_MS: u64 = 5 * 3_600_000;
const SEVEN_DAYS_MS: u64 = 7 * 86_400_000;
const DAY_MS: u64 = 86_400_000;
const MIN_ELAPSED_FOR_PROJECTION_MS: u64 = 10 * 60_000;
const MIN_PCT_FOR_PROJECTION: f64 = 50.0;
const PROJECTION_MARGIN: f64 = 0.8;
const STALE_MS: u64 = 10 * 60_000;

pub struct Quota {
    label: &'static str,
    motion: Option<Motion>,
    resets_at: Option<u64>,
    window: Option<u64>,
    show_reset: bool,
    paused: bool,
    changed_at: Option<u64>,
}

type Windows<'a> = Vec<(u64, Option<&'a payload::Quota>)>;

pub fn read(payload: &Payload, now: u64, session: &mut Session, groups: Option<&Groups>) -> Vec<Quota> {
    let mut quota = |label, pct: Option<f64>, resets_at: Option<u64>, window, show_reset, paused| {
        let (pct, resets_at) = match resets_at {
            Some(at) if at <= now => (pct.map(|_| 0.0), None),
            _ => (pct, resets_at),
        };
        let motion = session.track(label, pct);
        session.resets(label, resets_at);
        Quota { label, motion, resets_at, window, show_reset, paused, changed_at: None }
    };
    let mut slots = match payload.host() {
        Host::Claude | Host::Codex => {
            let limits = payload.rate_limits.as_ref();
            let five = limits.and_then(|l| l.five_hour.as_ref());
            let week = limits.and_then(|l| l.seven_day.as_ref());
            vec![
                quota(labels().session, used(five), resets_at(five), Some(FIVE_HOURS_MS), true, false),
                quota(labels().week, used(week), resets_at(week), Some(SEVEN_DAYS_MS), false, false),
            ]
        }
        Host::Agy => {
            let quotas = payload.quota.as_ref();
            let model = payload
                .model
                .as_ref()
                .map(|m| {
                    [m.id.as_deref(), m.display_name.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" ")
                })
                .unwrap_or_default();
            let used = payload::Quota::used;
            let resets = |q: &payload::Quota| q.resets_at(now);
            let empty = BTreeMap::new();
            let all = quotas.unwrap_or(&empty);
            let windows = match groups.and_then(|groups| groups.group_for(&model)) {
                Some(group) => official_windows(all, group),
                None => family_windows(all, &model),
            };
            if windows.is_empty() {
                let current = quotas.and_then(|q| pick(q, &model));
                vec![quota(labels().quota, current.and_then(used), current.and_then(resets), None, true, false)]
            } else {
                [(true, labels().session, FIVE_HOURS_MS), (false, labels().week, SEVEN_DAYS_MS)]
                    .into_iter()
                    .map(|(short, label, default)| {
                        let found = windows.iter().find(|(span, _)| (*span < DAY_MS) == short);
                        let span = found.map_or(default, |(span, _)| *span);
                        let current = found.and_then(|(_, q)| *q);
                        let paused = current.is_some_and(|q| q.disabled);
                        let current = current.filter(|_| !paused);
                        quota(label, current.and_then(used), current.and_then(resets), Some(span), short, paused)
                    })
                    .collect()
            }
        }
    };
    if let Some(first) = slots.first_mut() {
        first.changed_at = payload.usage_changed_at;
    }
    slots
}

fn used(window: Option<&payload::Window>) -> Option<f64> {
    window?.used_percentage
}

fn resets_at(window: Option<&payload::Window>) -> Option<u64> {
    window?.resets_at.map(|s| (s * 1000.0) as u64)
}

fn window_span(window: &str) -> Option<u64> {
    match window {
        "weekly" => Some(SEVEN_DAYS_MS),
        "daily" => Some(DAY_MS),
        hours => hours.strip_suffix('h')?.parse::<u64>().ok().map(|h| h * 3_600_000),
    }
}

fn official_windows<'a>(quotas: &'a BTreeMap<String, payload::Quota>, group: &Group) -> Windows<'a> {
    group.buckets.iter().filter_map(|bucket| Some((window_span(&bucket.window)?, quotas.get(&bucket.id)))).collect()
}

fn family_windows<'a>(quotas: &'a BTreeMap<String, payload::Quota>, model: &str) -> Windows<'a> {
    let entries: Vec<(&str, u64, &payload::Quota)> = quotas
        .iter()
        .filter_map(|(key, q)| {
            let (family, window) = key.rsplit_once('-')?;
            Some((family, window_span(window)?, q))
        })
        .collect();
    let model = model.to_ascii_lowercase();
    let model: Vec<&str> = model.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let fit = |family: &str| {
        let parts = family.split('-').count();
        let hits = family.split('-').filter(|part| model.contains(&part.to_ascii_lowercase().as_str())).count();
        (hits, parts - hits)
    };
    let families = || entries.iter().map(|(family, ..)| *family);
    let Some(family) = families()
        .filter(|family| fit(family).0 > 0)
        .min_by_key(|family| {
            let (hits, misses) = fit(family);
            (Reverse(hits), misses)
        })
        .or_else(|| families().find(|family| family.chars().any(|c| c.is_ascii_digit())))
        .or_else(|| families().next())
    else {
        return Vec::new();
    };
    let mut windows: Windows<'a> =
        entries.into_iter().filter(|(f, ..)| *f == family).map(|(_, span, q)| (span, Some(q))).collect();
    windows.sort_by_key(|(span, _)| *span);
    windows
}

fn pick<'a>(quotas: &'a BTreeMap<String, payload::Quota>, model_id: &str) -> Option<&'a payload::Quota> {
    let known = || {
        quotas.iter().filter(|(_, q)| !q.disabled).filter_map(|(key, q)| Some((key.as_str(), q, q.remaining_fraction?)))
    };
    let matches = |key: &str| !model_id.is_empty() && (key.contains(model_id) || model_id.contains(key));
    known().find(|(key, ..)| matches(key)).or_else(|| known().min_by(|a, b| a.2.total_cmp(&b.2))).map(|(_, q, _)| q)
}

impl Quota {
    fn runs_out_in(&self, now: u64) -> Option<u64> {
        let (pct, resets_at, window) = (self.motion.as_ref()?.shown, self.resets_at?, self.window?);
        let left = resets_at.saturating_sub(now);
        let elapsed = window.saturating_sub(left);
        if elapsed < MIN_ELAPSED_FOR_PROJECTION_MS || !(MIN_PCT_FOR_PROJECTION..100.0).contains(&pct) {
            return None;
        }
        let to_full = ((100.0 - pct) * elapsed as f64 / pct) as u64;
        ((to_full as f64) < left as f64 * PROJECTION_MARGIN).then_some(to_full)
    }

    pub fn cell(&self, frame: &Frame) -> Cell {
        let icons = caps().icons;
        let icon = if self.label == labels().week { icons.week() } else { icons.session() };
        Cell { icon, head: paint(LABEL, title(self.label)), body: self.body(frame) }
    }

    fn body(&self, frame: &Frame) -> String {
        let (pct, growing) = self.motion.as_ref().map_or((None, false), |m| (Some(m.shown), m.growing));
        let missing = self.paused.then_some(labels().paused);
        let mut out = Gauge { label: self.label, pct, growing, missing }.body(frame);
        if frame.tight || pct.is_none() {
            return out;
        }
        if let Some(resets_at) = self.resets_at.filter(|_| self.show_reset) {
            let left = duration(resets_at.saturating_sub(frame.now));
            out += &paint(SOFT, format_args!(" {} {left}", caps().glyphs.reset));
        }
        if let Some(to_full) = self.runs_out_in(frame.now) {
            out += &paint(heat(100.0).bold(), format_args!(" {} {}", caps().glyphs.exhaust, duration(to_full)));
        }
        if let Some(age) = self.changed_at.map(|at| frame.now.saturating_sub(at)).filter(|age| *age >= STALE_MS) {
            out += &paint(SOFT, format_args!(" · {}", title(&labels().age.replace("{}", &duration(age)))));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::style::strip;
    use crate::shared::payload::parse;

    const NOW: u64 = 1_000_000;

    fn flat(cell: &Cell) -> String {
        strip(&format!("{} {}", cell.head, cell.body))
    }

    fn lines(json: &str, tight: bool) -> Vec<String> {
        let frame = Frame { now: NOW, bar_width: 0, tight };
        let mut session = Session::detached(NOW);
        read(&parse(json), NOW, &mut session, None).iter().map(|q| flat(&q.cell(&frame))).collect()
    }

    #[test]
    fn a_reading_that_has_not_moved_for_ten_minutes_shows_its_age() {
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":72,"resets_at":6400},
                       "seven_day":{"used_percentage":31,"resets_at":9000}}}"#;
        let frame = Frame { now: NOW, bar_width: 0, tight: false };
        let render = |changed_at: u64| {
            let mut payload = parse(json);
            payload.usage_changed_at = Some(changed_at);
            let mut session = Session::detached(NOW);
            read(&payload, NOW, &mut session, None).iter().map(|q| flat(&q.cell(&frame))).collect::<Vec<_>>()
        };
        assert_eq!(render(NOW - 12 * 60_000), ["Sessão 72% ↻ 1h30m · Sem Mudança Há 12m", "Semana 31%"]);
        assert_eq!(render(NOW - 9 * 60_000), ["Sessão 72% ↻ 1h30m", "Semana 31%"]);
    }

    #[test]
    fn claude_shows_both_windows_with_session_reset() {
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":72,"resets_at":6400},
                       "seven_day":{"used_percentage":31,"resets_at":9000}}}"#;
        assert_eq!(lines(json, false), ["Sessão 72% ↻ 1h30m", "Semana 31%"]);
        assert_eq!(lines(json, true), ["Sessão 72%", "Semana 31%"]);
    }

    #[test]
    fn claude_without_rate_limits_shows_placeholders() {
        assert_eq!(lines("{}", false), ["Sessão –", "Semana –"]);
    }

    #[test]
    fn projects_exhaustion_only_when_it_beats_the_reset() {
        let quota = |pct: f64, left_min: u64| Quota {
            label: "x",
            motion: Some(Motion { shown: pct, growing: false }),
            resets_at: Some(left_min * 60_000),
            window: Some(FIVE_HOURS_MS),
            show_reset: false,
            paused: false,
            changed_at: None,
        };
        assert_eq!(quota(80.0, 180).runs_out_in(0), Some(30 * 60_000));
        assert_eq!(quota(30.0, 180).runs_out_in(0), None);
        assert_eq!(quota(80.0, 295).runs_out_in(0), None);
    }

    #[test]
    fn early_window_usage_does_not_duplicate_the_reset() {
        let quota = |pct: f64, left_min: u64| Quota {
            label: "x",
            motion: Some(Motion { shown: pct, growing: false }),
            resets_at: Some(left_min * 60_000),
            window: Some(FIVE_HOURS_MS),
            show_reset: false,
            paused: false,
            changed_at: None,
        };
        assert_eq!(quota(4.0, 288).runs_out_in(0), None);
        assert_eq!(quota(55.0, 100).runs_out_in(0), None);
    }

    #[test]
    fn shows_projection_next_to_the_reset() {
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":80,"resets_at":11800}}}"#;
        assert_eq!(lines(json, false)[0], "Sessão 80% ↻ 3h00m → 30m");
        assert_eq!(lines(json, true)[0], "Sessão 80%");
    }

    #[test]
    fn agy_prefers_current_model_quota() {
        let json = r#"{"product":"antigravity","model":{"id":"gemini-3-pro"},
            "quota":{"flash":{"remaining_fraction":0.1},"gemini-3-pro":{"remaining_fraction":0.75,"reset_in_seconds":120}}}"#;
        assert_eq!(lines(json, false), ["Cota 25% ↻ 2m"]);
    }

    #[test]
    fn agy_reads_reset_time_when_seconds_are_missing() {
        let json = r#"{"product":"antigravity","model":{"id":"gemini-3-pro"},
            "quota":{"gemini-3-pro":{"remaining_fraction":0.5,"reset_time":"1969-12-31T21:20:00-03:00"}}}"#;
        assert_eq!(lines(json, false), ["Cota 50% ↻ 3m"]);
    }

    const AGY_QUOTA: &str = r#""quota":{"3p-5h":{"remaining_fraction":1,"reset_in_seconds":18001},
        "3p-weekly":{"remaining_fraction":0.0004,"reset_in_seconds":204931},
        "gemini-5h":{"remaining_fraction":0.9989992,"reset_in_seconds":17279},
        "gemini-weekly":{"remaining_fraction":0.0058754333,"reset_in_seconds":194772}}"#;

    #[test]
    fn agy_shows_both_windows_of_the_current_model_family() {
        let json =
            format!(r#"{{"product":"antigravity","model":{{"display_name":"Gemini 3.8 Flash (High)"}},{AGY_QUOTA}}}"#);
        assert_eq!(lines(&json, false), ["Sessão 0% ↻ 4h47m", "Semana 99% → 40m"]);
    }

    #[test]
    fn agy_third_party_models_use_the_non_product_family() {
        let json = format!(r#"{{"product":"antigravity","model":{{"display_name":"Claude Sonnet 4.6"}},{AGY_QUOTA}}}"#);
        assert_eq!(lines(&json, false), ["Sessão 0% ↻ 5h00m", "Semana 100% → 2m"]);
    }

    const AGY_ARGON_QUOTA: &str = r#""quota":{"3p-weekly":{"remaining_fraction":0.5,"reset_in_seconds":7200},
        "gemini-5h":{"remaining_fraction":0.9,"reset_in_seconds":3600},
        "gemini-argon-wtu-weekly":{"remaining_fraction":0.2,"reset_in_seconds":7200},
        "gemini-weekly":{"remaining_fraction":0.7,"reset_in_seconds":7200}}"#;

    #[test]
    fn agy_switching_models_follows_the_most_specific_family() {
        let model = |name: &str| {
            format!(r#"{{"product":"antigravity","model":{{"display_name":"{name}"}},{AGY_ARGON_QUOTA}}}"#)
        };
        assert_eq!(lines(&model("Gemini 4 Argon (Medium)"), false), ["Sessão –", "Semana 80%"]);
        assert_eq!(lines(&model("Gemini 3.8 Flash (High)"), false), ["Sessão 10% ↻ 1h00m", "Semana 30%"]);
        assert_eq!(lines(&model("Claude Opus 5.5"), false), ["Sessão –", "Semana 50%"]);
    }

    #[test]
    fn agy_shows_a_disabled_bucket_as_not_applying_now() {
        let json = r#"{"product":"antigravity","model":{"display_name":"Gemini 4 Argon"},
            "quota":{"gemini-argon-weekly":{"remaining_fraction":0,"disabled":true},
                     "gemini-weekly":{"remaining_fraction":0.6,"reset_in_seconds":60}}}"#;
        assert_eq!(lines(json, false), ["Sessão –", "Semana Suspensa"]);
    }

    #[test]
    fn a_window_past_its_reset_shows_no_usage() {
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":72,"resets_at":500},
                       "seven_day":{"used_percentage":31,"resets_at":9000}}}"#;
        assert_eq!(lines(json, false), ["Sessão 0%", "Semana 31%"]);
    }

    #[test]
    fn agy_follows_the_official_groups_and_keeps_both_slots() {
        let groups = Groups {
            fetched_at: 0,
            groups: vec![
                Group {
                    name: "Gemini Models".into(),
                    models: vec!["Gemini Flash".into(), "Gemini Pro".into()],
                    buckets: vec![bucket("gemini-weekly", "weekly"), bucket("gemini-5h", "5h")],
                },
                Group {
                    name: "Claude and GPT models".into(),
                    models: vec!["Claude Opus".into(), "GPT-OSS".into()],
                    buckets: vec![bucket("3p-weekly", "weekly"), bucket("3p-5h", "5h")],
                },
                Group {
                    name: "Gemini 4 Argon".into(),
                    models: vec![],
                    buckets: vec![bucket("gemini-argon-wtu-weekly", "weekly")],
                },
            ],
        };
        let quota = r#""quota":{"3p-weekly":{"remaining_fraction":0,"reset_in_seconds":7200},
            "3p-5h":{"remaining_fraction":1,"disabled":true},
            "gemini-5h":{"remaining_fraction":0.9,"reset_in_seconds":3600},
            "gemini-argon-wtu-weekly":{"remaining_fraction":0.01,"reset_in_seconds":7200},
            "gemini-weekly":{"remaining_fraction":0.04,"reset_in_seconds":7200}}"#;
        let frame = Frame { now: NOW, bar_width: 0, tight: true };
        let line = |model: &str| {
            let json = format!(r#"{{"product":"antigravity","model":{{"display_name":"{model}"}},{quota}}}"#);
            let mut session = Session::detached(NOW);
            read(&parse(&json), NOW, &mut session, Some(&groups))
                .iter()
                .map(|q| flat(&q.cell(&frame)))
                .collect::<Vec<_>>()
        };
        assert_eq!(line("Gemini 3.8 Flash (High)"), ["Sessão 10%", "Semana 96%"]);
        assert_eq!(line("Gemini 4 Argon (Medium)"), ["Sessão –", "Semana 99%"]);
        assert_eq!(line("GPT-OSS 120B (Medium)"), ["Sessão Suspensa", "Semana 100%"]);
    }

    fn bucket(id: &str, window: &str) -> crate::agy::Bucket {
        crate::agy::Bucket { id: id.into(), window: window.into() }
    }

    #[test]
    fn agy_falls_back_to_most_consumed_quota() {
        let json = r#"{"quota":{"a":{"remaining_fraction":0.9},"b":{"remaining_fraction":0.2},"c":{}}}"#;
        assert_eq!(lines(json, false), ["Cota 80%"]);
    }
}
