use std::collections::BTreeMap;

use crate::payload::{self, Host, Payload};
use crate::state::{Motion, Session};
use crate::style::{Frame, Gauge, SOFT, duration, heat, paint};
use crate::terminal::caps;
use crate::time::parse_utc_millis;

const FIVE_HOURS_MS: u64 = 5 * 3_600_000;
const SEVEN_DAYS_MS: u64 = 7 * 86_400_000;
const DAY_MS: u64 = 86_400_000;
const MIN_ELAPSED_FOR_PROJECTION_MS: u64 = 10 * 60_000;
const MIN_PCT_FOR_PROJECTION: f64 = 50.0;
const PROJECTION_MARGIN: f64 = 0.8;

pub struct Quota {
    label: &'static str,
    motion: Option<Motion>,
    resets_at: Option<u64>,
    window: Option<u64>,
    show_reset: bool,
}

pub fn read(payload: &Payload, now: u64, session: &mut Session) -> Vec<Quota> {
    let mut quota = |label, pct, resets_at, window, show_reset| Quota {
        label,
        motion: session.track(label, pct),
        resets_at,
        window,
        show_reset,
    };
    match payload.host() {
        Host::Claude | Host::Codex => {
            let limits = payload.rate_limits.as_ref();
            let five = limits.and_then(|l| l.five_hour.as_ref());
            let week = limits.and_then(|l| l.seven_day.as_ref());
            vec![
                quota("sessão", used(five), resets_at(five), Some(FIVE_HOURS_MS), true),
                quota("semana", used(week), resets_at(week), Some(SEVEN_DAYS_MS), false),
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
            let used = |q: &payload::Quota| q.remaining_fraction.map(|r| (1.0 - r) * 100.0);
            let resets = |q: &payload::Quota| {
                q.reset_time
                    .as_deref()
                    .and_then(parse_utc_millis)
                    .or_else(|| q.reset_in_seconds.map(|s| now + (s * 1000.0) as u64))
            };
            let windows = quotas.map(|q| family_windows(q, &model)).unwrap_or_default();
            if windows.is_empty() {
                let current = quotas.and_then(|q| pick(q, &model));
                return vec![quota("cota", current.and_then(used), current.and_then(resets), None, true)];
            }
            windows
                .into_iter()
                .map(|(span, q)| {
                    let short = span < DAY_MS;
                    quota(if short { "sessão" } else { "semana" }, used(q), resets(q), Some(span), short)
                })
                .collect()
        }
    }
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

fn family_windows<'a>(quotas: &'a BTreeMap<String, payload::Quota>, model: &str) -> Vec<(u64, &'a payload::Quota)> {
    let entries: Vec<(&str, u64, &payload::Quota)> = quotas
        .iter()
        .filter_map(|(key, q)| {
            let (family, window) = key.rsplit_once('-')?;
            Some((family, window_span(window)?, q))
        })
        .collect();
    let model = model.to_ascii_lowercase();
    let families = || entries.iter().map(|(family, ..)| *family);
    let Some(family) = families()
        .find(|family| model.contains(&family.to_ascii_lowercase()))
        .or_else(|| families().find(|family| !family.chars().all(|c| c.is_ascii_alphabetic())))
        .or_else(|| families().next())
    else {
        return Vec::new();
    };
    let mut windows: Vec<(u64, &payload::Quota)> =
        entries.into_iter().filter(|(f, ..)| *f == family).map(|(_, span, q)| (span, q)).collect();
    windows.sort_by_key(|(span, _)| *span);
    windows
}

fn pick<'a>(quotas: &'a BTreeMap<String, payload::Quota>, model_id: &str) -> Option<&'a payload::Quota> {
    let known = || quotas.iter().filter_map(|(key, q)| Some((key.as_str(), q, q.remaining_fraction?)));
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

    pub fn render(&self, frame: &Frame) -> String {
        let (pct, growing) = self.motion.as_ref().map_or((None, false), |m| (Some(m.shown), m.growing));
        let mut out = Gauge { label: self.label, pct, growing }.render(frame);
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
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::parse;
    use crate::style::strip;

    const NOW: u64 = 1_000_000;

    fn lines(json: &str, tight: bool) -> Vec<String> {
        let frame = Frame { now: NOW, bar_width: 0, tight };
        let mut session = Session::detached(NOW);
        read(&parse(json), NOW, &mut session).iter().map(|q| strip(&q.render(&frame))).collect()
    }

    #[test]
    fn claude_shows_both_windows_with_session_reset() {
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":72,"resets_at":6400},
                       "seven_day":{"used_percentage":31,"resets_at":9000}}}"#;
        assert_eq!(lines(json, false), ["sessão 72% ↻ 1h30m", "semana 31%"]);
        assert_eq!(lines(json, true), ["sessão 72%", "semana 31%"]);
    }

    #[test]
    fn claude_without_rate_limits_shows_placeholders() {
        assert_eq!(lines("{}", false), ["sessão –", "semana –"]);
    }

    #[test]
    fn projects_exhaustion_only_when_it_beats_the_reset() {
        let quota = |pct: f64, left_min: u64| Quota {
            label: "x",
            motion: Some(Motion { shown: pct, growing: false }),
            resets_at: Some(left_min * 60_000),
            window: Some(FIVE_HOURS_MS),
            show_reset: false,
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
        };
        assert_eq!(quota(4.0, 288).runs_out_in(0), None);
        assert_eq!(quota(55.0, 100).runs_out_in(0), None);
    }

    #[test]
    fn shows_projection_next_to_the_reset() {
        let json = r#"{"rate_limits":{"five_hour":{"used_percentage":80,"resets_at":11800}}}"#;
        assert_eq!(lines(json, false)[0], "sessão 80% ↻ 3h00m ⇥ 30m");
        assert_eq!(lines(json, true)[0], "sessão 80%");
    }

    #[test]
    fn agy_prefers_current_model_quota() {
        let json = r#"{"product":"antigravity","model":{"id":"gemini-3-pro"},
            "quota":{"flash":{"remaining_fraction":0.1},"gemini-3-pro":{"remaining_fraction":0.75,"reset_in_seconds":120}}}"#;
        assert_eq!(lines(json, false), ["cota 25% ↻ 2m"]);
    }

    const AGY_QUOTA: &str = r#""quota":{"3p-5h":{"remaining_fraction":1,"reset_in_seconds":18001},
        "3p-weekly":{"remaining_fraction":0.0004,"reset_in_seconds":204931},
        "gemini-5h":{"remaining_fraction":0.9989992,"reset_in_seconds":17279},
        "gemini-weekly":{"remaining_fraction":0.0058754333,"reset_in_seconds":194772}}"#;

    #[test]
    fn agy_shows_both_windows_of_the_current_model_family() {
        let json =
            format!(r#"{{"product":"antigravity","model":{{"display_name":"Gemini 3.8 Flash (High)"}},{AGY_QUOTA}}}"#);
        assert_eq!(lines(&json, false), ["sessão 0% ↻ 4h47m", "semana 99% ⇥ 40m"]);
    }

    #[test]
    fn agy_third_party_models_use_the_non_product_family() {
        let json = format!(r#"{{"product":"antigravity","model":{{"display_name":"Claude Sonnet 4.6"}},{AGY_QUOTA}}}"#);
        assert_eq!(lines(&json, false), ["sessão 0% ↻ 5h00m", "semana 100% ⇥ 2m"]);
    }

    #[test]
    fn agy_falls_back_to_most_consumed_quota() {
        let json = r#"{"quota":{"a":{"remaining_fraction":0.9},"b":{"remaining_fraction":0.2},"c":{}}}"#;
        assert_eq!(lines(json, false), ["cota 80%"]);
    }
}
