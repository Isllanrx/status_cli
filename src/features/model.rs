use crate::payload::{Host, Payload};
use crate::style::{SOFT, STRONG, capitalize, heat, paint};

const EFFORT_HEAT: f64 = 65.0;

pub struct Model {
    name: String,
    effort: Option<String>,
    fast: bool,
}

pub fn read(payload: &Payload) -> Option<Model> {
    let name = short_name(payload.model.as_ref()?.display_name.as_deref()?)?;
    let level = match payload.host() {
        Host::Claude | Host::Codex => payload.effort.as_ref().map(|e| e.level.as_str()),
        Host::Agy => payload.execution_mode.as_deref(),
    };
    let effort = level.map(str::trim).filter(|l| !l.is_empty()).map(capitalize);
    Some(Model { name, effort, fast: payload.fast_mode == Some(true) })
}

fn short_name(display_name: &str) -> Option<String> {
    let words: Vec<&str> = display_name
        .split_whitespace()
        .take_while(|word| !word.starts_with('('))
        .filter(|word| !word.starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

impl Model {
    pub fn render(&self) -> String {
        let mut out = paint(STRONG, &self.name);
        if let Some(effort) = &self.effort {
            out += &paint(SOFT, " - ");
            out += &paint(heat(EFFORT_HEAT), effort);
        }
        if self.fast {
            out += &paint(SOFT, " · ");
            out += &paint(heat(70.0).bold(), "fast");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::parse;
    use crate::style::strip;

    fn line(json: &str) -> Option<String> {
        read(&parse(json)).map(|m| strip(&m.render()))
    }

    #[test]
    fn claude_shows_first_word_and_effort() {
        let json = r#"{"model":{"display_name":"Opus 5.5"},"effort":{"level":"xhigh"}}"#;
        assert_eq!(line(json).as_deref(), Some("Opus - Xhigh"));
    }

    #[test]
    fn agy_uses_execution_mode() {
        let json = r#"{"product":"antigravity","model":{"display_name":"Gemini 3"},"execution_mode":"planning"}"#;
        assert_eq!(line(json).as_deref(), Some("Gemini - Planning"));
    }

    #[test]
    fn fast_mode_adds_a_badge() {
        let json = r#"{"model":{"display_name":"Opus 5.5"},"effort":{"level":"high"},"fast_mode":true}"#;
        assert_eq!(line(json).as_deref(), Some("Opus - High · fast"));
    }

    #[test]
    fn short_name_keeps_the_family_without_versions() {
        for (display, short) in [
            ("Opus 5.5", "Opus"),
            ("Opus 4.6 (1M context)", "Opus"),
            ("Sonnet 5.5", "Sonnet"),
            ("Haiku 5.5", "Haiku"),
            ("Fable 5.1", "Fable"),
            ("Claude Sonnet 4.5", "Claude Sonnet"),
            ("Claude", "Claude"),
            ("Gemini 3 Pro", "Gemini Pro"),
            ("Gemini 3 Flash", "Gemini Flash"),
            ("GPT-OSS 120B", "GPT-OSS"),
        ] {
            assert_eq!(short_name(display).as_deref(), Some(short), "{display}");
        }
        assert_eq!(short_name("4.5"), None);
    }

    #[test]
    fn missing_model_hides_segment() {
        assert_eq!(line(r#"{"effort":{"level":"high"}}"#), None);
    }
}
