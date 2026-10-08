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
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use features::telemetry::{self, Event};
use features::{clock, context, model, quota};
use layout::Line;
use payload::Payload;
use state::Session;

const MAX_INPUT_BYTES: u64 = 1 << 20;

fn main() {
    let started = Instant::now();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let payload = read_payload();
    let output = match &payload {
        Ok(payload) => {
            let mut session = Session::open(&cache_dir(), payload.session_key().as_deref(), now);
            let output = line(payload, now, &mut session).render(now);
            session.save();
            output
        }
        Err(err) => format!("status_cli: {err}"),
    };
    let _ = io::stdout().lock().write_all(output.as_bytes());

    if let Some(path) = env::var_os("STATUS_CLI_LOG") {
        let error = payload.as_ref().err().map(ToString::to_string);
        let payload = payload.as_ref().ok();
        let event = Event {
            now,
            took: started.elapsed(),
            host: payload.map(|p| p.host().name()),
            session: payload.and_then(Payload::session_key),
            error,
        };
        telemetry::record(path.as_ref(), event);
    }
}

fn read_payload() -> Result<Payload, Box<dyn Error>> {
    let mut input = String::new();
    io::stdin().take(MAX_INPUT_BYTES).read_to_string(&mut input)?;
    Ok(serde_json::from_str(input.strip_prefix('\u{feff}').unwrap_or(&input))?)
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
    env::var_os("XDG_RUNTIME_DIR").map_or_else(env::temp_dir, PathBuf::from)
}

fn env_number(name: &str) -> Option<f64> {
    env::var(name).ok()?.trim().parse().ok()
}
