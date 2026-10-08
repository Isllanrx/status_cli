mod features;
mod i18n;
mod layout;
mod payload;
mod state;
mod style;
mod terminal;
mod time;

use std::env;
use std::error::Error;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use features::telemetry::{self, Event, Timer};
use features::{clock, codex, context, model, quota, setup, wrap};
use layout::Line;
use payload::Payload;
use state::Session;

const MAX_INPUT_BYTES: u64 = 1 << 20;
const PAD_CHARS: [char; 2] = ['\u{2800}', ' '];

fn main() {
    let invoked_as = env::args_os()
        .next()
        .and_then(|arg| PathBuf::from(arg).file_stem().map(|stem| stem.to_string_lossy().to_ascii_lowercase()));
    if invoked_as.as_deref() == Some(setup::CODEX_ALIAS) {
        return run_codex(env::args().skip(1).collect());
    }
    match env::args().nth(1).as_deref() {
        Some("setup") => return run_setup(),
        Some("doctor") => {
            let (lines, healthy) = setup::doctor();
            lines.iter().for_each(|line| println!("{line}"));
            std::process::exit(if healthy { 0 } else { 1 });
        }
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
    let (watch, once) = (flag("--watch"), flag("--once"));
    let launching = !watch && !once;
    let mut reader = match env::current_dir() {
        Ok(cwd) if launching => codex::Reader::for_launch(now_millis(), cwd),
        _ => codex::Reader::latest(),
    };
    let mut session: Option<(Option<String>, Session)> = None;
    let mut render = move |columns: Option<usize>| -> String {
        let now = now_millis();
        let mut payload = match reader.read(now) {
            Ok(payload) => payload,
            Err(err) => return format!("status_cli: {err}"),
        };
        payload.terminal_width = columns;
        let payload = payload.validated();
        let key = payload.session_key();
        if session.as_ref().is_none_or(|(open, _)| *open != key) {
            session = Some((key.clone(), Session::open(&cache_dir(), key.as_deref(), now)));
        }
        let Some((_, state)) = session.as_mut() else { return String::new() };
        state.advance(now);
        let output = line(&payload, now, state).render(now);
        state.save();
        output
    };
    let terminal_width = || terminal_size::terminal_size().map(|(width, _)| width.0 as usize);

    if once {
        println!("{}", render(terminal_width()));
        return;
    }
    if watch {
        loop {
            let mut stdout = io::stdout().lock();
            let _ = write!(stdout, "\r\x1b[2K{}", render(terminal_width()));
            let _ = stdout.flush();
            drop(stdout);
            thread::sleep(Duration::from_millis(1000 - now_millis() % 1000));
        }
    }
    let (program, program_args) = codex::command(&args);
    match wrap::run(&program, &program_args, |columns| render(Some(columns))) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("status_cli: could not start {program}: {err}");
            std::process::exit(1);
        }
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
        context: context::read(
            payload,
            context::AutoCompact {
                window: env_number("CLAUDE_CODE_AUTO_COMPACT_WINDOW"),
                percent: env_number("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"),
            },
            session,
        ),
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
