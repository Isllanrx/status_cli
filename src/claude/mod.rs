use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::shared::payload::Payload;
use crate::shared::telemetry;

const MIN_WINDOW: f64 = 100_000.0;
const MAX_WINDOW: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoCompact {
    pub enabled: bool,
    pub window: Option<f64>,
    pub percent: Option<f64>,
}

impl Default for AutoCompact {
    fn default() -> Self {
        Self { enabled: true, window: None, percent: None }
    }
}

pub fn auto_compact(payload: &Payload) -> AutoCompact {
    let project = payload.workspace.as_ref().and_then(|w| w.project_dir.as_deref()).map(Path::new);
    let settings: Vec<Value> =
        settings_files(project).iter().filter_map(|file| telemetry::read_json("claude.settings", file)).collect();
    let model = payload.model.as_ref().and_then(|m| m.id.as_deref()).map(canonical);
    resolve(&settings, model.as_deref(), |name| env::var(name).ok())
}

fn resolve(settings: &[Value], model: Option<&str>, var: impl Fn(&str) -> Option<String>) -> AutoCompact {
    let disabled = var("DISABLE_AUTO_COMPACT").is_some_and(|value| truthy(&value));
    let enabled = settings.iter().find_map(|s| s["autoCompactEnabled"].as_bool()).unwrap_or(true);
    let window = var("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .as_deref()
        .and_then(env_window)
        .or_else(|| settings.iter().find_map(|s| saved_window(s, model)).flatten());
    let percent = var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE").and_then(|value| {
        let parsed = value.trim().parse().map_err(|err| format!("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE={value}: {err}"));
        telemetry::logged("claude.env", parsed)
    });
    AutoCompact { enabled: enabled && !disabled, window, percent }
}

fn saved_window(settings: &Value, model: Option<&str>) -> Option<Option<f64>> {
    let per_model = model
        .and_then(|model| {
            let entries = settings["modelSettings"].as_object()?;
            entries.iter().find(|(key, _)| canonical(key) == model).map(|(_, entry)| &entry["autoCompactWindow"])
        })
        .filter(|value| !value.is_null());
    let value = per_model.or_else(|| Some(&settings["autoCompactWindow"]).filter(|value| !value.is_null()))?;
    Some(value.as_f64().filter(|window| (MIN_WINDOW..=MAX_WINDOW).contains(window)))
}

fn env_window(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let (digits, scale) = match raw.chars().last()? {
        'k' | 'K' => (&raw[..raw.len() - 1], 1e3),
        'm' | 'M' => (&raw[..raw.len() - 1], 1e6),
        _ => (raw, 1.0),
    };
    let tokens = digits.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0)? * scale;
    Some(tokens.clamp(MIN_WINDOW, MAX_WINDOW))
}

fn canonical(id: &str) -> String {
    let id = id.trim().to_ascii_lowercase().replace("[1m]", "");
    match id.rsplit_once('-') {
        Some((base, date)) if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) => base.to_owned(),
        _ => id,
    }
}

fn truthy(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn settings_files(project: Option<&Path>) -> Vec<PathBuf> {
    let managed = managed_dir();
    let mut files: Vec<PathBuf> = fs::read_dir(managed.join("managed-settings.d"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "json")
                && !path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    files.reverse();
    files.push(managed.join("managed-settings.json"));
    if let Some(project) = project {
        files.push(project.join(".claude").join("settings.local.json"));
        files.push(project.join(".claude").join("settings.json"));
    }
    files.extend(user_dir().map(|dir| dir.join("settings.json")));
    files
}

fn managed_dir() -> PathBuf {
    PathBuf::from(if cfg!(windows) {
        r"C:\Program Files\ClaudeCode"
    } else if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode"
    } else {
        "/etc/claude-code"
    })
}

fn user_dir() -> Option<PathBuf> {
    env::var_os("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty()).map(PathBuf::from).or_else(|| {
        let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"))?;
        Some(PathBuf::from(home).join(".claude"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn saved_model_window_beats_the_top_level_one_in_the_same_file() {
        let user =
            json!({"autoCompactWindow": 300000, "modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 600000}}});
        assert_eq!(resolve(std::slice::from_ref(&user), Some("claude-opus-5-5"), no_env).window, Some(600_000.0));
        assert_eq!(resolve(&[user], Some("claude-sonnet-5-5"), no_env).window, Some(300_000.0));
    }

    #[test]
    fn the_highest_precedence_file_that_sets_a_window_decides() {
        let local = json!({"autoCompactWindow": 200000});
        let user = json!({"modelSettings": {"claude-opus-5-5": {"autoCompactWindow": 600000}}});
        assert_eq!(resolve(&[local, user], Some("claude-opus-5-5"), no_env).window, Some(200_000.0));
        let auto =
            json!({"modelSettings": {"claude-opus-5-5": {"autoCompactWindow": "auto"}}, "autoCompactWindow": 300000});
        assert_eq!(resolve(&[auto], Some("claude-opus-5-5"), no_env).window, None);
    }

    #[test]
    fn the_variable_wins_and_follows_the_documented_limits() {
        let user = [json!({"autoCompactWindow": 300000})];
        let with = |value: &'static str| {
            resolve(&user, None, move |name| (name == "CLAUDE_CODE_AUTO_COMPACT_WINDOW").then(|| value.to_owned()))
                .window
        };
        assert_eq!(with("500k"), Some(500_000.0));
        assert_eq!(with("1M"), Some(1_000_000.0));
        assert_eq!(with("200"), Some(100_000.0));
        assert_eq!(with("5000000"), Some(1_000_000.0));
        assert_eq!(with("lots"), Some(300_000.0));
    }

    #[test]
    fn auto_compact_can_be_turned_off_by_setting_or_variable() {
        assert!(!resolve(&[json!({"autoCompactEnabled": false})], None, no_env).enabled);
        assert!(!resolve(&[], None, |name| (name == "DISABLE_AUTO_COMPACT").then(|| "1".to_owned())).enabled);
        assert!(resolve(&[], None, no_env).enabled);
    }

    #[test]
    fn model_ids_match_their_canonical_entry() {
        assert_eq!(canonical("claude-opus-5-5[1m]"), "claude-opus-5-5");
        assert_eq!(canonical("Claude-Haiku-4-5-20251001"), "claude-haiku-4-5");
        assert_eq!(canonical("claude-opus-5-5"), "claude-opus-5-5");
    }
}
