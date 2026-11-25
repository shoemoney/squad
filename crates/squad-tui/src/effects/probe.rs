//! Root-first connector trace over an already complete, accepted static tree.
use super::grill::support;
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use std::time::Duration;
pub use support::{Level, Motion};
/// Connector coordinates in root-to-leaf traversal order. Wrapped layouts must
/// supply their actual coordinates after layout, never positions in source text.
#[derive(Clone, Debug)]
pub struct Branch {
    pub connectors: Vec<(u16, u16)>,
}
#[derive(Clone, Debug)]
pub struct ProbeEffect {
    pub motion: Motion,
    connectors: Vec<(u16, u16)>,
    gold: Color,
}
impl ProbeEffect {
    /// Branch order comes directly from the accepted report. Rejected output
    /// must not call this constructor; pass `None` to `replace` instead.
    pub fn new(motion: Motion, key: u64, branches: &[Branch], gold: Color) -> Self {
        let mut effect = Self {
            motion,
            connectors: Vec::new(),
            gold,
        };
        effect.replace(key, Some(branches));
        effect
    }
    /// `None` is an unavailable or rejected report. Keep the last accepted tree
    /// and its current trace rather than treating rejected output as an update.
    pub fn replace(&mut self, key: u64, branches: Option<&[Branch]>) -> bool {
        let Some(branches) = branches else {
            return false;
        };
        if !self.motion.begin(key) {
            return false;
        }
        self.connectors.clear();
        self.connectors
            .extend(branches.iter().flat_map(|b| b.connectors.iter().copied()));
        true
    }
    pub fn duration(&self) -> Duration {
        self.motion.duration(900)
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.connectors.is_empty() || self.motion.is_done(elapsed, 900)
    }
    pub fn apply(&self, buf: &mut Buffer, elapsed: Duration, viewport: Rect) {
        if self.is_done(elapsed) {
            return;
        }
        let visible = if self.motion.level == Level::Lite {
            self.connectors.len()
        } else {
            (elapsed.as_millis() * self.connectors.len() as u128 / 900) as usize + 1
        };
        let area = viewport.intersection(buf.area);
        for &(x, y) in self.connectors.iter().take(visible) {
            if !area.contains((x, y).into()) {
                continue;
            }
            let cell = &mut buf[(x, y)];
            // A stale or erroneous geometry entry may never recolor node text.
            if matches!(
                cell.symbol(),
                "│" | "─" | "├" | "└" | "┬" | "┴" | "┼" | "┌" | "┐" | "┤" | "┘"
            ) {
                support::highlight(
                    cell,
                    self.gold,
                    self.motion.no_color,
                    1.0 - elapsed.as_secs_f32() / self.duration().as_secs_f32() * 0.5,
                );
            }
        }
    }
}
