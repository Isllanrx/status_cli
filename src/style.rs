use std::fmt::{Display, Write};

use crate::terminal::{Depth, caps};

#[derive(Clone, Copy)]
pub struct Style {
    rgb: Option<[u8; 3]>,
    bold: bool,
    dim: bool,
    underline: bool,
}

impl Style {
    const fn rgb(rgb: [u8; 3]) -> Self {
        Self { rgb: Some(rgb), bold: false, dim: false, underline: false }
    }

    pub const fn bold(self) -> Self {
        Self { bold: true, dim: false, ..self }
    }

    pub const fn dim(self) -> Self {
        Self { dim: true, bold: false, ..self }
    }

    pub const fn underline(self) -> Self {
        Self { underline: true, ..self }
    }

    fn write_sgr(self, out: &mut String, depth: Depth) -> bool {
        let attributes = [(self.bold, "1"), (self.dim, "2"), (self.underline, "4")];
        if depth == Depth::None || (self.rgb.is_none() && !attributes.iter().any(|(on, _)| *on)) {
            return false;
        }
        out.push_str("\x1b[");
        let mut separator = "";
        for (on, code) in attributes {
            if on {
                out.push_str(separator);
                out.push_str(code);
                separator = ";";
            }
        }
        if let Some(rgb) = self.rgb {
            out.push_str(separator);
            let [r, g, b] = rgb;
            let _ = match depth {
                Depth::TrueColor => write!(out, "38;2;{r};{g};{b}"),
                Depth::Ansi256 => write!(out, "38;5;{}", ansi256(rgb)),
                _ => write!(out, "{}", ansi16(rgb)),
            };
        }
        out.push('m');
        true
    }

    #[cfg(test)]
    fn sgr(self, depth: Depth) -> String {
        let mut out = String::new();
        if self.write_sgr(&mut out, depth) {
            out.drain(..2);
            out.pop();
        }
        out
    }
}

pub const LABEL: Style = Style::rgb([124, 130, 156]);
pub const TRACK: Style = Style::rgb([60, 64, 82]);
pub const SOFT: Style = Style::rgb([92, 97, 120]);
pub const STRONG: Style = Style { rgb: None, bold: true, dim: false, underline: false };

pub fn paint_into(out: &mut String, style: Style, text: impl Display) {
    let styled = style.write_sgr(out, caps().depth);
    let _ = write!(out, "{text}");
    if styled {
        out.push_str("\x1b[0m");
    }
}

pub fn paint(style: Style, text: impl Display) -> String {
    let mut out = String::with_capacity(32);
    paint_into(&mut out, style, text);
    out
}

pub fn heat(pct: f64) -> Style {
    const STOPS: [(f64, [f64; 3]); 4] = [
        (0.0, [110.0, 214.0, 170.0]),
        (55.0, [150.0, 210.0, 140.0]),
        (75.0, [236.0, 186.0, 92.0]),
        (100.0, [240.0, 98.0, 110.0]),
    ];
    let p = pct.clamp(0.0, 100.0);
    let i = STOPS.iter().position(|(stop, _)| p <= *stop).unwrap_or(STOPS.len() - 1).max(1);
    let ((p0, c0), (p1, c1)) = (STOPS[i - 1], STOPS[i]);
    let t = (p - p0) / (p1 - p0);
    Style::rgb(std::array::from_fn(|k| (c0[k] + (c1[k] - c0[k]) * t).round() as u8))
}

fn ansi256([r, g, b]: [u8; 3]) -> u8 {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max - min < 16 {
        let gray = (r as u16 + g as u16 + b as u16) / 3;
        return if gray < 8 {
            16
        } else if gray > 238 {
            231
        } else {
            232 + ((gray - 8) / 10) as u8
        };
    }
    let level = |c: u8| (c as u16 * 5 + 127) / 255;
    (16 + 36 * level(r) + 6 * level(g) + level(b)) as u8
}

fn ansi16([r, g, b]: [u8; 3]) -> u8 {
    match (r.max(g).max(b) - r.min(g).min(b), r, g) {
        (spread, ..) if spread < 40 => 90,
        (_, r, g) if r > 180 && g > 150 => 33,
        (_, r, g) if r > g.saturating_add(40) => 31,
        _ => 32,
    }
}

pub struct Frame {
    pub now: u64,
    pub bar_width: usize,
    pub tight: bool,
}

impl Frame {
    pub fn odd_second(&self) -> bool {
        (self.now / 1000) % 2 == 1
    }
}

pub struct Gauge<'a> {
    pub label: &'a str,
    pub pct: Option<f64>,
    pub growing: bool,
}

impl Gauge<'_> {
    pub fn render(&self, frame: &Frame) -> String {
        let glyphs = caps().glyphs;
        let mut out = String::with_capacity(64 + frame.bar_width * 24);
        paint_into(&mut out, LABEL, format_args!("{} ", self.label));
        let Some(pct) = self.pct else {
            if frame.bar_width > 0 {
                let track: String = std::iter::repeat_n(glyphs.empty, frame.bar_width).collect();
                paint_into(&mut out, TRACK, format_args!("{track} "));
            }
            paint_into(&mut out, SOFT, glyphs.missing);
            return out;
        };
        if frame.bar_width > 0 {
            self.bar(&mut out, pct, frame.bar_width, self.growing && frame.odd_second());
            out.push(' ');
        }
        let flash = frame.odd_second() && (self.growing || pct >= 90.0);
        let tone = if flash { heat(pct).dim() } else { heat(pct).bold() };
        paint_into(&mut out, tone, format_args!("{}%", pct.round()));
        out
    }

    fn bar(&self, out: &mut String, pct: f64, width: usize, flash_edge: bool) {
        let glyphs = caps().glyphs;
        let filled = (pct.clamp(0.0, 100.0) / 100.0 * width as f64).round() as usize;
        for i in 0..width {
            match i < filled {
                true if flash_edge && i + 1 == filled => paint_into(out, heat(pct).bold(), glyphs.edge),
                true => paint_into(out, heat((i + 1) as f64 / width as f64 * 100.0), glyphs.fill),
                false => paint_into(out, TRACK, glyphs.empty),
            }
        }
    }
}

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

    const FRAME: Frame = Frame { now: 0, bar_width: 4, tight: false };

    fn gauge(pct: Option<f64>, growing: bool) -> Gauge<'static> {
        Gauge { label: "x", pct, growing }
    }

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

    #[test]
    fn duration_formats() {
        assert_eq!(duration(59 * 60_000), "59m");
        assert_eq!(duration(89 * 60_000), "1h29m");
        assert_eq!(duration((2 * 1440 + 180) * 60_000), "2d3h");
    }

    #[test]
    fn styles_degrade_with_color_depth() {
        let coral = heat(100.0).bold();
        assert_eq!(coral.sgr(Depth::TrueColor), "1;38;2;240;98;110");
        assert_eq!(coral.sgr(Depth::Ansi256), "1;38;5;210");
        assert_eq!(coral.sgr(Depth::Ansi16), "1;31");
        assert_eq!(heat(0.0).sgr(Depth::Ansi16), "32");
        assert_eq!(LABEL.sgr(Depth::Ansi256), "38;5;109");
        assert_eq!(heat(80.0).sgr(Depth::Ansi16), "33");
        assert_eq!(TRACK.sgr(Depth::Ansi16), "90");
        assert_eq!(STRONG.underline().sgr(Depth::Ansi16), "1;4");
    }

    #[test]
    fn grays_map_to_the_256_gray_ramp() {
        assert_eq!(ansi256([60, 64, 82]), 60);
        assert_eq!(ansi256([118, 118, 118]), 243);
    }

    #[test]
    fn gauge_fills_by_usage_and_shows_placeholder() {
        assert_eq!(strip(&gauge(Some(79.0), false).render(&FRAME)), "x ━━━─ 79%");
        assert_eq!(strip(&gauge(None, false).render(&FRAME)), "x ──── –");
        assert_eq!(strip(&gauge(Some(7.4), false).render(&Frame { bar_width: 0, ..FRAME })), "x 7%");
    }

    #[test]
    fn growing_gauge_flashes_its_edge_on_odd_seconds() {
        let odd = Frame { now: 1_000, ..FRAME };
        assert_eq!(strip(&gauge(Some(50.0), true).render(&odd)), "x ━╸── 50%");
        assert_eq!(strip(&gauge(Some(50.0), true).render(&FRAME)), "x ━━── 50%");
    }

    #[test]
    fn capitalize_handles_empty() {
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("planning"), "Planning");
    }
}
