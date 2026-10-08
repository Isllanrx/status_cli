const MAX_ESCAPE_BYTES: usize = 64;
const SCREEN_SWITCHES: [&[u8]; 7] =
    [b"\x1b[?1049h", b"\x1b[?1049l", b"\x1b[?1047h", b"\x1b[?1047l", b"\x1b[?47h", b"\x1b[?47l", b"\x1bc"];
const RESTORE_MODES: &str =
    "\x1b[?2004l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[<u\x1b[?25h\x1b[0 q";

pub struct Stream {
    pub cols: u16,
    pub rows: u16,
    parser: vt100::Parser,
    pending: Vec<u8>,
    synchronized: bool,
    hyperlink: bool,
}

impl Stream {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            parser: vt100::Parser::new(rows - 1, cols, 0),
            pending: Vec::new(),
            synchronized: false,
            hyperlink: false,
        }
    }

    pub fn reserve(&self) -> Vec<u8> {
        format!("\n\x1b[1A\x1b7\x1b[1;{}r\x1b8", self.rows - 1).into_bytes()
    }

    pub fn seed_cursor(&mut self, row: u16, col: u16) {
        self.parser.process(format!("\x1b[{};{}H", row.min(self.rows - 2) + 1, col + 1).as_bytes());
    }

    pub fn release(&self) -> Vec<u8> {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        let leave_alternate = if screen.alternate_screen() { "\x1b[?1049l" } else { "" };
        format!(
            "{leave_alternate}{RESTORE_MODES}\x1b[r\x1b[{};1H\x1b[0m\x1b[2K\x1b[{};{}H",
            self.rows,
            row + 1,
            col + 1
        )
        .into_bytes()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Vec<u8> {
        self.cols = cols;
        self.rows = rows;
        self.parser.screen_mut().set_size(rows - 1, cols);
        self.region()
    }

    pub fn child_output(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(bytes);
        let keep = incomplete_tail(&data);
        self.pending = data.split_off(data.len() - keep);
        self.parser.process(&data);
        self.track_modes(&data);
        let mut out = clamp_scroll_regions(&data, self.rows - 1);
        if SCREEN_SWITCHES.iter().any(|switch| find(&data, switch).is_some()) {
            out.extend(self.region());
        }
        out
    }

    pub fn can_draw(&self) -> bool {
        let (_, col) = self.parser.screen().cursor_position();
        self.pending.is_empty() && !self.synchronized && !self.hyperlink && col + 1 < self.cols
    }

    pub fn status(&self, line: &str) -> Vec<u8> {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        let show = if screen.hide_cursor() { "" } else { "\x1b[?25h" };
        let mut out = format!("\x1b[?25l\x1b[{};1H\x1b[0m\x1b[2K{line}\x1b[0m\x1b[{};{}H", self.rows, row + 1, col + 1)
            .into_bytes();
        out.extend_from_slice(&screen.attributes_formatted());
        out.extend_from_slice(show.as_bytes());
        out
    }

    fn region(&self) -> Vec<u8> {
        let (row, col) = self.parser.screen().cursor_position();
        format!("\x1b[1;{}r\x1b[{};{}H", self.rows - 1, row + 1, col + 1).into_bytes()
    }

    fn track_modes(&mut self, data: &[u8]) {
        match (rfind(data, b"\x1b[?2026h"), rfind(data, b"\x1b[?2026l")) {
            (Some(on), Some(off)) => self.synchronized = on > off,
            (Some(_), None) => self.synchronized = true,
            (None, Some(_)) => self.synchronized = false,
            (None, None) => {}
        }
        if let Some(start) = rfind(data, b"\x1b]8;") {
            let rest = &data[start + 4..];
            let end = rest.iter().position(|&b| b == 0x07 || b == 0x1b).unwrap_or(rest.len());
            let uri = rest[..end].splitn(2, |&b| b == b';').nth(1).unwrap_or_default();
            self.hyperlink = !uri.is_empty();
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

pub fn incomplete_tail(data: &[u8]) -> usize {
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
        Some(b']' | b'P' | b'_') => sequence.contains(&0x07) || sequence.windows(2).any(|w| w == b"\x1b\\"),
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
    fn status_row_restores_the_child_cursor_and_visibility() {
        let mut stream = Stream::new(80, 24);
        stream.child_output(b"\x1b[5;7Hx");
        let frame = String::from_utf8(stream.status("line")).unwrap();
        assert!(frame.starts_with("\x1b[?25l\x1b[24;1H\x1b[0m\x1b[2Kline\x1b[0m\x1b[5;8H"), "{frame:?}");
        assert!(frame.ends_with("\x1b[?25h"), "{frame:?}");
    }

    #[test]
    fn drawing_waits_for_frames_links_and_partial_sequences() {
        let mut stream = Stream::new(80, 24);
        stream.child_output(b"\x1b[?2026hframe");
        assert!(!stream.can_draw());
        stream.child_output(b"\x1b[?2026l");
        assert!(stream.can_draw());
        stream.child_output(b"\x1b]8;;https://x\x1b\\link");
        assert!(!stream.can_draw());
        stream.child_output(b"\x1b]8;;\x1b\\");
        assert!(stream.can_draw());
        stream.child_output(b"\x1b[1;");
        assert!(!stream.can_draw());
    }

    #[test]
    fn seeded_cursor_and_screen_switches_keep_the_region() {
        let mut stream = Stream::new(80, 24);
        stream.seed_cursor(9, 4);
        let out = String::from_utf8(stream.child_output(b"\x1b[?1049h")).unwrap();
        assert!(out.contains("\x1b[1;23r\x1b["), "{out:?}");
        assert!(String::from_utf8(stream.release()).unwrap().starts_with("\x1b[?1049l"));
    }
}
