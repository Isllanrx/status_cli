use std::collections::BTreeMap;

use serde::Deserialize;

pub const MAX_COLUMNS: usize = 1000;
const MAX_TEXT_CHARS: usize = 48;
const MAX_KEY_CHARS: usize = 128;

#[derive(Default, Deserialize)]
pub struct Payload {
    pub session_id: Option<String>,
    pub conversation_id: Option<String>,
    pub transcript_path: Option<String>,
    pub product: Option<String>,
    pub model: Option<Model>,
    pub effort: Option<Effort>,
    pub execution_mode: Option<String>,
    pub fast_mode: Option<bool>,
    pub rate_limits: Option<RateLimits>,
    pub quota: Option<BTreeMap<String, Quota>>,
    pub context_window: Option<ContextWindow>,
    pub cost: Option<Cost>,
    pub terminal_width: Option<usize>,
}

#[derive(Default, Deserialize)]
pub struct Model {
    pub id: Option<String>,
    pub display_name: Option<String>,
    pub effort: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct Effort {
    pub level: String,
}

#[derive(Default, Deserialize)]
pub struct RateLimits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

#[derive(Default, Deserialize)]
pub struct Window {
    pub used_percentage: Option<f64>,
    pub resets_at: Option<f64>,
}

#[derive(Default, Deserialize)]
pub struct Quota {
    pub remaining_fraction: Option<f64>,
    pub reset_in_seconds: Option<f64>,
}

#[derive(Default, Deserialize)]
pub struct ContextWindow {
    pub used_percentage: Option<f64>,
    pub context_window_size: Option<f64>,
}

#[derive(Default, Deserialize)]
pub struct Cost {
    pub total_duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Claude,
    Agy,
    Codex,
}

impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Claude => "claude",
            Host::Agy => "agy",
            Host::Codex => "codex",
        }
    }
}

impl Payload {
    pub fn validated(mut self) -> Self {
        if let Some(model) = &mut self.model {
            model.display_name = model.display_name.as_deref().map(clean_text);
            model.effort = model.effort.as_deref().map(clean_text);
        }
        if let Some(effort) = &mut self.effort {
            effort.level = clean_text(&effort.level);
        }
        self.execution_mode = self.execution_mode.as_deref().map(clean_text);
        if let Some(limits) = &mut self.rate_limits {
            for window in [limits.five_hour.as_mut(), limits.seven_day.as_mut()].into_iter().flatten() {
                window.used_percentage = percent(window.used_percentage);
                window.resets_at = positive(window.resets_at);
            }
        }
        for quota in self.quota.iter_mut().flat_map(|q| q.values_mut()) {
            quota.remaining_fraction = quota.remaining_fraction.filter(|f| f.is_finite()).map(|f| f.clamp(0.0, 1.0));
            quota.reset_in_seconds = positive(quota.reset_in_seconds);
        }
        if let Some(context) = &mut self.context_window {
            context.used_percentage = percent(context.used_percentage);
            context.context_window_size = positive(context.context_window_size);
        }
        self.terminal_width = self.terminal_width.map(|w| w.min(MAX_COLUMNS));
        self
    }

    pub fn host(&self) -> Host {
        match self.product.as_deref() {
            Some("codex") => Host::Codex,
            Some("antigravity") => Host::Agy,
            _ if self.conversation_id.is_some() || self.quota.is_some() => Host::Agy,
            _ => Host::Claude,
        }
    }

    pub fn session_key(&self) -> Option<String> {
        let (prefix, id) = match self.host() {
            Host::Claude => ("", self.session_id.as_deref()?),
            Host::Codex => ("codex-", self.session_id.as_deref()?),
            Host::Agy => ("agy-", self.conversation_id.as_deref().or(self.session_id.as_deref())?),
        };
        let id: String =
            id.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).take(MAX_KEY_CHARS).collect();
        (!id.is_empty()).then(|| format!("{prefix}{id}"))
    }
}

fn clean_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(MAX_TEXT_CHARS).collect()
}

fn percent(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 100.0))
}

fn positive(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite() && *v > 0.0)
}

#[cfg(test)]
pub fn parse(json: &str) -> Payload {
    serde_json::from_str::<Payload>(json).unwrap().validated()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_host_by_agy_only_fields() {
        assert_eq!(parse(r#"{"session_id":"s"}"#).host(), Host::Claude);
        assert_eq!(parse(r#"{"product":"antigravity"}"#).host(), Host::Agy);
        assert_eq!(parse(r#"{"conversation_id":"c"}"#).host(), Host::Agy);
        assert_eq!(parse(r#"{"quota":{}}"#).host(), Host::Agy);
    }

    #[test]
    fn validation_bounds_numbers_and_strips_control_characters() {
        let p = parse(
            r#"{"model":{"display_name":"Opus\u001b[31m 5.5"},"effort":{"level":"hi\u0007gh"},
               "rate_limits":{"five_hour":{"used_percentage":-5,"resets_at":-1},"seven_day":{"used_percentage":250}},
               "quota":{"a":{"remaining_fraction":3,"reset_in_seconds":-9}},
               "context_window":{"used_percentage":101,"context_window_size":0},"terminal_width":99999999}"#,
        );
        assert_eq!(p.model.unwrap().display_name.as_deref(), Some("Opus[31m 5.5"));
        assert_eq!(p.effort.unwrap().level, "high");
        let limits = p.rate_limits.unwrap();
        assert_eq!(limits.five_hour.as_ref().unwrap().used_percentage, Some(0.0));
        assert_eq!(limits.five_hour.unwrap().resets_at, None);
        assert_eq!(limits.seven_day.unwrap().used_percentage, Some(100.0));
        assert_eq!(p.quota.as_ref().unwrap()["a"].remaining_fraction, Some(1.0));
        assert_eq!(p.quota.unwrap()["a"].reset_in_seconds, None);
        assert_eq!(p.context_window.as_ref().unwrap().used_percentage, Some(100.0));
        assert_eq!(p.context_window.unwrap().context_window_size, None);
        assert_eq!(p.terminal_width, Some(MAX_COLUMNS));
    }

    #[test]
    fn long_text_is_truncated() {
        let p = parse(&format!(
            r#"{{"model":{{"display_name":"{}"}},"session_id":"{}"}}"#,
            "x".repeat(500),
            "y".repeat(500)
        ));
        assert_eq!(p.model.unwrap().display_name.unwrap().len(), MAX_TEXT_CHARS);
    }

    #[test]
    fn session_key_is_sanitized_and_host_scoped() {
        assert_eq!(parse(r#"{"session_id":"a/b c"}"#).session_key().as_deref(), Some("abc"));
        assert_eq!(parse(r#"{"conversation_id":"c-1"}"#).session_key().as_deref(), Some("agy-c-1"));
        assert_eq!(parse(r#"{"session_id":"../"}"#).session_key(), None);
    }
}
