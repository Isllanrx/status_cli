use crate::render::style::{Cell, Frame, LABEL, RULE, gradient, paint, panel, truncate_visible, visible_width};
use crate::render::terminal::caps;
use crate::statusline::clock;
use crate::statusline::context::Context;
use crate::statusline::model::Model;
use crate::statusline::quota::Quota;

const RIGHT_MARGIN: usize = 8;
const CREDIT: &str = "By Isllan T.";
const LAYOUTS: [(bool, usize, bool); 8] = [
    (true, 10, false),
    (true, 7, false),
    (true, 5, false),
    (false, 10, false),
    (false, 7, false),
    (false, 5, false),
    (false, 0, false),
    (false, 0, true),
];

pub struct Line {
    pub model: Option<Model>,
    pub quotas: Vec<Quota>,
    pub context: Context,
    pub elapsed: Option<u64>,
    pub columns: Option<usize>,
}

impl Line {
    pub fn render(&self, now: u64) -> String {
        let room = self.columns.map(|c| c.saturating_sub(RIGHT_MARGIN));
        let fits = |rows: &[String; 2]| room.is_none_or(|room| rows.iter().all(|row| visible_width(row) <= room));
        let tight = Frame { now, bar_width: 0, tight: true };
        let rows = LAYOUTS
            .into_iter()
            .map(|(credit, bar_width, tight)| self.compose(&Frame { now, bar_width, tight }, credit, usize::MAX))
            .chain((1..self.segments()).rev().map(|keep| self.compose(&tight, false, keep)))
            .find(fits)
            .unwrap_or_else(|| self.compose(&tight, false, 1));
        rows.map(|row| align(row, room)).join("\n")
    }

    fn segments(&self) -> usize {
        usize::from(self.model.is_some()) + self.quotas.len() + 1 + usize::from(self.elapsed.is_some())
    }

    fn compose(&self, frame: &Frame, credit: bool, keep: usize) -> [String; 2] {
        let mut cells: Vec<Cell> = self.model.iter().map(Model::cell).collect();
        cells.extend(self.quotas.iter().map(|q| q.cell(frame)));
        cells.push(self.context.cell(frame));
        cells.extend(self.elapsed.map(|ms| clock::cell(ms, frame)));
        cells.truncate(keep);
        if credit {
            cells.push(Cell {
                icon: None,
                head: String::new(),
                body: gradient(CREDIT, [121, 192, 255], [210, 168, 255]),
            });
        }
        let separator = paint(RULE, format_args!(" {} ", caps().glyphs.separator));
        let mut rows = [Vec::new(), Vec::new()];
        for cell in cells {
            let head = match cell.icon {
                Some(icon) if !cell.head.is_empty() && !frame.tight => format!("{} {}", paint(LABEL, icon), cell.head),
                _ => cell.head,
            };
            let width = visible_width(&head).max(visible_width(&cell.body));
            rows[0].push(center(head, width));
            rows[1].push(fill(cell.body, width));
        }
        let [head, body] = rows.map(|row| row.join(&separator));
        [panel(&head, true), panel(&body, false)]
    }
}

fn center(text: String, width: usize) -> String {
    let gap = width.saturating_sub(visible_width(&text));
    " ".repeat(gap / 2) + &text + &" ".repeat(gap - gap / 2)
}

fn fill(text: String, width: usize) -> String {
    let gap = width.saturating_sub(visible_width(&text));
    text + &" ".repeat(gap)
}

fn align(mut line: String, room: Option<usize>) -> String {
    if let Some(room) = room.filter(|room| visible_width(&line) > *room) {
        line = truncate_visible(&line, room);
    }
    let pad = room.map_or(0, |room| room.saturating_sub(visible_width(&line)));
    let mut out = String::with_capacity(pad * 3 + line.len());
    out.extend(std::iter::repeat_n(caps().glyphs.pad, pad));
    out + &line
}
