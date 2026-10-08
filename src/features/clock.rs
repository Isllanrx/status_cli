use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::i18n::labels;
use crate::payload::{Host, Payload};
use crate::state::Session;
use crate::style::{Frame, LABEL, SOFT, STRONG, TRACK, paint};
use crate::terminal::caps;
use crate::time::parse_utc_millis;

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
    let id = [payload.conversation_id.as_deref(), payload.session_id.as_deref()]
        .into_iter()
        .flatten()
        .find(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))?;
    let home = PathBuf::from(env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?)
        .join(".gemini")
        .join("antigravity-cli");
    let transcript = home.join("brain").join(id).join(".system_generated").join("logs").join("transcript.jsonl");
    first_created_at(&transcript)
        .or_else(|| payload.transcript_path.as_deref().and_then(|path| first_created_at(Path::new(path))))
        .or_else(|| file_created(&home.join("conversations").join(format!("{id}.db"))))
}

fn first_created_at(transcript: &Path) -> Option<u64> {
    let mut line = String::new();
    BufReader::new(File::open(transcript).ok()?).read_line(&mut line).ok()?;
    let record: serde_json::Value = serde_json::from_str(&line).ok()?;
    parse_utc_millis(record["created_at"].as_str()?)
}

fn file_created(path: &Path) -> Option<u64> {
    let created = fs::metadata(path).ok()?.created().ok()?;
    Some(created.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

pub fn render(ms: u64, frame: &Frame) -> String {
    let minutes = ms / 60_000;
    let minute_turned = (ms / 1000) % 60 == 0 && ms > 0;
    let label = if frame.tight { String::new() } else { paint(LABEL, format_args!("{} ", labels().time)) };
    let hand = paint(SOFT, format_args!("{} ", caps().glyphs.hands[((frame.now / 1000) % 4) as usize]));
    let colon = paint(if frame.odd_second() { TRACK } else { STRONG }, ':');
    let minutes_style = if minute_turned { STRONG.underline() } else { STRONG };
    format!(
        "{label}{hand}{}{colon}{}{}",
        paint(STRONG, format_args!("{:02}", minutes / 60)),
        paint(minutes_style, format_args!("{:02}", minutes % 60)),
        paint(LABEL, format_args!(":{:02}", (ms / 1000) % 60)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::strip;

    #[test]
    fn renders_animated_hh_mm() {
        let frame = Frame { now: 1_000, bar_width: 0, tight: false };
        assert_eq!(strip(&render(3_900_000, &frame)), "tempo ◷ 01:05:00");
        assert_eq!(strip(&render(59_000, &Frame { tight: true, ..frame })), "◷ 00:00:59");
    }

    #[test]
    fn underlines_minutes_on_the_second_they_turn() {
        let frame = Frame { now: 0, bar_width: 0, tight: true };
        assert!(render(120_000, &frame).contains("\x1b[1;4m02"));
        assert!(!render(121_000, &frame).contains("\x1b[1;4m"));
    }
}
