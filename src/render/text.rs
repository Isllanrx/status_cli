pub fn duration(ms: u64) -> String {
    let minutes = ms / 60_000;
    let (days, hours, mins) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
    match (days, hours) {
        (0, 0) => format!("{mins}m"),
        (0, _) => format!("{hours}h{mins:02}m"),
        _ => format!("{days}d{hours}h"),
    }
}

pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

pub fn title(s: &str) -> String {
    s.split(' ').map(capitalize).collect::<Vec<_>>().join(" ")
}

pub fn strip(s: &str) -> String {
    let mut in_escape = false;
    s.chars()
        .filter(|&c| {
            let visible = !in_escape && c != '\x1b';
            in_escape = (in_escape || c == '\x1b') && c != 'm';
            visible
        })
        .collect()
}

pub fn char_width(c: char) -> usize {
    match c as u32 {
        0x0300..=0x036F
        | 0x0610..=0x061A
        | 0x064B..=0x065F
        | 0x0670
        | 0x06D6..=0x06DC
        | 0x06DF..=0x06E4
        | 0x06E7..=0x06E8
        | 0x06EA..=0x06ED
        | 0x0900..=0x0902
        | 0x093A
        | 0x093C
        | 0x0941..=0x0948
        | 0x094D
        | 0x0951..=0x0957
        | 0x0962..=0x0963
        | 0x0981
        | 0x09BC
        | 0x09C1..=0x09C4
        | 0x09CD
        | 0x09E2..=0x09E3
        | 0x0E31
        | 0x0E34..=0x0E3A
        | 0x0E47..=0x0E4E
        | 0x200B..=0x200F
        | 0xFE00..=0xFE0F => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

pub fn truncate_visible(s: &str, width: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let (mut visible, mut in_escape) = (0, false);
    for c in s.chars() {
        let is_escape = in_escape || c == '\x1b';
        in_escape = is_escape && c != 'm';
        if !is_escape {
            if visible + char_width(c) > width {
                break;
            }
            visible += char_width(c);
        }
        out.push(c);
    }
    if out.contains('\x1b') {
        out.push_str("\x1b[0m");
    }
    out
}

pub fn visible_width(s: &str) -> usize {
    let mut in_escape = false;
    s.chars()
        .filter(|&c| {
            let visible = !in_escape && c != '\x1b';
            in_escape = (in_escape || c == '\x1b') && c != 'm';
            visible
        })
        .map(char_width)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_width_skips_escapes() {
        assert_eq!(visible_width("\x1b[1mab\x1b[0m┃"), 3);
    }

    #[test]
    fn truncation_keeps_escapes_and_closes_the_style() {
        let cut = truncate_visible("\x1b[1mabc\x1b[0mdef", 4);
        assert_eq!(cut, "\x1b[1mabc\x1b[0md\x1b[0m");
        assert_eq!(visible_width(&cut), 4);
        assert_eq!(truncate_visible("plain", 9), "plain");
    }

    #[test]
    fn wide_characters_take_two_columns() {
        assert_eq!(visible_width("会话 ab"), 7);
        assert_eq!(truncate_visible("会话ab", 3), "会");
    }
}
