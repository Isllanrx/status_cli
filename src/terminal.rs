use std::env;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

pub struct Glyphs {
    pub fill: char,
    pub empty: char,
    pub edge: char,
    pub separator: char,
    pub reset: char,
    pub missing: char,
    pub pad: char,
    pub hands: [char; 4],
}

const UNICODE: Glyphs = Glyphs {
    fill: '━',
    empty: '─',
    edge: '╸',
    separator: '╱',
    reset: '↻',
    missing: '–',
    pad: '\u{2800}',
    hands: ['◴', '◷', '◶', '◵'],
};

const ASCII: Glyphs = Glyphs {
    fill: '=',
    empty: '-',
    edge: '>',
    separator: '/',
    reset: '~',
    missing: '-',
    pad: ' ',
    hands: ['|', '/', '-', '\\'],
};

const TRUECOLOR_PROGRAMS: [&str; 8] =
    ["iTerm.app", "WezTerm", "ghostty", "vscode", "Hyper", "Tabby", "WarpTerminal", "rio"];

pub struct Caps {
    pub depth: Depth,
    pub glyphs: &'static Glyphs,
}

pub fn caps() -> &'static Caps {
    static CAPS: OnceLock<Caps> = OnceLock::new();
    CAPS.get_or_init(|| detect(|name| env::var(name).ok().filter(|v| !v.is_empty())))
}

fn detect(var: impl Fn(&str) -> Option<String>) -> Caps {
    let term = var("TERM").unwrap_or_default();
    let limited_console = term == "dumb" || term == "linux";
    let ascii = var("STATUS_CLI_ASCII").is_some() || limited_console;
    Caps { depth: depth(&var, &term), glyphs: if ascii { &ASCII } else { &UNICODE } }
}

fn depth(var: &impl Fn(&str) -> Option<String>, term: &str) -> Depth {
    match var("STATUS_CLI_COLOR").as_deref() {
        Some("none") => return Depth::None,
        Some("16") => return Depth::Ansi16,
        Some("256") => return Depth::Ansi256,
        Some("truecolor") => return Depth::TrueColor,
        _ => {}
    }
    if var("NO_COLOR").is_some() || term == "dumb" {
        return Depth::None;
    }
    let colorterm = var("COLORTERM").unwrap_or_default();
    let program = var("TERM_PROGRAM").unwrap_or_default();
    if matches!(colorterm.as_str(), "truecolor" | "24bit")
        || var("WT_SESSION").is_some()
        || TRUECOLOR_PROGRAMS.contains(&program.as_str())
        || term.contains("direct")
        || (cfg!(windows) && term.is_empty())
    {
        Depth::TrueColor
    } else if term == "linux" || term.is_empty() && cfg!(not(windows)) && program.is_empty() {
        Depth::Ansi16
    } else {
        Depth::Ansi256
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_for(vars: &[(&str, &str)]) -> Caps {
        detect(|name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string()))
    }

    #[test]
    fn explicit_override_wins() {
        assert_eq!(caps_for(&[("STATUS_CLI_COLOR", "16"), ("COLORTERM", "truecolor")]).depth, Depth::Ansi16);
    }

    #[test]
    fn no_color_and_dumb_disable_colors() {
        assert_eq!(caps_for(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]).depth, Depth::None);
        assert_eq!(caps_for(&[("TERM", "dumb")]).depth, Depth::None);
    }

    #[test]
    fn modern_terminals_get_truecolor() {
        assert_eq!(caps_for(&[("TERM", "xterm-256color"), ("COLORTERM", "truecolor")]).depth, Depth::TrueColor);
        assert_eq!(caps_for(&[("WT_SESSION", "x")]).depth, Depth::TrueColor);
        assert_eq!(caps_for(&[("TERM", "xterm-256color"), ("TERM_PROGRAM", "iTerm.app")]).depth, Depth::TrueColor);
    }

    #[test]
    fn multiplexers_and_plain_xterm_get_256_colors() {
        assert_eq!(caps_for(&[("TERM", "tmux-256color")]).depth, Depth::Ansi256);
        assert_eq!(caps_for(&[("TERM", "screen")]).depth, Depth::Ansi256);
    }

    #[test]
    fn linux_console_gets_16_colors_and_ascii() {
        let caps = caps_for(&[("TERM", "linux")]);
        assert_eq!(caps.depth, Depth::Ansi16);
        assert_eq!(caps.glyphs.fill, '=');
    }

    #[test]
    fn ascii_can_be_forced() {
        assert_eq!(caps_for(&[("STATUS_CLI_ASCII", "1")]).glyphs.pad, ' ');
        assert_eq!(caps_for(&[]).glyphs.pad, '\u{2800}');
    }
}
