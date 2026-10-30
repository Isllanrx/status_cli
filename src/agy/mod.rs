use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::shared::state::write_atomic;

const FILE: &str = "agy-quota-groups.json";
const LOCK: &str = "agy-quota-groups.lock";
const REFRESH_MS: u64 = 3_600_000;
const RETRY_MS: u64 = 120_000;
const TIMEOUT: Duration = Duration::from_secs(60);
const MODELS_PREFIX: &str = "Models within this group:";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Groups {
    pub fetched_at: u64,
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    pub models: Vec<String>,
    pub buckets: Vec<Bucket>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    pub id: String,
    pub window: String,
}

impl Groups {
    pub fn group_for(&self, model: &str) -> Option<&Group> {
        let model = words(model);
        self.groups
            .iter()
            .filter_map(|group| {
                let best = group
                    .models
                    .iter()
                    .chain([&group.name])
                    .map(|candidate| words(candidate))
                    .filter(|candidate| !candidate.is_empty() && candidate.iter().all(|word| model.contains(word)))
                    .map(|candidate| candidate.len())
                    .max()?;
                Some((best, group))
            })
            .max_by_key(|(score, _)| *score)
            .map(|(_, group)| group)
    }

    fn knows(&self, bucket: &str) -> bool {
        self.groups.iter().flat_map(|group| &group.buckets).any(|known| known.id == bucket)
    }
}

pub fn load<'a>(dir: &Path, buckets: impl IntoIterator<Item = &'a str>, now: u64) -> Option<Groups> {
    let cached: Option<Groups> = fs::read(dir.join(FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let mut buckets = buckets.into_iter();
    let stale = match &cached {
        None => true,
        Some(groups) => now.saturating_sub(groups.fetched_at) > REFRESH_MS || buckets.any(|id| !groups.knows(id)),
    };
    if stale {
        refresh_in_background(dir, now);
    }
    cached
}

pub fn refresh(dir: &Path, now: u64) -> Result<String, String> {
    let mut child = Command::new("agy")
        .args(["-p", "/usage", "--output-format", "json"])
        .current_dir(env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("could not start agy: {err}"))?;
    let output = collect(&mut child)?;
    let value: Value = serde_json::from_slice(&output).map_err(|err| format!("unreadable agy /usage output: {err}"))?;
    let groups = Groups { fetched_at: now, groups: parse(&value) };
    if groups.groups.is_empty() {
        return Err("agy /usage returned no quota groups".to_owned());
    }
    let json = serde_json::to_vec(&groups).map_err(|err| err.to_string())?;
    fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    write_atomic(&dir.join(FILE), &json).map_err(|err| err.to_string())?;
    Ok(format!("agy quota groups: {}", groups.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>().join(", ")))
}

fn collect(child: &mut Child) -> Result<Vec<u8>, String> {
    let mut stdout = child.stdout.take().ok_or("agy stdout unavailable")?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < TIMEOUT => thread::sleep(Duration::from_millis(200)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("agy /usage took longer than {}s", TIMEOUT.as_secs()));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    reader.join().map_err(|_| "agy output reader failed".to_owned())?.map_err(|err| err.to_string())
}

fn parse(value: &Value) -> Vec<Group> {
    value["command"]["data"]["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|group| {
            let name = group["name"].as_str()?.to_owned();
            let models = group["description"]
                .as_str()
                .and_then(|text| text.strip_prefix(MODELS_PREFIX))
                .map(|list| list.split(',').map(|model| model.trim().to_owned()).filter(|m| !m.is_empty()).collect())
                .unwrap_or_default();
            let buckets = group["buckets"]
                .as_array()?
                .iter()
                .filter_map(|bucket| {
                    Some(Bucket {
                        id: bucket["id"].as_str()?.to_owned(),
                        window: bucket["window"].as_str()?.to_owned(),
                    })
                })
                .collect();
            Some(Group { name, models, buckets })
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

fn refresh_in_background(dir: &Path, now: u64) {
    let lock = dir.join(LOCK);
    let recent = fs::read_to_string(&lock)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .is_some_and(|at| now.saturating_sub(at) < RETRY_MS);
    if recent || fs::create_dir_all(dir).and_then(|()| fs::write(&lock, now.to_string())).is_err() {
        return;
    }
    let Ok(exe) = env::current_exe() else { return };
    let command = || {
        let mut command = Command::new(&exe);
        command.arg("agy-groups").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        command
    };
    spawn_detached(command);
}

#[cfg(windows)]
fn spawn_detached(command: impl Fn() -> Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    if command().creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB).spawn().is_err() {
        let _ = command().creation_flags(flags).spawn();
    }
}

#[cfg(not(windows))]
fn spawn_detached(command: impl Fn() -> Command) {
    let _ = command().spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    const USAGE: &str = r#"{"command":{"name":"usage","data":{"groups":[
        {"name":"Gemini Models","description":"Models within this group: Gemini Flash, Gemini Pro",
         "buckets":[{"id":"gemini-weekly","window":"weekly"},{"id":"gemini-5h","window":"5h"}]},
        {"name":"Claude and GPT models","description":"Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
         "buckets":[{"id":"3p-weekly","window":"weekly"},{"id":"3p-5h","window":"5h","disabled":true}]},
        {"name":"Gemini 4 Argon","description":"For paid Pro plans, Argon quota is separate.",
         "buckets":[{"id":"gemini-argon-wtu-weekly","window":"weekly"}]}]}}}"#;

    fn groups() -> Groups {
        Groups { fetched_at: 0, groups: parse(&serde_json::from_str(USAGE).unwrap()) }
    }

    #[test]
    fn parses_the_official_groups() {
        let groups = groups();
        assert_eq!(groups.groups.len(), 3);
        assert_eq!(groups.groups[1].models, ["Claude Opus", "Claude Sonnet", "GPT-OSS"]);
        assert!(groups.groups[2].models.is_empty());
        assert_eq!(
            groups.groups[2].buckets,
            [Bucket { id: "gemini-argon-wtu-weekly".into(), window: "weekly".into() }]
        );
    }

    #[test]
    fn each_model_maps_to_the_group_agy_lists_it_in() {
        let groups = groups();
        let name = |model: &str| groups.group_for(model).map(|g| g.name.clone());
        assert_eq!(name("Gemini 3.8 Flash (High)").as_deref(), Some("Gemini Models"));
        assert_eq!(name("Gemini 3.1 Pro (Low)").as_deref(), Some("Gemini Models"));
        assert_eq!(name("Gemini 4 Argon (Medium)").as_deref(), Some("Gemini 4 Argon"));
        assert_eq!(name("Claude Sonnet 4.6 (Thinking)").as_deref(), Some("Claude and GPT models"));
        assert_eq!(name("GPT-OSS 120B (Medium)").as_deref(), Some("Claude and GPT models"));
        assert_eq!(name("Mystery Model"), None);
    }

    #[test]
    fn unknown_buckets_and_age_make_the_cache_stale() {
        let groups = groups();
        assert!(groups.knows("3p-5h"));
        assert!(!groups.knows("gemini-ultra-5h"));
    }
}
