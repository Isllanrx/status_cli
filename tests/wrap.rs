mod common;

use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use common::*;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
fn wraps_the_child_and_keeps_the_last_row_for_the_status_line() {
    let temp = TempDir::new();
    let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 100, pixel_width: 0, pixel_height: 0 }).unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_status_cli"));
    command.arg("codex");
    if cfg!(windows) {
        command.env("STATUS_CLI_CODEX", "cmd");
        command.args(["/d", "/c", "echo READY& exit /b 3"]);
    } else {
        command.env("STATUS_CLI_CODEX", "sh");
        command.args(["-c", "printf READY; exit 3"]);
    }
    command.env("CODEX_HOME", &temp.0);
    command.env("XDG_RUNTIME_DIR", &temp.0);
    command.env("NO_COLOR", "1");
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let (sender, received) = mpsc::channel();
    thread::spawn(move || {
        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);
        let _ = sender.send(output);
    });
    let status = child.wait().unwrap();
    drop(pair.master);
    let output = String::from_utf8_lossy(&received.recv_timeout(Duration::from_secs(10)).unwrap()).into_owned();

    assert_eq!(status.exit_code(), 3);
    assert!(output.contains("READY"), "{output:?}");
    assert!(output.contains("\x1b[1;23r"), "{output:?}");
    assert!(output.contains("\x1b[24;1H"), "{output:?}");
    assert!(output.contains("no Codex session found"), "{output:?}");
}
