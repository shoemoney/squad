//! Neutral, explicitly labelled vote graphics; classification stays in core.
use super::grill::support;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use std::time::Duration;
pub use support::{Level, Motion};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Votes {
    pub agree: u32,
    pub disagree: u32,
    pub unsure: u32,
    pub missing: u32,
}
impl Votes {
    fn has_votes(self) -> bool {
        self.agree != 0 || self.disagree != 0 || self.unsure != 0
    }
    fn boundaries(self) -> [u64; 2] {
        let total = u64::from(self.agree) + u64::from(self.disagree) + u64::from(self.unsure);
        if total == 0 {
            return [0, 0];
        }
        [
            u64::from(self.agree) * 1_000_000 / total,
            (u64::from(self.agree) + u64::from(self.disagree)) * 1_000_000 / total,
        ]
    }
}
#[derive(Clone, Debug)]
pub struct SurveyEffect {
    pub motion: Motion,
    from: [u64; 2],
    current: Votes,
    initial: bool,
}
impl SurveyEffect {
    pub fn new(motion: Motion, key: u64, previous: Option<Votes>, current: Votes) -> Self {
        let mut effect = Self {
            motion,
            from: previous.map_or([0, 0], Votes::boundaries),
            current,
            initial: previous.is_none_or(|votes| !votes.has_votes()),
        };
        effect.motion.begin(key);
        effect
    }
    pub fn duration(&self) -> Duration {
        self.motion.duration(400)
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.motion.is_done(elapsed, 400)
    }
    pub fn boundaries(&self, elapsed: Duration, narrow: bool) -> [u64; 2] {
        let target = self.current.boundaries();
        if narrow || self.is_done(elapsed) {
            return target;
        }
        [
            support::tween(self.from[0], target[0], elapsed, self.duration()),
            support::tween(self.from[1], target[1], elapsed, self.duration()),
        ]
    }
    pub fn retarget(&mut self, key: u64, current: Votes, elapsed: Duration) -> bool {
        let from = self.boundaries(elapsed, false);
        if !self.motion.begin(key) {
            return false;
        }
        self.from = from;
        self.initial = !self.current.has_votes();
        self.current = current;
        true
    }
    /// Two graphic rows: authoritative labelled counts, then a patterned bar.
    /// Missing votes are labelled separately and never occupy UNSURE's segment.
    pub fn render_graphic(
        &self,
        buf: &mut Buffer,
        rect: Rect,
        elapsed: Duration,
        narrow: bool,
        colors: [Color; 3],
    ) {
        let rect = rect.intersection(buf.area);
        if rect.is_empty() {
            return;
        }
        let neutral = Style::default();
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                buf[(x, y)].set_symbol(" ").set_style(neutral);
            }
        }
        let v = self.current;
        // At narrow widths, stack every label: no category disappears.
        if narrow {
            for (i, line) in [
                format!("AGREE {}", v.agree),
                format!("DISAGREE {}", v.disagree),
                format!("UNSURE {}", v.unsure),
                format!("MISSING {}", v.missing),
            ]
            .iter()
            .enumerate()
            {
                if i < usize::from(rect.height) {
                    buf.set_stringn(
                        rect.x,
                        rect.y + i as u16,
                        line,
                        usize::from(rect.width),
                        neutral,
                    );
                }
            }
            return;
        }
        buf.set_stringn(
            rect.x,
            rect.y,
            format!(
                "AGREE {}  DISAGREE {}  UNSURE {}  MISSING {}",
                v.agree, v.disagree, v.unsure, v.missing
            ),
            usize::from(rect.width),
            neutral,
        );
        if rect.height < 2 {
            return;
        }
        let bounds = self.boundaries(elapsed, false);
        let initial = self.initial && !self.is_done(elapsed);
        // Initial tallies open from a neutral track. Revised tallies move only
        // the two split points, using the preceding committed proportions.
        let revealed = if initial {
            support::tween(0, u64::from(rect.width), elapsed, self.duration()) as u16
        } else {
            rect.width
        };
        for x in 0..rect.width {
            let cell = &mut buf[(rect.x + x, rect.y + 1)];
            if x >= revealed || v.agree == 0 && v.disagree == 0 && v.unsure == 0 {
                cell.set_symbol("·");
                continue;
            }
            let position = u64::from(x) * 1_000_000 / u64::from(rect.width);
            let segment = if position < bounds[0] {
                0
            } else if position < bounds[1] {
                1
            } else {
                2
            };
            cell.set_symbol(["#", "=", "?"][segment]);
            if !self.motion.no_color {
                cell.set_fg(colors[segment]);
            }
        }
    }
}
