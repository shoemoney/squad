//! Bounded chrome passes, applied after the static widget render.
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Full,
    Lite,
    Off,
}

#[derive(Clone, Copy, Debug)]
pub struct Chrome {
    pub level: Level,
    pub no_color: bool,
    pub ansi16: bool,
    pub gold: Color,
    /// Palette-owned rainbow samples; never interpolated in RGB here.
    pub rainbow: &'static [Color],
    pub seed: u64,
}
impl Chrome {
    pub fn logo_duration(self) -> Duration {
        Duration::from_millis(match (self.level, self.no_color || self.ansi16) {
            (Level::Off, _) | (_, true) => 0,
            (Level::Lite, _) => 150,
            _ => 1100,
        })
    }
    pub fn logo_done(self, elapsed: Duration) -> bool {
        elapsed >= self.logo_duration()
    }
    /// The caller supplies the already centred FIGlet (or narrow one-line) logo.
    pub fn logo(self, buf: &mut Buffer, rect: Rect, elapsed: Duration, protected: &[Rect]) {
        if self.logo_done(elapsed) {
            return;
        }
        let ms = elapsed.as_millis() as u64;
        let progress = (ms as f64 / 700.0).min(1.0);
        let edge = (1.0 - (1.0 - progress).powi(3)) * f64::from(rect.width);
        visit(buf, rect, protected, |x, y, cell| {
            if self.level == Level::Lite {
                cell.modifier.insert(Modifier::DIM);
                return;
            }
            if f64::from(x - rect.x) >= edge && cell.symbol() != " " {
                let hash = self
                    .seed
                    .wrapping_add(u64::from(x) * 7919)
                    .wrapping_add(u64::from(y) * 104729)
                    .wrapping_add((ms / 50) * 31);
                cell.set_symbol(["░", "▒", "▓", "#", "/", "\\"][(hash % 6) as usize]);
                cell.set_fg(self.gold).modifier.insert(Modifier::DIM);
            } else if ms >= 700 && !self.rainbow.is_empty() {
                let offset =
                    usize::from(x - rect.x) + ((ms - 700) * u64::from(rect.width) / 400) as usize;
                cell.set_fg(self.rainbow[offset % self.rainbow.len()]);
            }
        });
    }
    pub fn roster_duration(nick: &str) -> Duration {
        Duration::from_millis(nick.chars().count() as u64 * 20 + 120)
    }
    /// Row text is presentation-only. Failed resolution is explicit, including without colour.
    pub fn roster(
        self,
        buf: &mut Buffer,
        rect: Rect,
        nick: &str,
        failed: bool,
        hue: Color,
        elapsed: Duration,
    ) {
        if rect.is_empty() {
            return;
        }
        let done = self.level == Level::Off || elapsed >= Self::roster_duration(nick);
        let shown = if done {
            nick.chars().count()
        } else {
            (elapsed.as_millis() / 20) as usize
        };
        let text: String = nick
            .chars()
            .enumerate()
            .map(|(i, c)| if i < shown { c } else { '·' })
            .collect();
        let text = if failed { format!("× {text}") } else { text };
        let mut style = ratatui::style::Style::default();
        if !self.no_color {
            style = style.fg(if failed { Color::Red } else { hue });
        }
        if !done && self.no_color {
            style = style.add_modifier(Modifier::BOLD);
        } else if !done && shown < nick.chars().count() {
            style = style.add_modifier(Modifier::DIM);
        }
        if !done && shown >= nick.chars().count() {
            style = style.add_modifier(if self.no_color {
                Modifier::BOLD
            } else {
                Modifier::REVERSED
            });
        }
        let clipped = rect.intersection(buf.area);
        if !clipped.is_empty() {
            buf.set_stringn(clipped.x, clipped.y, text, clipped.width as usize, style);
        }
    }
    pub fn breath_done(self, elapsed: Duration) -> bool {
        self.level != Level::Full
            || self.no_color
            || self.ansi16
            || elapsed >= Duration::from_secs(20)
    }
    /// Three dim cells advance at 4 Hz, one lap in eight seconds. Geometry stays fixed.
    pub fn breath(self, buf: &mut Buffer, rect: Rect, elapsed: Duration, protected: &[Rect]) {
        if self.breath_done(elapsed) || rect.width < 2 || rect.height < 2 {
            return;
        }
        let perimeter = u64::from(rect.width) * 2 + u64::from(rect.height - 2) * 2;
        let head = (elapsed.as_millis() as u64 / 250 % 32) * perimeter / 32;
        visit(buf, rect, protected, |x, y, cell| {
            let offset = if y == rect.y {
                Some(u64::from(x - rect.x))
            } else if x == rect.right() - 1 {
                Some(u64::from(rect.width - 1) + u64::from(y - rect.y))
            } else if y == rect.bottom() - 1 {
                Some(
                    (u64::from(rect.width) + u64::from(rect.height) - 2)
                        + u64::from(rect.right() - 1 - x),
                )
            } else if x == rect.x {
                Some(perimeter - u64::from(y - rect.y))
            } else {
                None
            };
            if offset.is_some_and(|p| (p + perimeter - head) % perimeter < 3) {
                cell.set_fg(self.gold).modifier.insert(Modifier::DIM);
            }
        });
    }
    pub fn quit_duration(self) -> Duration {
        Duration::from_millis(
            if self.level == Level::Full && !self.no_color && !self.ansi16 {
                250
            } else {
                0
            },
        )
    }
    pub fn quit_done(self, elapsed: Duration) -> bool {
        elapsed >= self.quit_duration()
    }
    /// Full-buffer collapse is allowed only by the caller's capability gate. Protected text is spared.
    pub fn power_down(self, buf: &mut Buffer, rect: Rect, elapsed: Duration, protected: &[Rect]) {
        let ms = if self.quit_done(elapsed) {
            250
        } else {
            elapsed.as_millis() as u64
        };
        let half_height =
            f64::from(rect.height) / 2.0 * (1.0 - (ms.min(180) as f64 / 180.0).powi(2));
        let half_width = f64::from(rect.width) / 2.0
            * (1.0 - (ms.saturating_sub(180).min(70) as f64 / 70.0).powi(2));
        let cy = rect.y + rect.height / 2;
        let cx = rect.x + rect.width / 2;
        visit(buf, rect, protected, |x, y, cell| {
            if ms >= 250
                || f64::from(y.abs_diff(cy)) > half_height
                || f64::from(x.abs_diff(cx)) > half_width
            {
                cell.reset();
            }
        });
    }
    /// Resize discards all previous effect instances at the manager boundary. This is the new border flash only.
    pub fn resize(self, buf: &mut Buffer, rect: Rect, elapsed: Duration, protected: &[Rect]) {
        if self.level == Level::Off || self.no_color || elapsed >= Duration::from_millis(120) {
            return;
        }
        visit(buf, rect, protected, |x, y, cell| {
            if x == rect.x || x == rect.right() - 1 || y == rect.y || y == rect.bottom() - 1 {
                cell.set_fg(self.gold).modifier.insert(Modifier::BOLD);
            }
        });
    }
}
fn visit(
    buf: &mut Buffer,
    rect: Rect,
    protected: &[Rect],
    mut apply: impl FnMut(u16, u16, &mut ratatui::buffer::Cell),
) {
    let clipped = rect.intersection(buf.area);
    for y in clipped.y..clipped.bottom() {
        for x in clipped.x..clipped.right() {
            if !protected.iter().any(|r| r.contains((x, y).into())) {
                apply(x, y, &mut buf[(x, y)]);
            }
        }
    }
}
