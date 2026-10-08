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
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use features::telemetry::{self, Event, Timer};
use features::{clock, codex, context, model, quota, setup};
use layout::Line;
use payload::Payload;
use state::Session;

const MAX_INPUT_BYTES: u64 = 1 << 20;
const PAD_CHARS: [char; 2] = ['\u{2800}', ' '];

fn main() {
    match env::args().nth(1).as_deref() {
        Some("setup") => return run_setup(),
        Some("codex") => return run_codex(env::args().skip(2).collect()),
        Some("--version" | "-V") => return println!("status_cli {}", env!("CARGO_PKG_VERSION")),
        _ => {}
    }
    let mut timer = Timer::start();
    let now = now_millis();
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
            model: payload.and_then(|p| p.model.as_ref()?.display_name.clone()),
            effort: payload
                .and_then(|p| p.effort.as_ref().map(|e| e.level.clone()).or_else(|| p.execution_mode.clone())),
            line: style::strip(&output).trim_start_matches(PAD_CHARS).to_owned(),
            error,
        };
        telemetry::record(path.as_ref(), event, &timer);
    }
}

fn run_codex(args: Vec<String>) {
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let until = args.iter().position(|arg| arg == "--until").and_then(|i| args.get(i + 1)).map(PathBuf::from);
    if !flag("--watch") && !flag("--once") {
        let marker = cache_dir().join(format!("codex-{}.pid", std::process::id()));
        match codex::launch(&args, &marker) {
            Ok(code) => std::process::exit(code),
            Err(err) => {
                eprintln!("status_cli: {err}");
                std::process::exit(1);
            }
        }
    }
    let watch = flag("--watch");
    loop {
        if until.as_deref().is_some_and(|marker| !marker.exists()) {
            return;
        }
        let now = now_millis();
        let line = match codex::payload(now) {
            Ok(mut payload) => {
                payload.terminal_width = terminal_size::terminal_size().map(|(width, _)| width.0 as usize);
                let payload = payload.validated();
                let mut session = Session::open(&cache_dir(), payload.session_key().as_deref(), now);
                let output = line(&payload, now, &mut session).render(now);
                session.save();
                output
            }
            Err(err) => format!("status_cli: {err}"),
        };
        let mut stdout = io::stdout().lock();
        if !watch {
            let _ = writeln!(stdout, "{line}");
            return;
        }
        let _ = write!(stdout, "\r\x1b[2K{line}");
        let _ = stdout.flush();
        drop(stdout);
        thread::sleep(Duration::from_millis(1000 - now % 1000));
    }
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn run_setup() {
    let mut failed = false;
    for outcome in setup::run() {
        match outcome {
            Ok(message) => println!("{message}"),
            Err(message) => {
                eprintln!("{message}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn read_input() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut input = Vec::with_capacity(4096);
    io::stdin().lock().take(MAX_INPUT_BYTES).read_to_end(&mut input)?;
    Ok(input)
}

fn parse(input: &[u8]) -> Result<Payload, Box<dyn Error>> {
    Ok(serde_json::from_slice::<Payload>(input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(input))?.validated())
}

fn line(payload: &Payload, now: u64, session: &mut Session) -> Line {
    Line {
        model: model::read(payload),
        quotas: quota::read(payload, now, session),
        context: context::read(payload, env_number("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"), session),
        elapsed: clock::read(payload, session, now),
        columns: payload
            .terminal_width
            .or(env_number("COLUMNS").map(|c| (c as usize).min(payload::MAX_COLUMNS)))
            .filter(|&c| c > 0),
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
