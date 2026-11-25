//! Deterministic transcript chrome. Apply after a fresh static widget render.
//!
//! Rectangles use buffer coordinates. `protected` covers message bodies; only
//! the comet's colour pass may touch them. The insertion cell must be blank.
mod color;
use color::shade;
use std::time::Duration;

use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Full,
    Lite,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moment {
    Arrival,
    Streaming,
    Complete,
    Failure,
}

#[derive(Clone, Copy, Debug)]
pub struct Bubble {
    pub area: Rect,
    pub rail: Rect,
    pub header: Rect,
    pub footer: Rect,
    pub cursor: Option<Position>,
    /// Nearest-to-farthest terminal cells behind the insertion point, including
    /// the preceding wrapped line when necessary. None means no token there.
    pub tail: [Option<Position>; 4],
    /// Dedicated status slot: × in colour, [ERR] in NO_COLOR.
    pub status: Rect,
    pub hue: Color,
    /// Resolved default foreground, used to fade a Reset-colour footer.
    pub text: Color,
    pub error: Color,
    pub level: Level,
    pub no_color: bool,
    pub moment: Moment,
}

impl Bubble {
    /// Streaming alone is unbounded; the caller replaces it on completion/error.
    pub fn duration(&self) -> Option<Duration> {
        if self.level == Level::Off {
            return Some(Duration::ZERO);
        }
        if self.moment == Moment::Streaming {
            return None;
        }
        let ms = if self.level == Level::Off || self.no_color {
            0
        } else {
            match self.moment {
                Moment::Arrival if self.level == Level::Full => 280,
                Moment::Complete => 150,
                Moment::Failure if self.level == Level::Full => 240,
                _ => 0,
            }
        };
        Some(Duration::from_millis(ms))
    }

    pub fn is_done(&self, elapsed: Duration) -> bool {
        self.duration().is_some_and(|duration| elapsed >= duration)
    }

    /// Static final chrome is rendered here too, so Off is an exact reference.
    pub fn apply(&self, elapsed: Duration, buf: &mut Buffer, protected: &[Rect]) {
        let clip = self.area.intersection(buf.area);
        let rail = self.rail.intersection(clip);
        let animate = !self.is_done(elapsed) && self.level != Level::Off && !self.no_color;
        let ms = elapsed.as_millis();
        let progress = (ms as f64 / 280.0).min(1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        let visible = if animate && self.moment == Moment::Arrival {
            (f64::from(rail.height) * eased).ceil() as u16
        } else {
            rail.height
        };
        for y in rail.top()..rail.bottom() {
            let p = Position::new(rail.x, y);
            if rail.width == 0 || protected.iter().any(|r| r.contains(p)) {
                continue;
            }
            let cell = &mut buf[p];
            cell.set_fg(if self.no_color {
                Color::Reset
            } else if self.moment == Moment::Failure {
                self.error
            } else {
                if self.moment == Moment::Complete {
                    shade(self.hue, 0.5)
                } else {
                    self.hue
                }
            });
            let glyph = if self.no_color {
                "|"
            } else if animate && self.moment == Moment::Failure && (ms / 60) % 2 == 1 {
                "╎"
            } else {
                "▌"
            };
            cell.set_symbol(if y - rail.y < visible { glyph } else { " " });
            if self.moment == Moment::Complete {
                cell.modifier.insert(Modifier::DIM);
            }
            if self.moment == Moment::Failure && self.no_color {
                cell.modifier.insert(Modifier::REVERSED);
            }
        }
        if self.moment == Moment::Arrival {
            let header = self.header.intersection(clip);
            for y in header.top()..header.bottom() {
                for x in header.left()..header.right() {
                    let p = Position::new(x, y);
                    if protected.iter().any(|r| r.contains(p)) {
                        continue;
                    }
                    let ahead = f64::from(x - header.x) / f64::from(header.width.max(1)) >= eased;
                    buf[p].set_fg(if self.no_color {
                        Color::Reset
                    } else if animate && ahead {
                        Color::White
                    } else {
                        self.hue
                    });
                }
            }
        }
        if self.moment == Moment::Failure {
            let status = self.status.intersection(clip);
            let text = if self.no_color { "[ERR]" } else { "×" };
            for (i, glyph) in text.chars().take(usize::from(status.width)).enumerate() {
                let p = Position::new(status.x + i as u16, status.y);
                if status.height == 0 || protected.iter().any(|r| r.contains(p)) {
                    continue;
                }
                let mut encoded = [0; 4];
                buf[p]
                    .set_symbol(glyph.encode_utf8(&mut encoded))
                    .set_fg(if self.no_color {
                        Color::Reset
                    } else {
                        self.error
                    });
                if self.no_color {
                    buf[p].modifier.insert(Modifier::REVERSED);
                }
            }
        }
        if self.moment == Moment::Complete && animate {
            let footer = self.footer.intersection(clip);
            for y in footer.top()..footer.bottom() {
                for x in footer.left()..footer.right() {
                    let cell = &mut buf[(x, y)];
                    let foreground = if cell.fg == Color::Reset {
                        self.text
                    } else {
                        cell.fg
                    };
                    if matches!(foreground, Color::Rgb(..)) {
                        cell.set_fg(shade(foreground, (ms as f64 / 150.0).min(1.0)));
                    } else if ms < 75 {
                        cell.modifier.insert(Modifier::HIDDEN);
                    } else {
                        cell.modifier.insert(Modifier::DIM);
                    }
                }
            }
        }
        if let Some(cursor) = self.cursor.filter(|p| clip.contains(*p)) {
            let streaming = self.moment == Moment::Streaming;
            if streaming && !self.no_color && self.level == Level::Full {
                // Layout supplies real tail cells so wraps, wide glyphs and
                // combining sequences need no text reconstruction in the pass.
                for (index, p) in self.tail.iter().enumerate() {
                    let Some(p) = *p else {
                        continue;
                    };
                    let distance = index as u16 + 1;
                    if !clip.contains(p) || !protected.iter().any(|r| r.contains(p)) {
                        continue;
                    }
                    let cell = &mut buf[p];
                    cell.set_fg(if distance == 4 {
                        Color::Reset
                    } else {
                        shade(self.hue, 1.0 - f64::from(distance - 1) * 0.15)
                    });
                    if distance >= 2 {
                        cell.modifier.insert(Modifier::DIM);
                    }
                }
            }
            let glyph = if streaming {
                Some(if self.no_color { "_" } else { "█" })
            } else if self.moment == Moment::Complete && animate && ms < 120 {
                Some(["█", "▓", "▒", "░"][(ms / 30) as usize])
            } else {
                None
            };
            // A caller cannot accidentally erase a token or a wide glyph.
            if let Some(glyph) = glyph.filter(|_| {
                buf[cursor].symbol() == " " && !protected.iter().any(|r| r.contains(cursor))
            }) {
                buf[cursor].set_symbol(glyph).set_fg(if self.no_color {
                    Color::Reset
                } else {
                    self.hue
                });
            }
        }
    }
}
