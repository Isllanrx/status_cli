use std::io::{self, Read};
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::codex;
use crate::i18n::{ALL, labels};
use crate::render::style::duration;
use crate::shared::payload::{Host, Payload, Window};
use crate::shared::state::{self, Reading};

const MAX_INPUT_BYTES: u64 = 1 << 20;
const SESSION: Limits = Limits { near: 85.0, reached: 95.0 };
const WEEK: Limits = Limits { near: 90.0, reached: 97.0 };

struct Limits {
    near: f64,
    reached: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Fine,
    Near,
    Reached,
}

#[derive(Default, Deserialize)]
struct Input {
    session_id: Option<String>,
    hook_event_name: Option<String>,
    transcript_path: Option<String>,
    #[serde(rename = "conversationId")]
    conversation_id: Option<String>,
    #[serde(rename = "toolCall")]
    tool_call: Option<Value>,
    #[serde(rename = "invocationNum")]
    invocation_num: Option<u64>,
}

struct Usage {
    session: Option<Reading>,
    week: Option<Reading>,
}

pub fn host(name: &str) -> Option<Host> {
    match name {
        "claude" => Some(Host::Claude),
        "codex" => Some(Host::Codex),
        "agy" => Some(Host::Agy),
        _ => None,
    }
}

pub fn run(host: Host, dir: &Path, now: u64) -> Option<String> {
    let mut bytes = Vec::new();
    let reply = io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES)
        .read_to_end(&mut bytes)
        .ok()
        .and_then(|_| serde_json::from_slice::<Input>(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok())
        .and_then(|input| respond(host, &input, &usage(host, &input, dir, now)?, now));
    reply.or_else(|| (host == Host::Agy).then(|| "{}".to_owned()))
}

fn usage(host: Host, input: &Input, dir: &Path, now: u64) -> Option<Usage> {
    let usage = if host == Host::Codex {
        let payload = input
            .transcript_path
            .as_deref()
            .and_then(|path| codex::parse_session(Path::new(path)).ok())
            .filter(|payload| payload.rate_limits.is_some())
            .or_else(|| codex::Reader::latest().read(now).ok())?
            .validated();
        let limits = payload.rate_limits?;
        let reading = |window: Option<Window>| {
            let window = window?;
            Some(Reading { pct: window.used_percentage?, resets_at: window.resets_at.map(|s| (s * 1000.0) as u64) })
        };
        Usage { session: reading(limits.five_hour), week: reading(limits.seven_day) }
    } else {
        let key = Payload {
            session_id: input.session_id.clone(),
            conversation_id: input.conversation_id.clone(),
            product: (host == Host::Agy).then(|| "antigravity".to_owned()),
            ..Payload::default()
        }
        .session_key()?;
        let session: Vec<&str> = ALL.iter().flat_map(|l| [l.session, l.quota]).collect();
        let week: Vec<&str> = ALL.iter().map(|l| l.week).collect();
        Usage { session: state::read(dir, &key, &session), week: state::read(dir, &key, &week) }
    };
    let current = |reading: Option<Reading>| reading.filter(|r| r.resets_at.is_none_or(|at| at > now));
    let usage = Usage { session: current(usage.session), week: current(usage.week) };
    (usage.session.is_some() || usage.week.is_some()).then_some(usage)
}

impl Usage {
    fn level(&self) -> Level {
        let level = |reading: &Option<Reading>, limits: &Limits| match reading.as_ref().map(|r| r.pct.round()) {
            Some(pct) if pct >= limits.reached => Level::Reached,
            Some(pct) if pct >= limits.near => Level::Near,
            _ => Level::Fine,
        };
        level(&self.session, &SESSION).max(level(&self.week, &WEEK))
    }

    fn summary(&self, level: Level, now: u64) -> String {
        let l = labels();
        let part = |label: &str, reading: &Option<Reading>| match reading {
            None => format!("{label} ?"),
            Some(Reading { pct, resets_at: Some(at) }) => {
                format!("{label} {pct:.0}% ({} {})", l.resets_in, duration(at.saturating_sub(now)))
            }
            Some(Reading { pct, resets_at: None }) => format!("{label} {pct:.0}%"),
        };
        let usage = format!("status_cli: {}, {}.", part(l.session, &self.session), part(l.week, &self.week));
        match level {
            Level::Fine => usage,
            Level::Near => format!("{usage} {}", l.limit_near),
            Level::Reached => format!("{usage} {}", l.limit_reached),
        }
    }
}

fn respond(host: Host, input: &Input, usage: &Usage, now: u64) -> Option<String> {
    let level = usage.level();
    let text = usage.summary(level, now);
    let reply = match host {
        Host::Agy => match (&input.tool_call, input.invocation_num) {
            (Some(_), _) => return None,
            (None, Some(_)) if level == Level::Fine => json!({}),
            _ => json!({ "injectSteps": [{ "ephemeralMessage": text }] }),
        },
        Host::Claude | Host::Codex => match input.hook_event_name.as_deref()? {
            "PreToolUse" => {
                let decision = match level {
                    Level::Reached => "deny",
                    Level::Near if host == Host::Claude => "ask",
                    _ => return None,
                };
                json!({ "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": decision,
                    "permissionDecisionReason": text,
                } })
            }
            event @ ("UserPromptSubmit" | "SessionStart") => {
                json!({ "hookSpecificOutput": { "hookEventName": event, "additionalContext": text } })
            }
            _ => return None,
        },
    };
    Some(reply.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const NOW: u64 = 1_000_000_000;

    fn usage(session: f64, week: f64) -> Usage {
        Usage {
            session: Some(Reading { pct: session, resets_at: Some(NOW + 70 * 60_000) }),
            week: Some(Reading { pct: week, resets_at: Some(NOW + 5 * 86_400_000) }),
        }
    }

    fn event(name: &str) -> Input {
        Input { hook_event_name: Some(name.to_owned()), ..Input::default() }
    }

    fn reply(host: Host, input: &Input, usage: &Usage) -> Option<Value> {
        respond(host, input, usage, NOW).map(|text| serde_json::from_str(&text).unwrap())
    }

    #[test]
    fn levels_follow_the_session_and_week_limits() {
        assert_eq!(usage(50.0, 20.0).level(), Level::Fine);
        assert_eq!(usage(85.0, 20.0).level(), Level::Near);
        assert_eq!(usage(94.6, 20.0).level(), Level::Reached);
        assert_eq!(usage(10.0, 90.0).level(), Level::Near);
        assert_eq!(usage(10.0, 97.0).level(), Level::Reached);
    }

    #[test]
    fn summary_shows_usage_and_time_to_reset() {
        let fine = usage(53.0, 20.0);
        assert_eq!(
            fine.summary(Level::Fine, NOW),
            "status_cli: sessão 53% (reinicia em 1h10m), semana 20% (reinicia em 5d0h)."
        );
        let full = usage(96.0, 20.0);
        assert!(full.summary(full.level(), NOW).ends_with(labels().limit_reached));
    }

    #[test]
    fn prompt_events_always_get_context() {
        let out = reply(Host::Codex, &event("UserPromptSubmit"), &usage(10.0, 10.0)).unwrap();
        assert_eq!(out["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
        assert!(out["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("10%"));
        assert!(reply(Host::Claude, &event("Stop"), &usage(99.0, 10.0)).is_none());
    }

    #[test]
    fn tools_are_gated_only_near_the_limit() {
        let tool = event("PreToolUse");
        assert!(reply(Host::Claude, &tool, &usage(50.0, 10.0)).is_none());
        let decision = |host, u| reply(host, &tool, &u).map(|v| v["hookSpecificOutput"]["permissionDecision"].clone());
        assert_eq!(decision(Host::Claude, usage(88.0, 10.0)), Some(json!("ask")));
        assert_eq!(decision(Host::Codex, usage(88.0, 10.0)), None);
        assert_eq!(decision(Host::Codex, usage(96.0, 10.0)), Some(json!("deny")));
    }

    #[test]
    fn agy_injects_at_start_and_on_invocations_near_the_limit() {
        let start = reply(Host::Agy, &Input::default(), &usage(10.0, 10.0)).unwrap();
        assert!(start["injectSteps"][0]["ephemeralMessage"].as_str().unwrap().contains("10%"));
        let invocation = Input { invocation_num: Some(3), ..Input::default() };
        assert_eq!(reply(Host::Agy, &invocation, &usage(10.0, 10.0)), Some(json!({})));
        assert!(reply(Host::Agy, &invocation, &usage(90.0, 10.0)).unwrap()["injectSteps"].is_array());
    }

    #[test]
    fn reads_the_saved_state_and_drops_windows_that_already_reset() {
        let dir = std::env::temp_dir().join(format!("status_cli-guard-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let state = format!(
            r#"{{"gauges":{{"session":{{"from":1,"to":91,"changed_at":0,"resets_at":{}}},"week":{{"from":1,"to":40,"changed_at":0,"resets_at":{}}}}}}}"#,
            NOW + 60_000,
            NOW - 1
        );
        fs::write(dir.join("status_cli-abc"), state).unwrap();
        let input = Input { session_id: Some("abc".to_owned()), ..Input::default() };
        let read = super::usage(Host::Claude, &input, &dir, NOW).unwrap();
        assert_eq!(read.session.map(|r| r.pct), Some(91.0));
        assert!(read.week.is_none());
        assert!(super::usage(Host::Agy, &input, &dir, NOW).is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn codex_reads_limits_from_the_transcript() {
        let dir = std::env::temp_dir().join(format!("status_cli-guard-codex-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rollout-1.jsonl");
        let record = r#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":96.0,"window_minutes":300,"resets_at":2000000}}}}"#;
        fs::write(&file, format!("{record}\n")).unwrap();
        let input = Input { transcript_path: Some(file.to_string_lossy().into_owned()), ..Input::default() };
        let read = super::usage(Host::Codex, &input, &dir, NOW).unwrap();
        assert_eq!(read.level(), Level::Reached);
        fs::remove_dir_all(&dir).unwrap();
    }
}
