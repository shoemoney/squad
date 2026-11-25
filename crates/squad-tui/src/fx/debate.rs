//! Deterministic debate graphics rendered only inside caller-owned inline rects.
//! Replay/resize/skip callers use Off, which always samples the final frame.
use super::particles::{Burst, ParticlePool, Point};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub use crate::fx::FxLevel as Level;

#[derive(Clone, Copy, Debug)]
pub struct Presentation {
    pub level: Level,
    pub no_color: bool,
}
impl Presentation {
    fn color(self, color: Color) -> Color {
        if self.no_color { Color::Reset } else { color }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Contender<'a> {
    pub name: &'a str,
    pub wins: u32,
    pub losses: u32,
    pub color: Color,
}

pub const VERSUS_DURATION: Duration = Duration::from_millis(1480);
pub const SCORE_DURATION: Duration = Duration::from_millis(520);
pub const TUG_DURATION: Duration = Duration::from_millis(400);
pub const LEAD_DURATION: Duration = Duration::from_millis(800);

#[derive(Clone, Copy, Debug)]
pub struct VersusCard<'a> {
    pub a: Contender<'a>,
    pub b: Contender<'a>,
    pub topic: &'a str,
    pub seed: u64,
}
impl VersusCard<'_> {
    pub fn is_done(&self, t: Duration) -> bool {
        t >= VERSUS_DURATION
    }

    /// Supply a freshly rendered dedicated card rectangle each frame. The card
    /// never exits or collapses: its final frame is the permanent transcript art.
    pub fn draw(&self, area: Rect, buf: &mut Buffer, t: Duration, p: Presentation) {
        let mut area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].reset();
            }
        }
        let narrow = area.width < 100 || area.height < 7;
        if narrow {
            area.height = area.height.min(4);
        }
        let static_frame = p.level == Level::Off || p.no_color || self.is_done(t);
        let progress = if static_frame {
            1.0
        } else {
            unit(t, if p.level == Level::Lite { 150.0 } else { 280.0 })
        };

        let left = p.color(self.a.color);
        let right = p.color(self.b.color);
        // Fixed frame/topic stay readable; only dedicated name plates travel.
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let border =
                    y == area.y || y == area.bottom() - 1 || x == area.x || x == area.right() - 1;
                if border {
                    buf[(x, y)]
                        .set_char(if y == area.y || y == area.bottom() - 1 {
                            '─'
                        } else {
                            '│'
                        })
                        .set_fg(if x < area.x + area.width / 2 {
                            left
                        } else {
                            right
                        });
                }
            }
        }
        let center = area.x + area.width / 2;
        let line_y = area.y + 1;
        let half = area.width.saturating_sub(6) / 2;
        let offset = 0;
        let a_label = format!("{}  W{} L{}", self.a.name, self.a.wins, self.a.losses);
        let b_label = format!("{}  W{} L{}", self.b.name, self.b.wins, self.b.losses);
        let style_a = Style::default().fg(left).add_modifier(Modifier::BOLD);
        let style_b = Style::default().fg(right).add_modifier(Modifier::BOLD);
        let x_a = i32::from(area.x) + 2 - offset;
        let x_b = i32::from(center) + 3 + offset;
        text_clipped(buf, area, x_a, line_y, half, &a_label, style_a);
        text_clipped(buf, area, x_b, line_y, half, &b_label, style_b);
        let flash = p.level == Level::Full && !p.no_color && (280..400).contains(&t.as_millis());
        let vs_style = Style::default()
            .fg(if flash {
                Color::White
            } else {
                p.color(Color::Yellow)
            })
            .add_modifier(Modifier::BOLD);
        if narrow {
            text(
                buf,
                area,
                center.saturating_sub(1),
                line_y,
                2,
                "VS",
                vs_style,
            );
        } else {
            for (i, line) in ["_   __ ____", "| | / / __/", "|___/___/  "]
                .iter()
                .enumerate()
            {
                text(
                    buf,
                    area,
                    center.saturating_sub(5),
                    area.y + 2 + i as u16,
                    11,
                    line,
                    vs_style,
                );
            }
        }
        let topic_y = area.y + if narrow { 2 } else { 5 };
        text(
            buf,
            area,
            area.x.saturating_add(2),
            topic_y,
            area.width.saturating_sub(4),
            self.topic,
            Style::default(),
        );
        if p.level == Level::Lite && progress < 1.0 && !p.no_color {
            for y in area.y..area.bottom() {
                for x in area.x..area.right() {
                    buf[(x, y)].modifier.insert(Modifier::DIM);
                }
            }
        }
        if p.level == Level::Full && !p.no_color && (280..880).contains(&t.as_millis()) {
            let mut sparks = ParticlePool::new(self.seed, 30.0);
            sparks.spawn(Burst {
                count: 24,
                origin: Point {
                    x: f32::from(center),
                    y: f32::from(area.y + area.height / 2),
                },
                velocity: Point { x: 0.0, y: -3.0 },
                spread: Point { x: 16.0, y: 7.0 },
                colors: &[left, right, Color::Yellow],
                glyphs: &['*', '·', '+'],
                lifetime: (Duration::from_millis(400), Duration::from_millis(600)),
            });
            sparks.step(t.saturating_sub(Duration::from_millis(280)));
            sparks.draw(area, buf);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scores {
    pub a: u32,
    pub b: u32,
}
impl Scores {
    pub fn leader(self) -> Option<Side> {
        match self.a.cmp(&self.b) {
            std::cmp::Ordering::Greater => Some(Side::A),
            std::cmp::Ordering::Less => Some(Side::B),
            _ => None,
        }
    }
    pub fn share(self) -> f64 {
        let total = u64::from(self.a) + u64::from(self.b);
        if total == 0 {
            0.5
        } else {
            f64::from(self.a) / total as f64
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

#[derive(Clone, Copy, Debug)]
pub struct ScoreSlam {
    pub scores: Scores,
    pub colors: [Color; 2],
}
impl ScoreSlam {
    pub fn is_done(&self, t: Duration) -> bool {
        t >= SCORE_DURATION
    }
    pub fn sample(&self, t: Duration, level: Level) -> Scores {
        let delay = if level == Level::Full { 120 } else { 0 };
        let progress = if level == Level::Off {
            1.0
        } else {
            quad_out(unit(t.saturating_sub(Duration::from_millis(delay)), 400.0))
        };
        Scores {
            a: (f64::from(self.scores.a) * progress).round() as u32,
            b: (f64::from(self.scores.b) * progress).round() as u32,
        }
    }
    pub fn draw(&self, area: Rect, buf: &mut Buffer, t: Duration, p: Presentation) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        let values = self.sample(t, p.level);
        let y = area.y
            + u16::from(
                !(p.level == Level::Full && t < Duration::from_millis(120)) && area.height > 1,
            );
        for (i, value) in [values.a, values.b].iter().enumerate() {
            let winner = self.scores.leader() == Some(if i == 0 { Side::A } else { Side::B });
            let flash = winner
                && p.level == Level::Full
                && !p.no_color
                && (400..520).contains(&t.as_millis());
            let style = Style::default()
                .fg(if flash {
                    Color::White
                } else {
                    p.color(self.colors[i])
                })
                .add_modifier(if winner {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                });
            let x = area.x + (area.width / 2) * i as u16;
            text(
                buf,
                area,
                x,
                y,
                area.width / 2,
                &format!("[ {value} ]"),
                style,
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TugBar {
    pub previous: Scores,
    pub current: Scores,
    pub colors: [Color; 2],
}
impl TugBar {
    pub fn is_done(&self, t: Duration) -> bool {
        t >= TUG_DURATION
    }
    pub fn split(&self, t: Duration, level: Level, width: u16) -> u16 {
        let progress = if level == Level::Off {
            1.0
        } else {
            quad_out(unit(t, 400.0))
        };
        let share =
            self.previous.share() + (self.current.share() - self.previous.share()) * progress;
        (share * f64::from(width)).round() as u16
    }
    pub fn draw(&self, area: Rect, buf: &mut Buffer, t: Duration, p: Presentation) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        let width = area.width.saturating_sub(1).min(29);
        let split = self.split(t, p.level, width);
        for i in 0..=width {
            let side = usize::from(i > split);
            let glyph = if i == split {
                '|'
            } else if p.no_color {
                if side == 0 { '#' } else { '.' }
            } else {
                '━'
            };
            buf[(area.x + i, area.y)]
                .set_char(glyph)
                .set_fg(p.color(self.colors[side]));
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LeadFlip {
    pub previous: Scores,
    pub current: Scores,
    pub color: Color,
}
impl LeadFlip {
    /// Equal scores and an initial lead never count as a flip.
    pub fn triggered(&self) -> bool {
        self.previous.leader().is_some()
            && self.current.leader().is_some()
            && self.previous.leader() != self.current.leader()
    }
    pub fn is_done(&self, t: Duration) -> bool {
        !self.triggered() || t >= LEAD_DURATION
    }
    /// The TUI supplies its rendered smblock label for wide displays (or None for
    /// a single-line fallback). The label and bar rectangles must exclude bodies.
    pub fn draw(
        &self,
        label: Rect,
        bar: Rect,
        buf: &mut Buffer,
        t: Duration,
        p: Presentation,
        art: Option<&[&str]>,
    ) {
        if self.is_done(t) || p.level == Level::Off {
            return;
        }
        let style = Style::default()
            .fg(p.color(self.color))
            .add_modifier(Modifier::BOLD);
        if p.level == Level::Full && label.width >= 100 && art.is_some() {
            for (i, line) in art
                .unwrap_or_default()
                .iter()
                .take(usize::from(label.height))
                .enumerate()
            {
                text(
                    buf,
                    label,
                    label.x,
                    label.y + i as u16,
                    label.width,
                    line,
                    style,
                );
            }
        } else {
            text(
                buf,
                label,
                label.x,
                label.y,
                label.width,
                "LEAD CHANGE",
                style,
            );
        }
        if p.level != Level::Full || p.no_color {
            return;
        }
        let bar = bar.intersection(buf.area);
        if bar.is_empty() {
            return;
        }
        let width = bar.width.min(30);
        let split = (self.current.share() * f64::from(width.saturating_sub(1))).round() as u16;
        let (start, end) = if self.current.leader() == Some(Side::A) {
            (0, split)
        } else {
            (split, width)
        };
        let x = start + ((f64::from(end.saturating_sub(start)) * unit(t, 800.0)).floor() as u16);
        if x < end {
            buf[(bar.x + x, bar.y)].set_fg(Color::White);
        }
    }
    /// Row-pulse input for the participant widget; no participant geometry is owned here.
    pub fn row_highlight(&self, t: Duration, p: Presentation) -> Option<Color> {
        if !self.is_done(t)
            && p.level == Level::Full
            && !p.no_color
            && t < Duration::from_millis(120)
        {
            Some(Color::White)
        } else {
            None
        }
    }
}
fn unit(t: Duration, ms: f64) -> f64 {
    (t.as_secs_f64() * 1000.0 / ms).clamp(0.0, 1.0)
}
fn quad_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(2)
}
fn text(buf: &mut Buffer, area: Rect, x: u16, y: u16, width: u16, s: &str, style: Style) {
    let area = area.intersection(buf.area);
    if x < area.x || x >= area.right() || y < area.y || y >= area.bottom() {
        return;
    }
    buf.set_stringn(x, y, s, usize::from(width.min(area.right() - x)), style);
}
fn text_clipped(buf: &mut Buffer, area: Rect, x: i32, y: u16, width: u16, s: &str, style: Style) {
    let clip = (i32::from(area.x) - x).max(0) as usize;
    let mut skipped = 0;
    let mut byte = 0;
    for (index, glyph) in s.grapheme_indices(true) {
        if skipped >= clip {
            break;
        }
        skipped += glyph.width();
        byte = index + glyph.len();
    }
    if skipped >= usize::from(width) {
        return;
    }
    let x = (x + skipped as i32).clamp(0, i32::from(u16::MAX)) as u16;
    text(
        buf,
        area,
        x,
        y,
        width.saturating_sub(skipped as u16),
        &s[byte..],
        style,
    );
}
