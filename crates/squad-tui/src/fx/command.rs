//! Command transitions. The wipe never edits glyphs until its final hard cut.
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier},
};

use super::palette::darken as shade;
use std::time::Duration;

pub use crate::fx::FxLevel as Level;

#[derive(Clone, Copy, Debug)]
pub enum Command<'a> {
    /// `line` is a dedicated, unprotected chrome row; all stream cursors stop at once.
    Stop {
        cursors: &'a [Position],
        rails: &'a [Rect],
        line: Rect,
    },
    /// At the cut copy the TUI's authoritative cleared render, including its chrome.
    /// Full-buffer passes must be disabled when synchronized output is unavailable.
    Clear {
        cleared: &'a Buffer,
        synchronized: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct CommandEffect<'a> {
    pub area: Rect,
    pub kind: Command<'a>,
    pub gold: Color,
    pub level: Level,
    pub no_color: bool,
}

impl CommandEffect<'_> {
    pub fn duration(&self) -> Duration {
        let ms = if self.level != Level::Full || self.no_color {
            0
        } else {
            match self.kind {
                Command::Stop { .. } => 180,
                Command::Clear {
                    synchronized: true, ..
                } => 280,
                Command::Clear { .. } => 0,
            }
        };
        Duration::from_millis(ms)
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration()
    }

    pub fn apply(&self, elapsed: Duration, buf: &mut Buffer, protected: &[Rect]) {
        let area = self.area.intersection(buf.area);
        let done = self.is_done(elapsed);
        match self.kind {
            Command::Stop {
                cursors,
                rails,
                line,
            } => {
                for rail in rails {
                    let rail = rail.intersection(area);
                    for y in rail.top()..rail.bottom() {
                        for x in rail.left()..rail.right() {
                            buf[(x, y)].modifier.insert(Modifier::DIM);
                        }
                    }
                }
                for p in cursors {
                    if area.contains(*p) && !protected.iter().any(|r| r.contains(*p)) {
                        buf[*p]
                            .set_symbol(if self.no_color { "_" } else { "■" })
                            .set_fg(if self.no_color {
                                Color::Reset
                            } else {
                                self.gold
                            });
                    }
                }
                let line = line.intersection(area);
                if line.is_empty() {
                    return;
                }
                const TEXT: &str = "━━ stopped ━━";
                let width = TEXT.chars().count().min(usize::from(line.width)) as u16;
                let start = line.x + (line.width - width) / 2;
                let progress = if done {
                    1.0
                } else {
                    (elapsed.as_secs_f64() / 0.180).powi(2)
                };
                for (i, glyph) in TEXT.chars().take(usize::from(width)).enumerate() {
                    let p = Position::new(start + i as u16, line.y);
                    let distance = ((i as f64 + 0.5) - f64::from(width) / 2.0).abs();
                    if distance <= f64::from(width) * progress / 2.0
                        && !protected.iter().any(|r| r.contains(p))
                    {
                        let mut encoded = [0; 4];
                        buf[p].set_symbol(glyph.encode_utf8(&mut encoded)).set_fg(
                            if self.no_color {
                                Color::Reset
                            } else {
                                self.gold
                            },
                        );
                    }
                }
            }
            Command::Clear { cleared, .. } => {
                for y in area.top()..area.bottom() {
                    for x in area.left()..area.right() {
                        let p = Position::new(x, y);
                        if done {
                            if cleared.area.contains(p) {
                                buf[p] = cleared[p].clone();
                            } else {
                                buf[p].reset();
                            }
                        } else {
                            let row = f64::from(y - area.y) / f64::from(area.height.max(1));
                            let column = f64::from(x - area.x) / f64::from(area.width.max(1));
                            let front = elapsed.as_secs_f64() / 0.280 * 1.25;
                            if row + column * 0.25 < front {
                                // Style-only, including protected bodies and wide glyphs.
                                let cell = &mut buf[p];
                                cell.modifier.insert(Modifier::DIM);
                                let amount = ((front - row - column * 0.25) * 4.0).clamp(0.0, 1.0);
                                cell.set_fg(shade(cell.fg, 1.0 - amount * 0.65));
                            }
                        }
                    }
                }
            }
        }
    }
}
