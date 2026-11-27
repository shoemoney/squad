//! Send, fan-out and persistent quote-selection accents over static widgets.
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier},
};

use super::palette::darken as shade;
use std::time::Duration;

pub use crate::fx::FxLevel as Level;

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub row: Rect,
    pub hue: Color,
}

#[derive(Clone, Copy, Debug)]
pub enum Dispatch<'a> {
    /// Gutter and input border are chrome-only rectangles.
    Launch { gutter: Rect, border: Rect },
    /// `origin` is the human rail; each route travels horizontally, then vertically.
    Spokes {
        origin: Position,
        targets: &'a [Target],
    },
    /// Selection persists after the entrance. `others` are dimmed colour-only.
    Quote { rail: Rect, others: &'a [Rect] },
}

#[derive(Clone, Copy, Debug)]
pub struct DispatchEffect<'a> {
    pub area: Rect,
    pub kind: Dispatch<'a>,
    pub gold: Color,
    pub level: Level,
    pub no_color: bool,
    /// Supply the whole terminal dimensions, not the transcript panel size.
    pub terminal: Rect,
    pub ansi16: bool,
}

impl DispatchEffect<'_> {
    pub fn duration(&self) -> Duration {
        let ms = if self.level == Level::Off || self.no_color {
            0
        } else {
            match self.kind {
                Dispatch::Launch { .. } => {
                    if self.level == Level::Full {
                        180
                    } else {
                        120
                    }
                }
                Dispatch::Spokes { targets, .. } => {
                    if targets.is_empty() {
                        0
                    } else {
                        280 + 40 * targets.len().saturating_sub(1) as u64
                    }
                }
                Dispatch::Quote { .. } => {
                    if self.level == Level::Full {
                        120
                    } else {
                        0
                    }
                }
            }
        };
        Duration::from_millis(ms)
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration()
    }

    pub fn apply(&self, elapsed: Duration, buf: &mut Buffer, protected: &[Rect]) {
        let clip = self.area.intersection(buf.area);
        let active = !self.is_done(elapsed);
        let ms = elapsed.as_millis();
        match self.kind {
            Dispatch::Launch { gutter, border } if active => {
                if ms < 120 {
                    for p in positions(border.intersection(clip)) {
                        if !protected.iter().any(|r| r.contains(p)) {
                            buf[p].set_fg(shade(self.gold, 1.0 - ms as f64 / 120.0 * 0.4));
                            if ms >= 60 {
                                buf[p].modifier.insert(Modifier::DIM);
                            }
                        }
                    }
                }
                let gutter = gutter.intersection(clip);
                if self.level == Level::Full && gutter.width > 0 && gutter.height > 0 {
                    let f = (ms as f64 / 180.0).min(1.0);
                    let y = gutter.bottom()
                        - 1
                        - ((f64::from(gutter.height - 1) * (1.0 - (1.0 - f).powi(3))) as u16);
                    self.blank_glyph(
                        Position::new(gutter.x, y),
                        "▸",
                        self.gold,
                        clip,
                        buf,
                        protected,
                    );
                }
            }
            Dispatch::Spokes { origin, targets } if active => {
                let narrow = self.terminal.width < 100 || self.terminal.height < 30;
                for (i, target) in targets.iter().enumerate() {
                    let Some(t) = ms.checked_sub(i as u128 * 40) else {
                        continue;
                    };
                    if t >= 280 {
                        continue;
                    }
                    let row = target.row.intersection(clip);
                    if row.is_empty() {
                        continue;
                    }
                    let hit = t >= 160;
                    if hit {
                        for p in positions(row) {
                            // Row pulse is style-only even when it overlaps text.
                            buf[p].set_fg(if self.ansi16 { self.gold } else { target.hue });
                            if t < 220 {
                                buf[p].modifier.insert(Modifier::BOLD);
                            }
                        }
                    }
                    if self.level != Level::Full || narrow || t >= 160 {
                        continue;
                    }
                    let end = Position::new(row.x, row.y);
                    let horizontal = origin.x.abs_diff(end.x) as u32;
                    let vertical = origin.y.abs_diff(end.y) as u32;
                    let length = horizontal + vertical;
                    let head = ((length as f64 * (1.0 - (1.0 - t as f64 / 160.0).powi(3))) as u32)
                        .min(length);
                    for step in head.saturating_sub(5)..=head {
                        let p = if step <= horizontal {
                            Position::new(
                                if end.x >= origin.x {
                                    origin.x.saturating_add(step as u16)
                                } else {
                                    origin.x.saturating_sub(step as u16)
                                },
                                origin.y,
                            )
                        } else {
                            let dy = (step - horizontal) as u16;
                            Position::new(
                                end.x,
                                if end.y >= origin.y {
                                    origin.y.saturating_add(dy)
                                } else {
                                    origin.y.saturating_sub(dy)
                                },
                            )
                        };
                        let hue = if self.ansi16 || step < length / 2 {
                            self.gold
                        } else {
                            target.hue
                        };
                        self.blank_glyph(
                            p,
                            if step <= horizontal { "━" } else { "┃" },
                            hue,
                            clip,
                            buf,
                            protected,
                        );
                    }
                }
            }
            Dispatch::Quote { rail, others } => {
                if self.level != Level::Lite && !self.no_color {
                    for other in others {
                        for p in positions(other.intersection(clip)) {
                            let foreground = buf[p].fg;
                            buf[p].set_fg(shade(foreground, 0.7));
                            if !matches!(buf[p].fg, Color::Rgb(..)) {
                                buf[p].modifier.insert(Modifier::DIM);
                            }
                        }
                    }
                }
                let rail = rail.intersection(clip);
                let count = if active {
                    (u128::from(rail.height) * ms / 120) as u16
                } else {
                    rail.height
                };
                for p in positions(rail) {
                    if protected.iter().any(|r| r.contains(p)) {
                        continue;
                    }
                    if self.no_color {
                        buf[p].modifier.insert(Modifier::REVERSED);
                    } else {
                        buf[p].set_symbol("┃").set_fg(self.gold);
                        if p.y - rail.y >= count {
                            buf[p].modifier.insert(Modifier::DIM);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn blank_glyph(
        &self,
        p: Position,
        glyph: &str,
        hue: Color,
        clip: Rect,
        buf: &mut Buffer,
        protected: &[Rect],
    ) {
        if clip.contains(p) && !protected.iter().any(|r| r.contains(p)) && buf[p].symbol() == " " {
            buf[p].set_symbol(glyph).set_fg(hue);
        }
    }
}

fn positions(rect: Rect) -> impl Iterator<Item = Position> {
    (rect.top()..rect.bottom())
        .flat_map(move |y| (rect.left()..rect.right()).map(move |x| Position::new(x, y)))
}

/// Every mode owns its effects hook; disabled modes emit no effects.
pub fn mode_enabled(kind: squad_protocol::ContestKind) -> bool {
    match kind {
        squad_protocol::ContestKind::Forge => super::forge::enabled(),
        squad_protocol::ContestKind::Tune => super::tune::enabled(),
        squad_protocol::ContestKind::BassAckwardsPrd => super::bass_ackwards_prd::enabled(),
        squad_protocol::ContestKind::Forecast => super::forecast::enabled(),
        squad_protocol::ContestKind::Decide => super::decide::enabled(),
        squad_protocol::ContestKind::Negotiate => super::negotiate::enabled(),
        squad_protocol::ContestKind::BlowItUp => super::blow_it_up::enabled(),
        squad_protocol::ContestKind::Readback => super::readback::enabled(),
        squad_protocol::ContestKind::Debugging => super::debugging::enabled(),
        squad_protocol::ContestKind::Trace => super::trace::enabled(),
        squad_protocol::ContestKind::Rank => super::rank::enabled(),
        _ => true,
    }
}
