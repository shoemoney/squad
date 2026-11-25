//! One bounded batch, ordered by stable grid position, with readable statuses.
use super::grill::support;
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use std::time::Duration;
pub use support::{Level, Motion};
#[derive(Clone, Copy, Debug)]
pub struct CellAccent {
    pub grid_index: usize,
    pub rect: Rect,
}
#[derive(Clone, Debug)]
pub struct CrosscheckEffect {
    pub motion: Motion,
    cells: Vec<CellAccent>,
    gold: Color,
}
impl CrosscheckEffect {
    pub fn new(motion: Motion, key: u64, cells: Vec<CellAccent>, gold: Color) -> Self {
        let mut effect = Self {
            motion,
            cells: Vec::new(),
            gold,
        };
        effect.replace(key, cells);
        effect
    }
    /// Pass committed results only; absent/pending cells are not manufactured.
    /// Sorting affects the accent schedule, never the grid or its content.
    pub fn replace(&mut self, key: u64, mut cells: Vec<CellAccent>) -> bool {
        if !self.motion.begin(key) {
            return false;
        }
        cells.sort_unstable_by_key(|c| c.grid_index);
        cells.dedup_by_key(|c| c.grid_index);
        self.cells = cells;
        true
    }
    pub fn duration(&self) -> Duration {
        self.motion
            .duration(if self.cells.len() <= 1 { 120 } else { 900 })
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.cells.is_empty()
            || self
                .motion
                .is_done(elapsed, self.duration().as_millis() as u64)
    }
    pub fn apply(
        &self,
        buf: &mut Buffer,
        elapsed: Duration,
        viewport: Rect,
        narrow_summary: Option<Rect>,
    ) {
        if self.is_done(elapsed) {
            return;
        }
        if let Some(summary) = narrow_summary {
            if elapsed < Duration::from_millis(150) {
                support::accent(
                    buf,
                    summary.intersection(viewport),
                    self.gold,
                    self.motion.no_color,
                    1.0 - elapsed.as_secs_f32() / 0.15,
                );
            }
            return;
        }
        let lite = self.motion.level == Level::Lite;
        for (i, cell) in self.cells.iter().enumerate() {
            let start = if lite || self.cells.len() <= 1 {
                0
            } else {
                i as u128 * 780 / (self.cells.len() - 1) as u128
            };
            let now = elapsed.as_millis();
            let span = if lite { 150 } else { 120 };
            if now >= start && now < start + span {
                support::accent(
                    buf,
                    cell.rect.intersection(viewport),
                    self.gold,
                    self.motion.no_color,
                    1.0 - (now - start) as f32 / span as f32,
                );
            }
        }
    }
}
