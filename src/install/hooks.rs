use serde_json::{Map, Value, json};

const TIMEOUT_SECS: u64 = 10;
pub const AGY_PLUGIN: &str = "status_cli";
const AGY_HOOK: &str = "status_cli-guard";

pub struct Event {
    pub name: &'static str,
    pub matcher: Option<&'static str>,
}

pub const CLAUDE_EVENTS: &[Event] = &[
    Event { name: "UserPromptSubmit", matcher: None },
    Event { name: "PreToolUse", matcher: Some("Agent|Task|Workflow") },
];
pub const CODEX_EVENTS: &[Event] =
    &[Event { name: "SessionStart", matcher: None }, Event { name: "UserPromptSubmit", matcher: None }];

pub fn command(binary: &str, host: &str) -> String {
    format!("{binary} guard {host}")
}

pub fn is_guard(text: &str) -> bool {
    text.contains("status_cli") && text.contains(" guard ")
}

pub fn merge(hooks: &mut Map<String, Value>, events: &[Event], command: &str) {
    for groups in hooks.values_mut() {
        if let Value::Array(groups) = groups {
            groups.retain(|group| !mentions_guard(group));
        }
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    for event in events {
        let mut group = Map::new();
        if let Some(matcher) = event.matcher {
            group.insert("matcher".to_owned(), json!(matcher));
        }
        group.insert("hooks".to_owned(), json!([{ "type": "command", "command": command, "timeout": TIMEOUT_SECS }]));
        let groups = hooks.entry(event.name).or_insert_with(|| json!([]));
        if let Value::Array(groups) = groups {
            groups.push(Value::Object(group));
        } else {
            *groups = json!([group]);
        }
    }
}

fn mentions_guard(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| hooks.iter().any(|h| h["command"].as_str().is_some_and(is_guard)))
}

pub fn agy_plugin(command: &str) -> (String, String) {
    let handler = json!([{ "type": "command", "command": command, "timeout": TIMEOUT_SECS }]);
    let manifest = json!({
        "name": AGY_PLUGIN,
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Tells the agent how much of the session and weekly usage is left",
    });
    let hooks = json!({ AGY_HOOK: { "enabled": true, "SessionStart": handler, "PreInvocation": handler } });
    (pretty(&manifest), pretty(&hooks))
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default() + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_replaces_old_guard_entries_and_keeps_other_hooks() {
        let mut hooks = json!({
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [{ "type": "command", "command": "rtk hook" }] },
                { "matcher": "Agent", "hooks": [{ "type": "command", "command": "status_cli guard claude" }] }
            ],
            "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "'/opt/status_cli' guard claude" }] }]
        });
        let map = hooks.as_object_mut().unwrap();
        merge(map, CLAUDE_EVENTS, "status_cli guard claude");
        merge(map, CLAUDE_EVENTS, "status_cli guard claude");
        assert_eq!(hooks["PreToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(hooks["PreToolUse"][0]["hooks"][0]["command"], "rtk hook");
        assert_eq!(hooks["PreToolUse"][1]["matcher"], "Agent|Task|Workflow");
        assert_eq!(hooks["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert!(hooks["UserPromptSubmit"][0].get("matcher").is_none());
    }

    #[test]
    fn merge_drops_events_left_empty() {
        let mut hooks = json!({ "Stop": [{ "hooks": [{ "command": "status_cli guard codex" }] }] });
        merge(hooks.as_object_mut().unwrap(), CODEX_EVENTS, "status_cli guard codex");
        assert!(hooks.get("Stop").is_none());
        assert!(hooks["SessionStart"].is_array());
    }

    #[test]
    fn agy_plugin_hooks_start_and_every_invocation() {
        let (manifest, hooks) = agy_plugin("status_cli guard agy");
        assert!(manifest.contains(r#""name": "status_cli""#));
        let hooks: Value = serde_json::from_str(&hooks).unwrap();
        assert_eq!(hooks[AGY_HOOK]["PreInvocation"][0]["command"], "status_cli guard agy");
        assert!(is_guard(hooks[AGY_HOOK]["SessionStart"][0]["command"].as_str().unwrap()));
    }
}
