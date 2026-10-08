mod features;
mod layout;
mod payload;
mod state;
mod style;
mod terminal;

use std::env;
use std::error::Error;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use features::telemetry::{self, Event, Timer};
use features::{clock, context, model, quota};
use layout::Line;
use payload::Payload;
use state::Session;

const MAX_INPUT_BYTES: u64 = 1 << 20;

fn main() {
    let mut timer = Timer::start();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let payload = read_input().and_then(|input| {
        timer.mark("stdin");
        let payload = parse(&input);
        timer.mark("parse");
        payload
    });
    let output = match &payload {
        Ok(payload) => {
            let mut session = Session::open(&cache_dir(), payload.session_key().as_deref(), now);
            timer.mark("state_load");
            let line = line(payload, now, &mut session);
            timer.mark("features");
            let output = line.render(now);
            timer.mark("render");
            session.save();
            timer.mark("state_save");
            output
        }
        Err(err) => format!("status_cli: {err}"),
    };
    let _ = io::stdout().lock().write_all(output.as_bytes());
    timer.mark("stdout");

    if let Some(path) = env::var_os("STATUS_CLI_LOG") {
        let error = payload.as_ref().err().map(ToString::to_string);
        let payload = payload.as_ref().ok();
        let event = Event {
            now,
            host: payload.map(|p| p.host().name()),
            session: payload.and_then(Payload::session_key),
            error,
        };
        telemetry::record(path.as_ref(), event, &timer);
    }
}

fn read_input() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut input = Vec::with_capacity(4096);
    io::stdin().lock().take(MAX_INPUT_BYTES).read_to_end(&mut input)?;
    Ok(input)
}

fn parse(input: &[u8]) -> Result<Payload, Box<dyn Error>> {
    Ok(serde_json::from_slice(input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(input))?)
}

fn line(payload: &Payload, now: u64, session: &mut Session) -> Line {
    Line {
        model: model::read(payload),
        quotas: quota::read(payload, now, session),
        context: context::read(payload, env_number("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"), session),
        elapsed: clock::read(payload, session),
        columns: payload.terminal_width.or(env_number("COLUMNS").map(|c| c as usize)).filter(|&c| c > 0),
    }
}

fn cache_dir() -> PathBuf {
    let base = ["XDG_RUNTIME_DIR", "LOCALAPPDATA", "XDG_CACHE_HOME"]
        .into_iter()
        .find_map(env::var_os)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(env::temp_dir);
    base.join("status_cli")
}

fn env_number(name: &str) -> Option<f64> {
    env::var(name).ok()?.trim().parse().ok()
}
