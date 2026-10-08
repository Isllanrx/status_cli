use crate::payload::{Host, Payload};
use crate::state::Session;
use crate::style::{Frame, LABEL, SOFT, STRONG, TRACK, paint};
use crate::terminal::caps;

pub fn read(payload: &Payload, session: &mut Session) -> Option<u64> {
    let base_ms = match payload.host() {
        Host::Claude => payload.cost.as_ref()?.total_duration_ms?,
        Host::Agy => payload.session_key().map(|_| 0)?,
    };
    Some(session.elapsed(base_ms))
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
