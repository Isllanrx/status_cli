use crate::payload::{Host, Payload};
use crate::state::{Motion, Session};
use crate::style::{Frame, Gauge};

const AUTOCOMPACT_BUFFER: f64 = 33_000.0;

pub struct Context {
    label: &'static str,
    motion: Option<Motion>,
}

pub fn read(payload: &Payload, compact_override: Option<f64>, session: &mut Session) -> Context {
    let window = payload.context_window.as_ref();
    let (label, pct) = match payload.host() {
        Host::Claude => (
            "compactar",
            window.and_then(|w| compact_progress(w.used_percentage?, w.context_window_size?, compact_override)),
        ),
        Host::Agy => ("contexto", window.and_then(|w| w.used_percentage)),
    };
    Context { label, motion: session.track(label, pct) }
}

fn compact_progress(used: f64, size: f64, compact_override: Option<f64>) -> Option<f64> {
    let mut threshold = (size - AUTOCOMPACT_BUFFER) / size;
    if let Some(pct) = compact_override.filter(|p| *p > 0.0 && *p < 100.0) {
        threshold = threshold.min(pct / 100.0);
    }
    (threshold > 0.0).then(|| (used / threshold).min(100.0))
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

    fn line(json: &str, compact_override: Option<f64>) -> String {
        let mut session = Session::detached(0);
        let context = read(&parse(json), compact_override, &mut session);
        strip(&context.render(&Frame { now: 0, bar_width: 0, tight: false }))
    }

    #[test]
    fn claude_measures_progress_towards_auto_compact() {
        let json = r#"{"context_window":{"used_percentage":48,"context_window_size":200000}}"#;
        assert_eq!(line(json, None), "compactar 57%");
    }

    #[test]
    fn override_only_lowers_the_threshold() {
        assert_eq!(compact_progress(40.0, 200_000.0, Some(50.0)), Some(80.0));
        assert_eq!(compact_progress(40.0, 200_000.0, Some(99.0)), compact_progress(40.0, 200_000.0, None));
    }

    #[test]
    fn tiny_windows_have_no_threshold() {
        assert_eq!(compact_progress(10.0, 1_000.0, None), None);
    }

    #[test]
    fn agy_shows_raw_context_usage() {
        let json = r#"{"product":"antigravity","context_window":{"used_percentage":40}}"#;
        assert_eq!(line(json, None), "contexto 40%");
    }
}
