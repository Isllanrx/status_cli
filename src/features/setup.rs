use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

struct Host {
    name: &'static str,
    settings: PathBuf,
    status_line: Value,
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

fn hosts(home: &Path) -> [Host; 2] {
    let command = command();
    let claude_dir = env::var_os("CLAUDE_CONFIG_DIR").map_or_else(|| home.join(".claude"), PathBuf::from);
    [
        Host {
            name: "Claude Code",
            settings: claude_dir.join("settings.json"),
            status_line: json!({ "type": "command", "command": command, "refreshInterval": 1 }),
        },
        Host {
            name: "Antigravity CLI",
            settings: home.join(".gemini").join("antigravity-cli").join("settings.json"),
            status_line: json!({ "type": "command", "command": command, "enabled": true }),
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
    let mut settings = match &existing {
        Some(bytes) => match serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))? {
            Value::Object(map) => map,
            _ => return Err("settings file is not a JSON object".into()),
        },
        None => Map::new(),
    };
    let mut status_line = match settings.remove("statusLine") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    if let Value::Object(wanted) = &host.status_line {
        status_line.extend(wanted.clone());
    }
    settings.insert("statusLine".to_owned(), Value::Object(status_line));

    let backup = match existing {
        Some(bytes) => {
            let backup = host.settings.with_extension("json.bak-status_cli");
            fs::write(&backup, bytes)?;
            Some(backup)
        }
        None => None,
    };
    let staging = host.settings.with_extension("json.tmp-status_cli");
    fs::write(&staging, serde_json::to_string_pretty(&Value::Object(settings))? + "\n")?;
    fs::rename(&staging, &host.settings)?;
    Ok(backup)
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
}
