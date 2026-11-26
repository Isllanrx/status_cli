use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::shared::time::parse_utc_millis;

pub const MAX_COLUMNS: usize = 1000;
const MAX_TEXT_CHARS: usize = 48;
const MAX_KEY_CHARS: usize = 128;
const MILLIS_FLOOR: f64 = 1e11;

#[derive(Clone, Default, Deserialize)]
pub struct Payload {
    #[serde(default, deserialize_with = "lenient")]
    pub session_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub conversation_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub transcript_path: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub product: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub model: Option<Model>,
    #[serde(default, deserialize_with = "lenient")]
    pub effort: Option<Effort>,
    #[serde(default, deserialize_with = "lenient")]
    pub execution_mode: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub fast_mode: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    pub rate_limits: Option<RateLimits>,
    #[serde(default, deserialize_with = "lenient")]
    pub quota: Option<BTreeMap<String, Quota>>,
    #[serde(default, deserialize_with = "lenient")]
    pub context_window: Option<ContextWindow>,
    #[serde(default, deserialize_with = "lenient")]
    pub cost: Option<Cost>,
    #[serde(default, deserialize_with = "lenient")]
    pub workspace: Option<Workspace>,
    #[serde(default, deserialize_with = "count")]
    pub terminal_width: Option<usize>,
    #[serde(skip)]
    pub usage_changed_at: Option<u64>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Workspace {
    #[serde(default, deserialize_with = "lenient")]
    pub project_dir: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Model {
    #[serde(default, deserialize_with = "lenient")]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub effort: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Effort {
    pub level: String,
}

#[derive(Clone, Default, Deserialize)]
pub struct RateLimits {
    #[serde(default, deserialize_with = "lenient")]
    pub five_hour: Option<Window>,
    #[serde(default, deserialize_with = "lenient")]
    pub seven_day: Option<Window>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Window {
    #[serde(default, deserialize_with = "lenient")]
    pub used_percentage: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub resets_at: Option<f64>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Quota {
    #[serde(default, deserialize_with = "lenient")]
    pub remaining_fraction: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub reset_time: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub reset_in_seconds: Option<f64>,
    #[serde(default, deserialize_with = "flag")]
    pub disabled: bool,
}

impl Quota {
    pub fn used(&self) -> Option<f64> {
        self.remaining_fraction.map(|r| (1.0 - r) * 100.0)
    }

    pub fn resets_at(&self, now: u64) -> Option<u64> {
        self.reset_time
            .as_deref()
            .and_then(parse_utc_millis)
            .or_else(|| self.reset_in_seconds.map(|s| now + (s * 1000.0) as u64))
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct ContextWindow {
    #[serde(default, deserialize_with = "lenient")]
    pub used_percentage: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub context_window_size: Option<f64>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Cost {
    #[serde(default, deserialize_with = "count")]
    pub total_duration_ms: Option<u64>,
}

fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(deserializer: D) -> Result<Option<T>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(deserializer)?).ok())
}

fn count<'de, D: Deserializer<'de>, T: TryFrom<u64>>(deserializer: D) -> Result<Option<T>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    let whole = value.as_u64().or_else(|| value.as_f64().filter(|v| v.is_finite() && *v >= 0.0).map(|v| v as u64));
    Ok(whole.and_then(|v| T::try_from(v).ok()))
}

fn flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(deserializer)?.as_bool().unwrap_or(false))
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
                window.resets_at =
                    positive(window.resets_at).map(|at| if at > MILLIS_FLOOR { at / 1000.0 } else { at });
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
    fn a_field_with_an_unexpected_type_is_dropped_instead_of_failing_the_line() {
        let p = parse(
            r#"{"session_id":7,"model":{"display_name":"Opus 5.5","effort":{}},"effort":"high","fast_mode":"no",
               "cost":{"total_duration_ms":1234.0},"terminal_width":"wide",
               "rate_limits":{"five_hour":{"used_percentage":"12","resets_at":1800000000000}},
               "quota":{"a":{"remaining_fraction":0.5,"disabled":null,"reset_time":3}}}"#,
        );
        assert!(p.session_id.is_none() && p.effort.is_none() && p.fast_mode.is_none() && p.terminal_width.is_none());
        assert_eq!(p.model.unwrap().display_name.as_deref(), Some("Opus 5.5"));
        assert_eq!(p.cost.unwrap().total_duration_ms, Some(1234));
        let five = p.rate_limits.unwrap().five_hour.unwrap();
        assert_eq!((five.used_percentage, five.resets_at), (None, Some(1_800_000_000.0)));
        let quota = &p.quota.unwrap()["a"];
        assert_eq!((quota.remaining_fraction, quota.disabled, quota.reset_time.is_none()), (Some(0.5), false, true));
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
