use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Payload {
    pub session_id: Option<String>,
    pub conversation_id: Option<String>,
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

#[derive(Deserialize)]
pub struct Model {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct Effort {
    pub level: String,
}

#[derive(Deserialize)]
pub struct RateLimits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

#[derive(Deserialize)]
pub struct Window {
    pub used_percentage: Option<f64>,
    pub resets_at: Option<f64>,
}

#[derive(Deserialize)]
pub struct Quota {
    pub remaining_fraction: Option<f64>,
    pub reset_in_seconds: Option<f64>,
}

#[derive(Deserialize)]
pub struct ContextWindow {
    pub used_percentage: Option<f64>,
    pub context_window_size: Option<f64>,
}

#[derive(Deserialize)]
pub struct Cost {
    pub total_duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Claude,
    Agy,
}

impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Claude => "claude",
            Host::Agy => "agy",
        }
    }
}

impl Payload {
    pub fn host(&self) -> Host {
        let agy =
            self.product.as_deref() == Some("antigravity") || self.conversation_id.is_some() || self.quota.is_some();
        if agy { Host::Agy } else { Host::Claude }
    }

    pub fn session_key(&self) -> Option<String> {
        let (prefix, id) = match self.host() {
            Host::Claude => ("", self.session_id.as_deref()?),
            Host::Agy => ("agy-", self.conversation_id.as_deref().or(self.session_id.as_deref())?),
        };
        let id: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect();
        (!id.is_empty()).then(|| format!("{prefix}{id}"))
    }
}

#[cfg(test)]
pub fn parse(json: &str) -> Payload {
    serde_json::from_str(json).unwrap()
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
    fn session_key_is_sanitized_and_host_scoped() {
        assert_eq!(parse(r#"{"session_id":"a/b c"}"#).session_key().as_deref(), Some("abc"));
        assert_eq!(parse(r#"{"conversation_id":"c-1"}"#).session_key().as_deref(), Some("agy-c-1"));
        assert_eq!(parse(r#"{"session_id":"../"}"#).session_key(), None);
    }
}
