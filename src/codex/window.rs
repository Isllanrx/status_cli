use std::path::Path;

use crate::shared::telemetry;
use serde_json::Value;

const DEFAULT_EFFECTIVE_PERCENT: f64 = 95.0;

pub fn usable(home: &Path, model: &str) -> Option<f64> {
    let cache: Value = telemetry::read_json("codex.models", &home.join("models_cache.json"))?;
    let configured = telemetry::read_optional("codex.config", &home.join("config.toml"))
        .and_then(|bytes| {
            telemetry::logged("codex.config", String::from_utf8_lossy(&bytes).parse::<toml_edit::DocumentMut>())
        })
        .and_then(|config| config.get("model_context_window")?.as_integer())
        .map(|window| window as f64);
    from_cache(&cache, model, configured)
}

fn from_cache(cache: &Value, model: &str, configured: Option<f64>) -> Option<f64> {
    let info = cache["models"].as_array()?.iter().find(|info| info["slug"].as_str() == Some(model))?;
    let max = info["max_context_window"].as_f64();
    let window = configured
        .map(|window| max.map_or(window, |max| window.min(max)))
        .or_else(|| info["context_window"].as_f64())
        .or(max)?;
    let percent = info["effective_context_window_percent"].as_f64().unwrap_or(DEFAULT_EFFECTIVE_PERCENT);
    Some((window * percent / 100.0).floor())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usable_window_follows_the_codex_model_catalog() {
        let cache = json!({"models": [
            {"slug": "gpt-6.1-sol", "context_window": 500000, "max_context_window": 872000, "effective_context_window_percent": 95},
            {"slug": "gpt-6-luna", "max_context_window": 272000}
        ]});
        assert_eq!(from_cache(&cache, "gpt-6.1-sol", None), Some(475_000.0));
        assert_eq!(from_cache(&cache, "gpt-6.1-sol", Some(2_000_000.0)), Some(828_400.0));
        assert_eq!(from_cache(&cache, "gpt-6-luna", None), Some(258_400.0));
        assert_eq!(from_cache(&cache, "unknown", None), None);
    }
}
