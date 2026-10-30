use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::shared::telemetry;
use serde_json::Value;

const MARKER: &[u8] = b"\"turn_context\"";

#[derive(Default)]
pub struct Turns {
    file: Option<PathBuf>,
    offset: u64,
    last: Option<Value>,
}

impl Turns {
    pub fn latest(&mut self, file: &Path) -> Option<&Value> {
        if self.file.as_deref() != Some(file) {
            *self = Turns { file: Some(file.to_path_buf()), ..Turns::default() };
        }
        let opened = File::open(file).and_then(|mut handle| handle.seek(SeekFrom::Start(self.offset)).map(|_| handle));
        if let Some(handle) =
            telemetry::logged("codex.turns", opened.map_err(|err| format!("{}: {err}", file.display())))
        {
            let mut reader = BufReader::new(handle);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(read @ 1..) if line.ends_with(b"\n") => self.offset += read as u64,
                    _ => break,
                }
                if line.windows(MARKER.len()).any(|window| window == MARKER)
                    && let Ok(record) = serde_json::from_slice::<Value>(&line)
                    && record["type"] == "turn_context"
                {
                    self.last = Some(record["payload"].clone());
                }
            }
        }
        self.last.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    #[test]
    fn finds_the_last_turn_context_and_reads_only_new_lines_after() {
        let dir = std::env::temp_dir().join(format!("status_cli-codex-turns-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rollout.jsonl");
        let turn = |model: &str| format!("{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"{model}\"}}}}\n");
        fs::write(&file, turn("gpt-a") + "{\"type\":\"event_msg\"}\n").unwrap();
        let mut turns = Turns::default();
        assert_eq!(turns.latest(&file).and_then(|t| t["model"].as_str()), Some("gpt-a"));
        let mut append = OpenOptions::new().append(true).open(&file).unwrap();
        append.write_all(turn("gpt-b").as_bytes()).unwrap();
        append.write_all(b"{\"type\":\"turn_context\",\"payload\":{\"model\":\"partial").unwrap();
        assert_eq!(turns.latest(&file).and_then(|t| t["model"].as_str()), Some("gpt-b"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
