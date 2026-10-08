use std::error::Error;
use std::io::{self, IsTerminal, Read, Write};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use crossterm::terminal;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const TICK: Duration = Duration::from_millis(250);
const DRAIN: Duration = Duration::from_millis(60);
const MAX_ESCAPE_BYTES: usize = 64;

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
    let mut writer = pair.master.take_writer()?;
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut stdin = io::stdin().lock();
        while let Ok(read @ 1..) = stdin.read(&mut buffer) {
            if writer.write_all(&buffer[..read]).and_then(|_| writer.flush()).is_err() {
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

    let mut screen = Screen::new(cols, rows);
    let mut stdout = io::stdout().lock();
    stdout.write_all(&screen.reserve())?;
    let code = pump(&events, &mut screen, &mut stdout, &mut status, |cols, rows| {
        let _ = pair.master.resize(pty_size(cols, rows));
    });
    stdout.write_all(&screen.release())?;
    stdout.flush()?;
    drop(console);
    Ok(code as i32)
}

fn pump(
    events: &Receiver<Event>,
    screen: &mut Screen,
    stdout: &mut impl Write,
    status: &mut impl FnMut(usize) -> String,
    mut resize: impl FnMut(u16, u16),
) -> u32 {
    let mut line = status(screen.cols as usize);
    let mut exit = None;
    loop {
        let event = match exit {
            Some(_) => events.recv_timeout(DRAIN),
            None => events.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let frame = match event {
            Ok(Event::Output(bytes)) => screen.child_output(&bytes),
            Ok(Event::Tick) => {
                let mut frame = Vec::new();
                if let Ok((cols, rows)) = terminal::size()
                    && rows >= 3
                    && (cols, rows) != (screen.cols, screen.rows)
                {
                    resize(cols, rows);
                    frame = screen.resize(cols, rows);
                }
                line = status(screen.cols as usize);
                frame
            }
            Ok(Event::Exit(code)) => {
                exit = Some(code);
                continue;
            }
            Err(_) => return exit.unwrap_or(1),
        };
        let _ = stdout.write_all(&frame);
        let _ = stdout.write_all(&screen.status(&line));
        let _ = stdout.flush();
    }
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows - 1, cols, pixel_width: 0, pixel_height: 0 }
}

struct Screen {
    cols: u16,
    rows: u16,
    parser: vt100::Parser,
    pending: Vec<u8>,
}

impl Screen {
    fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows, parser: vt100::Parser::new(rows - 1, cols, 0), pending: Vec::new() }
    }

    fn reserve(&self) -> Vec<u8> {
        format!("\n\x1b[1A\x1b7\x1b[1;{}r\x1b8", self.rows - 1).into_bytes()
    }

    fn release(&self) -> Vec<u8> {
        let (row, col) = self.parser.screen().cursor_position();
        format!("\x1b[r\x1b[{};1H\x1b[0m\x1b[2K\x1b[{};{}H", self.rows, row + 1, col + 1).into_bytes()
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Vec<u8> {
        self.cols = cols;
        self.rows = rows;
        self.parser.screen_mut().set_size(rows - 1, cols);
        let (row, col) = self.parser.screen().cursor_position();
        format!("\x1b[1;{}r\x1b[{};{}H", rows - 1, row + 1, col + 1).into_bytes()
    }

    fn child_output(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(bytes);
        let keep = incomplete_tail(&data);
        self.pending = data.split_off(data.len() - keep);
        self.parser.process(&data);
        clamp_scroll_regions(&data, self.rows - 1)
    }

    fn status(&self, line: &str) -> Vec<u8> {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        let mut out =
            format!("\x1b[{};1H\x1b[0m\x1b[2K{line}\x1b[0m\x1b[{};{}H", self.rows, row + 1, col + 1).into_bytes();
        out.extend_from_slice(&screen.attributes_formatted());
        out
    }
}

fn incomplete_tail(data: &[u8]) -> usize {
    if let Some(start) = data.iter().rposition(|&b| b == 0x1b)
        && data.len() - start <= MAX_ESCAPE_BYTES
        && !escape_complete(&data[start..])
    {
        return data.len() - start;
    }
    let lead = data.iter().rev().take(4).position(|&b| b & 0xC0 != 0x80);
    match lead.map(|offset| (offset, data[data.len() - 1 - offset])) {
        Some((offset, byte)) if byte >= 0xC0 => {
            let needed = if byte >= 0xF0 {
                4
            } else if byte >= 0xE0 {
                3
            } else {
                2
            };
            if offset + 1 < needed { offset + 1 } else { 0 }
        }
        _ => 0,
    }
}

fn escape_complete(sequence: &[u8]) -> bool {
    match sequence.get(1) {
        None => false,
        Some(b'[') => sequence[2..].iter().any(|b| (0x40..=0x7E).contains(b)),
        Some(b']') => sequence.contains(&0x07) || sequence.windows(2).any(|w| w == b"\x1b\\"),
        Some(_) => true,
    }
}

fn clamp_scroll_regions(data: &[u8], limit: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && data.get(i + 1) == Some(&b'[') {
            let params = i + 2;
            let end = params + data[params..].iter().take_while(|b| b.is_ascii_digit() || **b == b';').count();
            if data.get(end) == Some(&b'r') {
                let text = String::from_utf8_lossy(&data[params..end]);
                let mut numbers = text.split(';').map(|n| n.parse::<u16>().ok().filter(|n| *n > 0));
                let top = numbers.next().flatten().unwrap_or(1);
                let bottom = numbers.next().flatten().unwrap_or(limit).min(limit);
                out.extend_from_slice(format!("\x1b[{};{}r", top.min(bottom), bottom).as_bytes());
                i = end + 1;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

#[cfg(windows)]
mod console {
    use std::io;

    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode,
    };

    pub struct Mode {
        input: CONSOLE_MODE,
        output: CONSOLE_MODE,
    }

    fn mode(handle: u32) -> CONSOLE_MODE {
        let mut mode = 0;
        unsafe { GetConsoleMode(GetStdHandle(handle), &mut mode) };
        mode
    }

    impl Mode {
        pub fn enter() -> io::Result<Self> {
            let (input, output) = (mode(STD_INPUT_HANDLE), mode(STD_OUTPUT_HANDLE));
            crossterm::terminal::enable_raw_mode()?;
            unsafe {
                SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), mode(STD_INPUT_HANDLE) | ENABLE_VIRTUAL_TERMINAL_INPUT);
                SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), output | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
            Ok(Self { input, output })
        }
    }

    impl Drop for Mode {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            unsafe {
                SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), self.input);
                SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), self.output);
            }
        }
    }
}

#[cfg(not(windows))]
mod console {
    use std::io;

    pub struct Mode;

    impl Mode {
        pub fn enter() -> io::Result<Self> {
            crossterm::terminal::enable_raw_mode()?;
            Ok(Self)
        }
    }

    impl Drop for Mode {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_regions_never_reach_the_status_row() {
        assert_eq!(clamp_scroll_regions(b"a\x1b[rb", 23), b"a\x1b[1;23rb");
        assert_eq!(clamp_scroll_regions(b"\x1b[5;40r", 23), b"\x1b[5;23r");
        assert_eq!(clamp_scroll_regions(b"\x1b[2;10r", 23), b"\x1b[2;10r");
        assert_eq!(clamp_scroll_regions(b"\x1b[?1049h\x1b[2J", 23), b"\x1b[?1049h\x1b[2J");
    }

    #[test]
    fn split_sequences_wait_for_the_next_chunk() {
        assert_eq!(incomplete_tail(b"abc\x1b[1;2"), 5);
        assert_eq!(incomplete_tail(b"abc\x1b[1;2r"), 0);
        assert_eq!(incomplete_tail(b"abc\x1b]0;title"), 9);
        assert_eq!(incomplete_tail("a\u{2501}".as_bytes()), 0);
        assert_eq!(incomplete_tail(&"a\u{2501}".as_bytes()[..3]), 2);
    }

    #[test]
    fn status_row_restores_the_child_cursor() {
        let mut screen = Screen::new(80, 24);
        screen.child_output(b"\x1b[5;7Hx");
        let frame = String::from_utf8(screen.status("line")).unwrap();
        assert!(frame.starts_with("\x1b[24;1H\x1b[0m\x1b[2Kline\x1b[0m\x1b[5;8H"), "{frame:?}");
    }
}
