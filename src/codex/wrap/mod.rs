mod console;
mod input;
mod stream;

use std::error::Error;
use std::io::{self, IsTerminal, Read, Write};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::terminal;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use stream::Stream;

const TICK: Duration = Duration::from_millis(250);
const QUIET: Duration = Duration::from_millis(30);
const DRAIN: Duration = Duration::from_millis(60);

enum Event {
    Output(Vec<u8>),
    Tick,
    Exit(u32),
}

pub fn run(program: &str, args: &[String], mut status: impl FnMut(usize) -> String) -> Result<i32, Box<dyn Error>> {
    let size = terminal::size().ok().filter(|(_, rows)| *rows >= 3);
    let Some((cols, rows)) = size.filter(|_| io::stdin().is_terminal() && io::stdout().is_terminal()) else {
        return Ok(Command::new(program).args(args).status()?.code().unwrap_or(1));
    };

    let pair = native_pty_system().openpty(pty_size(cols, rows))?;
    let mut command = CommandBuilder::new(program);
    command.args(args);
    command.cwd(std::env::current_dir()?);
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);

    let console = console::Mode::enter()?;
    let mut stream = Stream::new(cols, rows);
    let mut stdout = io::stdout().lock();
    stdout.write_all(&stream.reserve())?;
    stdout.flush()?;
    if cfg!(not(windows))
        && let Ok((col, row)) = crossterm::cursor::position()
    {
        stream.seed_cursor(row, col);
    }

    let (sender, events) = mpsc::channel();
    let mut reader = pair.master.try_clone_reader()?;
    let output = sender.clone();
    thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        while let Ok(read @ 1..) = reader.read(&mut buffer) {
            if output.send(Event::Output(buffer[..read].to_vec())).is_err() {
                break;
            }
        }
    });
    let status_row = Arc::new(AtomicU16::new(rows));
    let input_row = Arc::clone(&status_row);
    let mut writer = pair.master.take_writer()?;
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut stdin = io::stdin().lock();
        while let Ok(read @ 1..) = stdin.read(&mut buffer) {
            let keys = input::drop_status_row_mouse(&buffer[..read], input_row.load(Ordering::Relaxed));
            if writer.write_all(&keys).and_then(|_| writer.flush()).is_err() {
                break;
            }
        }
    });
    let ticks = sender.clone();
    thread::spawn(move || {
        while ticks.send(Event::Tick).is_ok() {
            thread::sleep(TICK);
        }
    });
    thread::spawn(move || {
        let code = child.wait().map_or(1, |status| status.exit_code());
        let _ = sender.send(Event::Exit(code));
    });

    let code = pump(&events, &mut stream, &mut stdout, &mut status, |cols, rows| {
        status_row.store(rows, Ordering::Relaxed);
        let _ = pair.master.resize(pty_size(cols, rows));
    });
    stdout.write_all(&stream.release())?;
    stdout.flush()?;
    drop(console);
    Ok(code as i32)
}

fn pump(
    events: &Receiver<Event>,
    stream: &mut Stream,
    stdout: &mut impl Write,
    status: &mut impl FnMut(usize) -> String,
    mut resize: impl FnMut(u16, u16),
) -> u32 {
    let mut line = status(stream.cols as usize);
    let mut exit = None;
    let mut dirty = true;
    let mut awaiting_redraw = false;
    let mut last_output = Instant::now();
    loop {
        match events.recv_timeout(if exit.is_some() { DRAIN } else { QUIET }) {
            Ok(Event::Output(bytes)) => {
                let _ = stdout.write_all(&stream.child_output(&bytes));
                last_output = Instant::now();
                awaiting_redraw = false;
                dirty = true;
            }
            Ok(Event::Tick) => {
                if let Ok((cols, rows)) = terminal::size()
                    && rows >= 3
                    && (cols, rows) != (stream.cols, stream.rows)
                {
                    let _ = stdout.write_all(&stream.resize(cols, rows));
                    resize(cols, rows);
                    awaiting_redraw = true;
                }
                line = status(stream.cols as usize);
                dirty = true;
            }
            Ok(Event::Exit(code)) => exit = Some(code),
            Err(RecvTimeoutError::Timeout) if exit.is_some() => return exit.unwrap_or(1),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return exit.unwrap_or(1),
        }
        if dirty && !awaiting_redraw && last_output.elapsed() >= QUIET && stream.can_draw() {
            let _ = stdout.write_all(&stream.status(&line));
            dirty = false;
        }
        let _ = stdout.flush();
    }
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows - 1, cols, pixel_width: 0, pixel_height: 0 }
}
