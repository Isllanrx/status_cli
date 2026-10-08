use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use toml_edit::{Array, DocumentMut, Item, Table, value};

const CODEX_STATUS_ITEMS: [&str; 4] = ["model-with-reasoning", "five-hour-limit", "weekly-limit", "context-used"];

enum Config {
    Json(Value),
    CodexToml,
}

struct Host {
    name: &'static str,
    settings: PathBuf,
    config: Config,
}

pub fn run() -> Vec<Result<String, String>> {
    let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")).map(PathBuf::from) else {
        return vec![Err("home directory not found; nothing configured".to_owned())];
    };
    hosts(&home)
        .into_iter()
        .map(|host| {
            let outcome = match host.settings.parent() {
                Some(dir) if dir.is_dir() => configure(&host).map(|backup| match backup {
                    Some(backup) => format!("configured (backup: {})", backup.display()),
                    None => "configured".to_owned(),
                }),
                _ => Ok("not installed, skipped".to_owned()),
            };
            match outcome {
                Ok(message) => Ok(format!("{}: {message} -> {}", host.name, host.settings.display())),
                Err(err) => Err(format!("{}: not changed, {err} -> {}", host.name, host.settings.display())),
            }
        })
        .collect()
}

fn hosts(home: &Path) -> [Host; 3] {
    let command = command();
    let config_dir = |var: &str, default: PathBuf| env::var_os(var).map_or(default, PathBuf::from);
    [
        Host {
            name: "Claude Code",
            settings: config_dir("CLAUDE_CONFIG_DIR", home.join(".claude")).join("settings.json"),
            config: Config::Json(json!({ "type": "command", "command": command, "refreshInterval": 1 })),
        },
        Host {
            name: "Antigravity CLI",
            settings: home.join(".gemini").join("antigravity-cli").join("settings.json"),
            config: Config::Json(json!({ "type": "command", "command": command, "enabled": true })),
        },
        Host {
            name: "Codex CLI",
            settings: config_dir("CODEX_HOME", home.join(".codex")).join("config.toml"),
            config: Config::CodexToml,
        },
    ]
}

fn command() -> String {
    if cfg!(windows) {
        return "status_cli".to_owned();
    }
    match env::current_exe().and_then(|exe| exe.canonicalize()) {
        Ok(exe) => shell_quote(&exe.to_string_lossy()),
        Err(_) => "status_cli".to_owned(),
    }
}

fn shell_quote(path: &str) -> String {
    let plain = path.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'));
    if plain { path.to_owned() } else { format!("'{}'", path.replace('\'', r"'\''")) }
}

fn configure(host: &Host) -> Result<Option<PathBuf>, Box<dyn Error>> {
    let existing = match fs::read(&host.settings) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    let text = existing.as_deref().map(|bytes| bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes));
    let updated = match &host.config {
        Config::Json(status_line) => merge_json(text, status_line)?,
        Config::CodexToml => merge_codex(text.map(std::str::from_utf8).transpose()?.unwrap_or_default())?,
    };
    let backup = match existing {
        Some(bytes) => {
            let backup = with_suffix(&host.settings, ".bak-status_cli");
            fs::write(&backup, bytes)?;
            Some(backup)
        }
        None => None,
    };
    let staging = with_suffix(&host.settings, ".tmp-status_cli");
    fs::write(&staging, updated)?;
    fs::rename(&staging, &host.settings)?;
    Ok(backup)
}

fn merge_json(existing: Option<&[u8]>, wanted: &Value) -> Result<String, Box<dyn Error>> {
    let mut settings = match existing.map(serde_json::from_slice).transpose()? {
        Some(Value::Object(map)) => map,
        Some(_) => return Err("settings file is not a JSON object".into()),
        None => Map::new(),
    };
    let mut status_line = match settings.remove("statusLine") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    if let Value::Object(wanted) = wanted {
        status_line.extend(wanted.clone());
    }
    settings.insert("statusLine".to_owned(), Value::Object(status_line));
    Ok(serde_json::to_string_pretty(&Value::Object(settings))? + "\n")
}

fn merge_codex(existing: &str) -> Result<String, Box<dyn Error>> {
    let mut document: DocumentMut = existing.parse()?;
    let tui = document.entry("tui").or_insert(Item::Table(Table::new()));
    let tui = tui.as_table_like_mut().ok_or("[tui] in config.toml is not a table")?;
    tui.insert("status_line", value(CODEX_STATUS_ITEMS.into_iter().collect::<Array>()));
    tui.insert("status_line_use_colors", value(true));
    Ok(document.to_string())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_only_when_a_shell_would_split_them() {
        assert_eq!(shell_quote("/home/ana/.local/bin/status_cli"), "/home/ana/.local/bin/status_cli");
        assert_eq!(shell_quote("/Users/Ana Lu/bin/status_cli"), "'/Users/Ana Lu/bin/status_cli'");
        assert_eq!(shell_quote("/tmp/o'neil/status_cli"), r"'/tmp/o'\''neil/status_cli'");
    }

    #[test]
    fn codex_merge_keeps_comments_and_other_settings() {
        let before =
            "# mine\nmodel = \"gpt\"\n\n[tui]\nscreen_reader_detection_done = true\n\n[tui.model_availability_nux]\n";
        let after = merge_codex(before).unwrap();
        assert!(after.starts_with("# mine\nmodel = \"gpt\"\n"));
        assert!(after.contains("screen_reader_detection_done = true"));
        assert!(
            after.contains(
                r#"status_line = ["model-with-reasoning", "five-hour-limit", "weekly-limit", "context-used"]"#
            )
        );
        assert!(after.contains("status_line_use_colors = true"));
        assert!(after.contains("[tui.model_availability_nux]"));
        assert!(merge_codex(&after).unwrap() == after);
    }

    #[test]
    fn codex_merge_creates_the_table_and_rejects_broken_toml() {
        assert!(merge_codex("").unwrap().contains("[tui]"));
        assert!(merge_codex("tui = 3").is_err());
        assert!(merge_codex("[broken").is_err());
    }
}
