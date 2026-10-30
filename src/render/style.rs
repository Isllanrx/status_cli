use std::fmt::{Display, Write};

use crate::render::terminal::{Depth, caps};

pub use super::text::{capitalize, duration, strip, title, truncate_visible, visible_width};

#[derive(Clone, Copy)]
pub struct Style {
    rgb: Option<[u8; 3]>,
    bg: Option<[u8; 3]>,
    bold: bool,
    dim: bool,
    underline: bool,
}

impl Style {
    const fn rgb(rgb: [u8; 3]) -> Self {
        Self { rgb: Some(rgb), bg: None, bold: false, dim: false, underline: false }
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
        if depth == Depth::None || (self.rgb.is_none() && self.bg.is_none() && !attributes.iter().any(|(on, _)| *on)) {
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
            separator = ";";
        }
        if let Some(rgb) = self.bg {
            out.push_str(separator);
            let [r, g, b] = rgb;
            let _ = match depth {
                Depth::TrueColor => write!(out, "48;2;{r};{g};{b}"),
                Depth::Ansi256 => write!(out, "48;5;{}", ansi256(rgb)),
                _ => write!(out, "{}", ansi16(rgb) + 10),
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

pub const LABEL: Style = Style::rgb([173, 186, 199]);
pub const TRACK: Style = Style::rgb([72, 79, 88]);
pub const RULE: Style = Style::rgb([110, 118, 129]);
pub const SOFT: Style = Style::rgb([145, 152, 161]);
pub const STRONG: Style = Style { rgb: None, bg: None, bold: true, dim: false, underline: false };
pub const VALUE: Style = Style::rgb([121, 192, 255]);
const PANEL_RGB: [u8; 3] = [22, 27, 34];
const PANEL: Style =
    Style { rgb: Some([230, 237, 243]), bg: Some(PANEL_RGB), bold: false, dim: false, underline: false };

pub struct Cell {
    pub icon: Option<char>,
    pub head: String,
    pub body: String,
}

pub fn paint_into(out: &mut String, style: Style, text: impl Display) {
    let styled = style.write_sgr(out, caps().depth);
    let _ = write!(out, "{text}");
    if styled {
        out.push_str("\x1b[0m");
    }
}

pub fn panel(row: &str, top: bool) -> String {
    let pad = caps().glyphs.pad;
    let mut fill = String::new();
    let deep = matches!(caps().depth, Depth::TrueColor | Depth::Ansi256);
    if !deep || !PANEL.write_sgr(&mut fill, caps().depth) {
        return format!("{pad}{row} ");
    }
    let body = row.replace("\x1b[0m", &format!("\x1b[0m{fill}"));
    match caps().icons.corners(top) {
        Some((left, right)) => {
            let edge = Style::rgb(PANEL_RGB);
            format!("{}{fill} {body} \x1b[0m{}", paint(edge, left), paint(edge, right))
        }
        None => format!("{fill}{pad}{body} \x1b[0m"),
    }
}

pub fn paint(style: Style, text: impl Display) -> String {
    let mut out = String::with_capacity(32);
    paint_into(&mut out, style, text);
    out
}

pub fn gradient(text: &str, from: [u8; 3], to: [u8; 3]) -> String {
    let steps = text.chars().count().saturating_sub(1).max(1) as f64;
    let mut out = String::with_capacity(text.len() * 20);
    for (i, ch) in text.chars().enumerate() {
        let t = i as f64 / steps;
        let rgb = std::array::from_fn(|k| (from[k] as f64 + (to[k] as f64 - from[k] as f64) * t).round() as u8);
        paint_into(&mut out, Style::rgb(rgb), ch);
    }
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
    let light = (r as u16 + g as u16 + b as u16) / 3 > 140;
    match r.max(g).max(b) - r.min(g).min(b) {
        spread if spread < 40 && light => 39,
        spread if spread < 40 => 90,
        _ if r > 180 && g > 150 && b < 150 => 33,
        _ if b > r.saturating_add(40) && b >= g => {
            if r > g {
                35
            } else {
                36
            }
        }
        _ if r > g.saturating_add(40) => 31,
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
    pub missing: Option<&'a str>,
}

impl Gauge<'_> {
    pub fn head(&self) -> String {
        paint(LABEL, title(self.label))
    }

    pub fn body(&self, frame: &Frame) -> String {
        let glyphs = caps().glyphs;
        let mut out = String::with_capacity(64 + frame.bar_width * 24);
        let Some(pct) = self.pct else {
            if frame.bar_width > 0 {
                let track: String = std::iter::repeat_n(glyphs.empty, frame.bar_width).collect();
                paint_into(&mut out, TRACK, format_args!("{track} "));
            }
            match self.missing {
                Some(text) => paint_into(&mut out, SOFT, title(text)),
                None => paint_into(&mut out, SOFT, glyphs.missing),
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Frame = Frame { now: 0, bar_width: 4, tight: false };

    fn gauge(pct: Option<f64>, growing: bool) -> Gauge<'static> {
        Gauge { label: "x", pct, growing, missing: None }
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
        assert_eq!(LABEL.sgr(Depth::Ansi256), "38;5;152");
        assert_eq!(heat(80.0).sgr(Depth::Ansi16), "33");
        assert_eq!(TRACK.sgr(Depth::Ansi16), "90");
        assert_eq!(LABEL.sgr(Depth::Ansi16), "39");
        assert_eq!(VALUE.sgr(Depth::Ansi16), "36");
        assert_eq!(Style::rgb([210, 168, 255]).sgr(Depth::Ansi16), "35");
        assert_eq!(STRONG.underline().sgr(Depth::Ansi16), "1;4");
    }

    #[test]
    fn gradient_runs_from_the_first_to_the_last_color() {
        let text = gradient("abc", [0, 0, 0], [200, 100, 50]);
        assert_eq!(strip(&text), "abc");
    }

    #[test]
    fn grays_map_to_the_256_gray_ramp() {
        assert_eq!(ansi256([60, 64, 82]), 60);
        assert_eq!(ansi256([118, 118, 118]), 243);
    }

    #[test]
    fn gauge_fills_by_usage_and_shows_placeholder() {
        assert_eq!(strip(&gauge(Some(79.0), false).body(&FRAME)), "━━━─ 79%");
        assert_eq!(strip(&gauge(None, false).body(&FRAME)), "──── –");
        assert_eq!(strip(&gauge(Some(7.4), false).body(&Frame { bar_width: 0, ..FRAME })), "7%");
    }

    #[test]
    fn growing_gauge_flashes_its_edge_on_odd_seconds() {
        let odd = Frame { now: 1_000, ..FRAME };
        assert_eq!(strip(&gauge(Some(50.0), true).body(&odd)), "━╸── 50%");
        assert_eq!(strip(&gauge(Some(50.0), true).body(&FRAME)), "━━── 50%");
    }

    #[test]
    fn capitalize_handles_empty() {
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("planning"), "Planning");
    }
}
