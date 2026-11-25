//! Local deterministic presentation primitives; no application state or wall clock.
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use std::time::Duration;

pub use crate::fx::FxLevel as Level;

/// The caller settles before collapse, stop, failure or relayout. Reopening uses
/// the same instance (or Replay), so an old artifact cannot start again.
#[derive(Clone, Debug)]
pub struct Motion {
    pub level: Level,
    pub no_color: bool,
    key: Option<u64>,
    settled: bool,
    replay: bool,
}
impl Motion {
    pub fn new(level: Level, no_color: bool, replay: bool) -> Self {
        Self {
            level,
            no_color,
            key: None,
            settled: replay,
            replay,
        }
    }
    /// Keys identify committed artifacts within this contest, never token chunks.
    pub fn begin(&mut self, key: u64) -> bool {
        if self.key == Some(key) {
            return false;
        }
        self.key = Some(key);
        self.settled = self.replay;
        true
    }
    pub fn settle(&mut self) {
        self.settled = true;
    }
    pub fn duration(&self, full_ms: u64) -> Duration {
        Duration::from_millis(match self.level {
            Level::Full => full_ms,
            Level::Lite => 150,
            Level::Off => 0,
        })
    }
    pub fn is_done(&self, elapsed: Duration, full_ms: u64) -> bool {
        self.key.is_none() || self.settled || elapsed >= self.duration(full_ms)
    }
}

/// Integer QuadOut, exact even for endpoint values above f64's precision.
pub fn tween(from: u64, to: u64, elapsed: Duration, duration: Duration) -> u64 {
    let d = duration.as_millis().max(1);
    let t = elapsed.as_millis().min(d);
    let numerator = t * (2 * d - t);
    let delta = u128::from(from.abs_diff(to)) * numerator / (d * d);
    if to >= from {
        from + delta as u64
    } else {
        from - delta as u64
    }
}

/// Mutates styles only, clipped against both target and buffer. Always apply
/// after a fresh static render; settled passes deliberately do no work.
pub fn accent(buf: &mut Buffer, rect: Rect, color: Color, no_color: bool, strength: f32) {
    let area = rect.intersection(buf.area);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            highlight(&mut buf[(x, y)], color, no_color, strength);
        }
    }
}
pub fn highlight(cell: &mut ratatui::buffer::Cell, color: Color, no_color: bool, strength: f32) {
    if no_color {
        cell.modifier.insert(Modifier::BOLD);
    } else {
        cell.fg = match (cell.fg, color) {
            (Color::Rgb(..), Color::Rgb(..)) => {
                crate::fx::palette::interpolate(cell.fg, color, f64::from(strength))
            }
            _ => color,
        };
    }
}
