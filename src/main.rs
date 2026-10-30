mod agy;
mod claude;
mod codex;
mod i18n;
mod install;
mod render;
mod shared;
mod statusline;
mod usage;

use std::env;
use std::error::Error;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use codex::wrap;
use install::{setup, update};
use render::style;
use shared::payload::{self, Host, Payload};
use shared::state::Session;
use shared::telemetry::{self, Event, Timer};
use statusline::layout::Line;
use statusline::{clock, context, model, quota};
use usage::guard;

const MAX_INPUT_BYTES: u64 = 1 << 20;
const PAD_CHARS: [char; 2] = ['\u{2800}', ' '];

fn main() {
    let invoked_as = env::args_os()
        .next()
        .and_then(|arg| PathBuf::from(arg).file_stem().map(|stem| stem.to_string_lossy().to_ascii_lowercase()));
    if invoked_as.as_deref() == Some("codex") {
        return run_shim(env::args().skip(1).collect());
    }
    match env::args().nth(1).as_deref() {
        Some("setup") => return run_setup(),
        Some("doctor") => {
            let (lines, healthy) = setup::doctor();
            lines.iter().for_each(|line| println!("{line}"));
            std::process::exit(if healthy { 0 } else { 1 });
        }
        Some("update") => match update::run() {
            Ok(message) => return println!("{message}"),
            Err(err) => {
                eprintln!("status_cli: {err}");
                std::process::exit(1);
            }
        },
        Some("agy-groups") => match agy::refresh(&cache_dir(), now_millis()) {
            Ok(message) => return println!("{message}"),
            Err(err) => {
                eprintln!("status_cli: {err}");
                std::process::exit(1);
            }
        },
        Some("codex") => {
            return run_codex(env::args().skip(2).collect(), env::var_os(codex::SHIM_VAR).map(PathBuf::from));
        }
        Some("guard") => {
            let Some(host) = env::args().nth(2).as_deref().and_then(guard::host) else {
                eprintln!("status_cli: usage: status_cli guard <claude|codex|agy>");
                std::process::exit(2);
            };
            if let Some(reply) = guard::run(host, &cache_dir(), now_millis()) {
                print!("{reply}");
            }
            return;
        }
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

fn run_shim(args: Vec<String>) {
    let shim = env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    if env::var_os(codex::GUARD_VAR).is_some() || env::var_os(codex::SHIM_VAR).is_some() {
        let launch = codex::command(&args, shim.as_deref());
        let status = Command::new(&launch.program).args(&launch.args).envs(launch.env.iter().cloned()).status();
        exit_with(status, &launch.program);
    }
    if let Some(dir) = &shim
        && let Some(exe) = codex::installed_status_cli(dir)
    {
        if let Ok(status) = Command::new(&exe).arg("codex").args(&args).env(codex::SHIM_VAR, dir).status() {
            std::process::exit(status.code().unwrap_or(1));
        }
    }
    run_codex(args, shim)
}

fn exit_with(status: io::Result<ExitStatus>, program: &str) -> ! {
    match status {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(err) => {
            eprintln!("status_cli: could not start {program}: {err}");
            std::process::exit(1);
        }
    }
}

fn run_codex(args: Vec<String>, shim: Option<PathBuf>) {
    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let (watch, once) = (flag("--watch"), flag("--once"));
    let launching = !watch && !once;
    let launch = codex::command(&args, shim.as_deref());
    if launching && !codex::interactive(&args) {
        let status = Command::new(&launch.program).args(&launch.args).envs(launch.env.iter().cloned()).status();
        exit_with(status, &launch.program);
    }
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
    match wrap::run(&launch, |columns| render(Some(columns))) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("status_cli: could not start {}: {err}", launch.program);
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
    let payload = &usage::share(&cache_dir(), payload, now);
    Line {
        model: model::read(payload),
        quotas: quota::read(payload, now, session, agy_groups(payload, now).as_ref()),
        context: context::read(
            payload,
            if payload.host() == Host::Claude { claude::auto_compact(payload) } else { claude::AutoCompact::default() },
            session,
        ),
        elapsed: clock::read(payload, session, now),
        columns: payload
            .terminal_width
            .or(env_number("COLUMNS").map(|c| (c as usize).min(payload::MAX_COLUMNS)))
            .filter(|&c| c > 0),
    }
}

fn agy_groups(payload: &Payload, now: u64) -> Option<agy::Groups> {
    if payload.host() != Host::Agy {
        return None;
    }
    let buckets = payload.quota.iter().flat_map(|quota| quota.keys().map(String::as_str));
    agy::load(&cache_dir(), buckets, now)
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
