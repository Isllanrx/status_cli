use crate::i18n::labels;
use crate::payload::{Host, Payload};
use crate::state::{Motion, Session};
use crate::style::{Frame, Gauge};

const AUTOCOMPACT_BUFFER: f64 = 33_000.0;

#[derive(Clone, Copy, Default)]
pub struct AutoCompact {
    pub window: Option<f64>,
    pub percent: Option<f64>,
}

pub struct Context {
    label: &'static str,
    motion: Option<Motion>,
}

pub fn read(payload: &Payload, auto_compact: AutoCompact, session: &mut Session) -> Context {
    let window = payload.context_window.as_ref();
    let (label, pct) = match payload.host() {
        Host::Claude => (
            labels().compact,
            window.and_then(|w| compact_progress(w.used_percentage?, w.context_window_size?, auto_compact)),
        ),
        Host::Agy | Host::Codex => (labels().context, window.and_then(|w| w.used_percentage)),
    };
    Context { label, motion: session.track(label, pct) }
}

fn compact_progress(used: f64, size: f64, auto_compact: AutoCompact) -> Option<f64> {
    let window = auto_compact.window.filter(|w| *w > 0.0).map_or(size, |w| w.min(size));
    let mut threshold = window - AUTOCOMPACT_BUFFER;
    if let Some(percent) = auto_compact.percent.filter(|p| *p > 0.0 && *p < 100.0) {
        threshold = threshold.min(window * percent / 100.0);
    }
    (threshold > 0.0 && size > 0.0).then(|| (used / 100.0 * size / threshold * 100.0).min(100.0))
}

impl Context {
    pub fn render(&self, frame: &Frame) -> String {
        let (pct, growing) = self.motion.as_ref().map_or((None, false), |m| (Some(m.shown), m.growing));
        Gauge { label: self.label, pct, growing }.render(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::parse;
    use crate::style::strip;

    fn line(json: &str, auto_compact: AutoCompact) -> String {
        let mut session = Session::detached(0);
        let context = read(&parse(json), auto_compact, &mut session);
        strip(&context.render(&Frame { now: 0, bar_width: 0, tight: false }))
    }

    #[test]
    fn claude_measures_progress_towards_auto_compact() {
        let json = r#"{"context_window":{"used_percentage":48,"context_window_size":200000}}"#;
        assert_eq!(line(json, AutoCompact::default()), "compactar 57%");
    }

    #[test]
    fn override_only_lowers_the_threshold() {
        assert_eq!(compact_progress(40.0, 200_000.0, AutoCompact { window: None, percent: Some(50.0) }), Some(80.0));
        assert_eq!(
            compact_progress(40.0, 200_000.0, AutoCompact { window: None, percent: Some(99.0) }),
            compact_progress(40.0, 200_000.0, AutoCompact::default())
        );
    }

    #[test]
    fn a_smaller_auto_compact_window_moves_the_threshold() {
        let window = AutoCompact { window: Some(100_000.0), percent: None };
        assert_eq!(compact_progress(30.0, 200_000.0, window).map(f64::round), Some(90.0));
        let full_1m = compact_progress(96.7, 1_000_000.0, AutoCompact::default()).map(f64::round);
        assert_eq!(full_1m, Some(100.0));
    }

    #[test]
    fn tiny_windows_have_no_threshold() {
        assert_eq!(compact_progress(10.0, 1_000.0, AutoCompact::default()), None);
    }

    #[test]
    fn agy_shows_raw_context_usage() {
        let json = r#"{"product":"antigravity","context_window":{"used_percentage":40}}"#;
        assert_eq!(line(json, AutoCompact::default()), "contexto 40%");
    }
}
