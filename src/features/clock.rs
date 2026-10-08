use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::payload::{Host, Payload};
use crate::state::Session;
use crate::style::{Frame, LABEL, SOFT, STRONG, TRACK, paint};
use crate::terminal::caps;

pub fn read(payload: &Payload, session: &mut Session, now: u64) -> Option<u64> {
    let base_ms = match payload.host() {
        Host::Claude | Host::Codex => payload.cost.as_ref()?.total_duration_ms?,
        Host::Agy => {
            payload.session_key()?;
            conversation_started(payload).map_or(0, |started| now.saturating_sub(started))
        }
    };
    Some(session.elapsed(base_ms))
}

fn conversation_started(payload: &Payload) -> Option<u64> {
    let path = match payload.transcript_path.as_deref() {
        Some(path) => PathBuf::from(path),
        None => {
            let id = payload.conversation_id.as_deref().or(payload.session_id.as_deref())?;
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return None;
            }
            let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
            PathBuf::from(home).join(".gemini").join("antigravity-cli").join("conversations").join(format!("{id}.db"))
        }
    };
    let created = fs::metadata(path).ok()?.created().ok()?;
    Some(created.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

pub fn render(ms: u64, frame: &Frame) -> String {
    let minutes = ms / 60_000;
    let minute_turned = (ms / 1000) % 60 == 0 && ms > 0;
    let label = if frame.tight { String::new() } else { paint(LABEL, "tempo ") };
    let hand = paint(SOFT, format_args!("{} ", caps().glyphs.hands[((frame.now / 1000) % 4) as usize]));
    let colon = paint(if frame.odd_second() { TRACK } else { STRONG }, ':');
    let minutes_style = if minute_turned { STRONG.underline() } else { STRONG };
    format!(
        "{label}{hand}{}{colon}{}",
        paint(STRONG, format_args!("{:02}", minutes / 60)),
        paint(minutes_style, format_args!("{:02}", minutes % 60)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::strip;

    #[test]
    fn renders_animated_hh_mm() {
        let frame = Frame { now: 1_000, bar_width: 0, tight: false };
        assert_eq!(strip(&render(3_900_000, &frame)), "tempo ◷ 01:05");
        assert_eq!(strip(&render(59_000, &Frame { tight: true, ..frame })), "◷ 00:00");
    }

    #[test]
    fn underlines_minutes_on_the_second_they_turn() {
        let frame = Frame { now: 0, bar_width: 0, tight: true };
        assert!(render(120_000, &frame).contains("\x1b[1;4m02"));
        assert!(!render(121_000, &frame).contains("\x1b[1;4m"));
    }
}
