pub fn drop_status_row_mouse(data: &[u8], status_row: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if let Some(len) = sgr_mouse_on_row(&data[i..], status_row) {
            i += len;
            continue;
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

fn sgr_mouse_on_row(data: &[u8], status_row: u16) -> Option<usize> {
    let body = data.strip_prefix(b"\x1b[<")?;
    let end = body.iter().position(|b| matches!(b, b'M' | b'm'))?;
    let fields = std::str::from_utf8(&body[..end]).ok()?;
    let mut numbers = fields.split(';').map(|n| n.parse::<u16>().ok());
    let (_, _, row) = (numbers.next()??, numbers.next()??, numbers.next()??);
    (row == status_row && numbers.next().is_none()).then_some(3 + end + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_on_the_status_row_never_reach_the_child() {
        assert_eq!(drop_status_row_mouse(b"a\x1b[<0;10;24Mb\x1b[<0;10;24m", 24), b"ab");
        assert_eq!(drop_status_row_mouse(b"\x1b[<0;10;5M", 24), b"\x1b[<0;10;5M");
        assert_eq!(drop_status_row_mouse(b"\x1b[A", 24), b"\x1b[A");
    }
}
