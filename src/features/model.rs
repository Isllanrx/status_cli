use crate::payload::{Host, Payload};
use crate::style::{SOFT, STRONG, capitalize, heat, paint};

pub struct Model {
    name: String,
    effort: Option<Effort>,
    fast: bool,
}

struct Effort {
    name: String,
    heat: f64,
}

pub fn read(payload: &Payload) -> Option<Model> {
    let name = payload.model.as_ref()?.display_name.as_deref()?.split_whitespace().next()?.to_owned();
    let effort = match payload.host() {
        Host::Claude => payload.effort.as_ref().map(|e| claude_effort(&e.level)),
        Host::Agy => payload.execution_mode.as_deref().map(agy_mode),
    };
    Some(Model { name, effort, fast: payload.fast_mode == Some(true) })
}

fn claude_effort(level: &str) -> Effort {
    let (name, heat) = match level {
        "low" => ("Low", 10.0),
        "medium" => ("Medium", 40.0),
        "high" => ("High", 65.0),
        "xhigh" => ("XHigh", 80.0),
        "max" => ("Max", 95.0),
        other => return Effort { name: capitalize(other), heat: 40.0 },
    };
    Effort { name: name.to_owned(), heat }
}

fn agy_mode(mode: &str) -> Effort {
    Effort { name: capitalize(mode), heat: if mode == "fast" { 25.0 } else { 65.0 } }
}

impl Model {
    pub fn render(&self) -> String {
        let mut out = paint(STRONG, &self.name);
        if let Some(effort) = &self.effort {
            out += &paint(SOFT, " - ");
            out += &paint(heat(effort.heat), &effort.name);
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
        assert_eq!(line(json).as_deref(), Some("Opus - XHigh"));
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
    fn missing_model_hides_segment() {
        assert_eq!(line(r#"{"effort":{"level":"high"}}"#), None);
    }
}
