use std::env;
use std::path::PathBuf;
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
    pub exhaust: char,
    pub missing: char,
    pub pad: char,
    pub hands: [char; 4],
}

const UNICODE: Glyphs = Glyphs {
    fill: '━',
    empty: '─',
    edge: '╸',
    separator: '│',
    reset: '↻',
    exhaust: '→',
    missing: '–',
    pad: '\u{2800}',
    hands: ['◴', '◷', '◶', '◵'],
};

const ASCII: Glyphs = Glyphs {
    fill: '=',
    empty: '-',
    edge: '>',
    separator: '|',
    reset: '~',
    exhaust: '!',
    missing: '-',
    pad: ' ',
    hands: ['|', '/', '-', '\\'],
};

const TRUECOLOR_PROGRAMS: [&str; 9] =
    ["iTerm.app", "WezTerm", "ghostty", "vscode", "Hyper", "Tabby", "WarpTerminal", "rio", "mintty"];
const TRUECOLOR_TERMS: [&str; 7] =
    ["xterm-kitty", "alacritty", "foot", "foot-extra", "xterm-ghostty", "wezterm", "contour"];
const VTE_TRUECOLOR: u32 = 3600;

const NERD_FONT: &str = "CaskaydiaCoveNerdFont-Regular.ttf";
const NERD_ICONS: [char; 4] = ['\u{f06a9}', '\u{f0150}', '\u{f00ed}', '\u{f01bc}'];
const NERD_TIMER: char = '\u{f051b}';
const NERD_CAPS: (char, char) = ('\u{e0b6}', '\u{e0b4}');
const PLAIN_ICONS: [char; 4] = ['◆', '◷', '▦', '≡'];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icons {
    None,
    Plain,
    Nerd,
}

impl Icons {
    pub fn model(self) -> Option<char> {
        self.pick(0)
    }

    pub fn session(self) -> Option<char> {
        self.pick(1)
    }

    pub fn week(self) -> Option<char> {
        self.pick(2)
    }

    pub fn context(self) -> Option<char> {
        self.pick(3)
    }

    pub fn timer(self, now: u64) -> Option<char> {
        match self {
            Icons::None => None,
            Icons::Plain => Some(UNICODE.hands[((now / 1000) % 4) as usize]),
            Icons::Nerd => Some(NERD_TIMER),
        }
    }

    pub fn rounded(self) -> Option<(char, char)> {
        (self == Icons::Nerd).then_some(NERD_CAPS)
    }

    fn pick(self, slot: usize) -> Option<char> {
        match self {
            Icons::None => None,
            Icons::Plain => Some(PLAIN_ICONS[slot]),
            Icons::Nerd => Some(NERD_ICONS[slot]),
        }
    }
}

pub struct Caps {
    pub depth: Depth,
    pub glyphs: &'static Glyphs,
    pub icons: Icons,
}

pub fn caps() -> &'static Caps {
    static CAPS: OnceLock<Caps> = OnceLock::new();
    CAPS.get_or_init(|| detect(|name| env::var(name).ok().filter(|v| !v.is_empty())))
}

fn detect(var: impl Fn(&str) -> Option<String>) -> Caps {
    let term = var("TERM").unwrap_or_default();
    let limited_console = term == "dumb" || term == "linux";
    let locale = var("LC_ALL").or_else(|| var("LC_CTYPE")).or_else(|| var("LANG")).unwrap_or_default();
    let wide_ambiguous = ["ja", "zh", "ko"].iter().any(|cjk| locale.starts_with(cjk));
    let ascii = var("STATUS_CLI_ASCII").is_some() || limited_console || wide_ambiguous;
    let icons = match var("STATUS_CLI_ICONS").as_deref() {
        _ if ascii => Icons::None,
        Some("none") => Icons::None,
        Some("plain") => Icons::Plain,
        Some("nerd") => Icons::Nerd,
        _ if nerd_font_installed(&var) => Icons::Nerd,
        _ => Icons::Plain,
    };
    Caps { depth: depth(&var, &term), glyphs: if ascii { &ASCII } else { &UNICODE }, icons }
}

fn nerd_font_installed(var: &impl Fn(&str) -> Option<String>) -> bool {
    let home = var("HOME").or_else(|| var("USERPROFILE")).map(PathBuf::from);
    [
        var("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Microsoft").join("Windows").join("Fonts")),
        var("WINDIR").map(|dir| PathBuf::from(dir).join("Fonts")),
        home.as_ref().map(|home| home.join(".local").join("share").join("fonts")),
        home.as_ref().map(|home| home.join("Library").join("Fonts")),
    ]
    .into_iter()
    .flatten()
    .any(|dir| dir.join(NERD_FONT).is_file())
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
    let vte = var("VTE_VERSION").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    if var("ConEmuANSI").is_some() {
        Depth::Ansi256
    } else if matches!(colorterm.as_str(), "truecolor" | "24bit")
        || var("WT_SESSION").is_some()
        || var("KONSOLE_VERSION").is_some()
        || var("KITTY_WINDOW_ID").is_some()
        || vte >= VTE_TRUECOLOR
        || TRUECOLOR_PROGRAMS.contains(&program.as_str())
        || TRUECOLOR_TERMS.contains(&term)
        || term.ends_with("-direct")
        || (cfg!(windows) && term.is_empty())
    {
        Depth::TrueColor
    } else if term == "linux" || term == "screen" || term.is_empty() {
        Depth::Ansi16
    } else {
        Depth::Ansi256
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Signature = (&'static str, &'static [(&'static str, &'static str)], Depth);

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
    fn known_terminal_signatures() {
        use Depth::*;
        let windows_default = if cfg!(windows) { TrueColor } else { Ansi16 };
        let cases: &[Signature] = &[
            ("Windows Terminal", &[("WT_SESSION", "x")], TrueColor),
            ("WSL inside Windows Terminal", &[("TERM", "xterm-256color"), ("WT_SESSION", "x")], TrueColor),
            ("cmd / PowerShell in conhost", &[], windows_default),
            ("ConEmu / Cmder", &[("ConEmuANSI", "ON")], Ansi256),
            (
                "Git Bash / MSYS2 / Cygwin (mintty)",
                &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "mintty")],
                TrueColor,
            ),
            ("Alacritty", &[("TERM", "alacritty")], TrueColor),
            ("WezTerm", &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "WezTerm")], TrueColor),
            ("Tabby", &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "Tabby")], TrueColor),
            ("Hyper", &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "Hyper")], TrueColor),
            ("PuTTY", &[("TERM", "xterm")], Ansi256),
            (
                "GNOME Terminal / Tilix / Terminator / Xfce (VTE)",
                &[("TERM", "xterm-256color"), ("VTE_VERSION", "7600")],
                TrueColor,
            ),
            ("old VTE", &[("TERM", "xterm-256color"), ("VTE_VERSION", "3400")], Ansi256),
            ("Konsole", &[("TERM", "xterm-256color"), ("KONSOLE_VERSION", "240800")], TrueColor),
            ("Kitty", &[("TERM", "xterm-kitty")], TrueColor),
            ("Foot", &[("TERM", "foot")], TrueColor),
            ("Ghostty", &[("TERM", "xterm-ghostty"), ("TERM_PROGRAM", "ghostty")], TrueColor),
            ("Linux virtual console", &[("TERM", "linux")], Ansi16),
            ("tmux", &[("TERM", "tmux-256color"), ("TERM_PROGRAM", "tmux")], Ansi256),
            ("tmux passing COLORTERM", &[("TERM", "tmux-256color"), ("COLORTERM", "truecolor")], TrueColor),
            ("GNU Screen", &[("TERM", "screen")], Ansi16),
            ("GNU Screen 256", &[("TERM", "screen-256color")], Ansi256),
            ("Zellij", &[("TERM", "xterm-256color"), ("ZELLIJ", "0"), ("COLORTERM", "truecolor")], TrueColor),
            ("Terminal.app", &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "Apple_Terminal")], Ansi256),
            ("iTerm2", &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "iTerm.app")], TrueColor),
            ("Warp", &[("TERM", "xterm-256color"), ("TERM_PROGRAM", "WarpTerminal")], TrueColor),
            ("dumb", &[("TERM", "dumb")], None),
        ];
        for (name, vars, expected) in cases {
            assert_eq!(caps_for(vars).depth, *expected, "{name}");
        }
    }

    #[test]
    fn multiplexers_and_plain_xterm_get_256_colors() {
        assert_eq!(caps_for(&[("TERM", "tmux-256color")]).depth, Depth::Ansi256);
        assert_eq!(caps_for(&[("TERM", "xterm")]).depth, Depth::Ansi256);
    }

    #[test]
    fn icons_follow_the_setting_then_the_installed_nerd_font() {
        assert_eq!(caps_for(&[("STATUS_CLI_ICONS", "nerd")]).icons, Icons::Nerd);
        assert_eq!(caps_for(&[("STATUS_CLI_ICONS", "nerd"), ("STATUS_CLI_ASCII", "1")]).icons, Icons::None);
        let fonts = env::temp_dir().join(format!("status_cli-fonts-{}", std::process::id()));
        let windows = fonts.join("Microsoft").join("Windows").join("Fonts");
        std::fs::create_dir_all(&windows).unwrap();
        let local = fonts.to_str().unwrap().to_owned();
        assert_eq!(caps_for(&[("LOCALAPPDATA", &local)]).icons, Icons::Plain);
        std::fs::write(windows.join(NERD_FONT), b"").unwrap();
        assert_eq!(caps_for(&[("LOCALAPPDATA", &local)]).icons, Icons::Nerd);
        assert_eq!(caps_for(&[("LOCALAPPDATA", &local), ("STATUS_CLI_ICONS", "plain")]).icons, Icons::Plain);
        std::fs::remove_dir_all(&fonts).unwrap();
    }

    #[test]
    fn cjk_locales_avoid_ambiguous_width_glyphs() {
        assert_eq!(caps_for(&[("LANG", "ja_JP.UTF-8")]).glyphs.fill, '=');
        assert_eq!(caps_for(&[("LC_ALL", "zh_CN.UTF-8"), ("LANG", "en_US.UTF-8")]).glyphs.fill, '=');
        assert_eq!(caps_for(&[("LANG", "pt_BR.UTF-8")]).glyphs.fill, '━');
    }

    #[test]
    fn ascii_can_be_forced() {
        assert_eq!(caps_for(&[("STATUS_CLI_ASCII", "1")]).glyphs.pad, ' ');
        assert_eq!(caps_for(&[]).glyphs.pad, '\u{2800}');
    }
}
