//! Stored range endpoint tween in an explicitly dedicated graphic rectangle.
use super::grill::support;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use std::time::Duration;
pub use support::{Level, Motion};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub low: u64,
    pub high: u64,
}
#[derive(Clone, Debug)]
pub struct CensusEffect {
    pub motion: Motion,
    from: Option<Estimate>,
    current: Option<Estimate>,
}
impl CensusEffect {
    pub fn new(
        motion: Motion,
        key: u64,
        previous: Option<Estimate>,
        current: Option<Estimate>,
    ) -> Self {
        let mut effect = Self {
            motion,
            from: previous,
            current,
        };
        effect.motion.begin(key);
        effect
    }
    pub fn duration(&self) -> Duration {
        self.motion.duration(400)
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.from.is_none() || self.current.is_none() || self.motion.is_done(elapsed, 400)
    }
    pub fn value(&self, elapsed: Duration, narrow: bool) -> Option<Estimate> {
        if narrow || self.is_done(elapsed) {
            return self.current;
        }
        match (self.from, self.current) {
            (Some(a), Some(b)) => Some(Estimate {
                low: support::tween(a.low, b.low, elapsed, self.duration()),
                high: support::tween(a.high, b.high, elapsed, self.duration()),
            }),
            _ => self.current,
        }
    }
    /// Replace a live artifact from its currently displayed endpoints. Returns
    /// false for duplicate delivery; caller resets elapsed only on true.
    pub fn retarget(&mut self, key: u64, current: Option<Estimate>, elapsed: Duration) -> bool {
        let from = self.value(elapsed, false);
        if !self.motion.begin(key) {
            return false;
        }
        self.from = from;
        self.current = current;
        true
    }
    /// This rect must exclude the exact headline, caveat, ledger and curve.
    /// Unavailable and first-pass estimates never acquire a fabricated origin.
    pub fn render_graphic(
        &self,
        buf: &mut Buffer,
        rect: Rect,
        elapsed: Duration,
        narrow: bool,
        color: Color,
    ) {
        let rect = rect.intersection(buf.area);
        if rect.is_empty() {
            return;
        }
        let text = match self.value(elapsed, narrow) {
            Some(v) => format!("LOW {}  HIGH {}", v.low, v.high),
            None => "Estimate unavailable".to_owned(),
        };
        let style = Style::default().fg(if self.motion.no_color {
            Color::Reset
        } else {
            color
        });
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                buf[(x, y)].set_symbol(" ").set_style(style);
            }
        }
        buf.set_stringn(rect.x, rect.y, text, usize::from(rect.width), style);
    }
}
