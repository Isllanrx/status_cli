use crate::features::clock;
use crate::features::context::Context;
use crate::features::model::Model;
use crate::features::quota::Quota;
use crate::style::{Frame, TRACK, paint, visible_width};
use crate::terminal::caps;

const RIGHT_MARGIN: usize = 8;
const LAYOUTS: [(usize, bool); 5] = [(10, false), (7, false), (5, false), (0, false), (0, true)];

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
        let mut line = String::new();
        for (bar_width, tight) in LAYOUTS {
            line = self.compose(&Frame { now, bar_width, tight });
            if room.is_none_or(|room| visible_width(&line) <= room) {
                break;
            }
        }
        let pad = room.map_or(0, |room| room.saturating_sub(visible_width(&line)));
        std::iter::repeat_n(caps().glyphs.pad, pad).chain(line.chars()).collect()
    }

    fn compose(&self, frame: &Frame) -> String {
        let segments = self
            .model
            .iter()
            .map(Model::render)
            .chain(self.quotas.iter().map(|q| q.render(frame)))
            .chain([self.context.render(frame)])
            .chain(self.elapsed.map(|ms| clock::render(ms, frame)));
        let separator =
            if frame.tight { "  ".to_owned() } else { paint(TRACK, format_args!(" {} ", caps().glyphs.separator)) };
        segments.collect::<Vec<_>>().join(&separator)
    }
}
