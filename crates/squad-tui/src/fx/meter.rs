//! Daily accounting presentation. Values are authoritative snapshots, never accumulated here.
pub use crate::fx::FxLevel as Level;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use std::time::Duration;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heat {
    Normal,
    Warm,
    Hot,
    Max,
}
/// Money is integer millionths of a dollar, avoiding rounding at heat thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spend {
    pub day: i32,
    pub actual: u64,
    pub pending: u64,
    pub max: u64,
    pub unknown_calls: u64,
}
impl Spend {
    pub fn heat(self) -> Heat {
        let actual = u128::from(self.actual) * 100;
        let max = u128::from(self.max);
        if actual >= max * 100 {
            Heat::Max
        } else if actual >= max * 90 {
            Heat::Hot
        } else if actual >= max * 75 {
            Heat::Warm
        } else {
            Heat::Normal
        }
    }
    pub fn labels(self) -> String {
        format!(
            "{}{} / {} · pending {}{}{}",
            if self.unknown_calls > 0 { "~" } else { "" },
            money(self.actual),
            money(self.max),
            money(self.pending),
            if self.unknown_calls > 0 {
                " · cost unknown: "
            } else {
                ""
            },
            if self.unknown_calls > 0 {
                self.unknown_calls.to_string()
            } else {
                String::new()
            }
        )
    }
}
fn money(value: u64) -> String {
    format!("${}.{:02}", value / 1_000_000, (value % 1_000_000) / 10_000)
}
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub gold: Color,
    pub orange: Color,
    pub red: Color,
    pub pending: Color,
}
#[derive(Clone, Copy, Debug)]
pub struct Meter {
    pub level: Level,
    pub no_color: bool,
    pub palette: Palette,
}
impl Meter {
    pub fn odometer_done(self, elapsed: Duration) -> bool {
        self.level != Level::Full || self.no_color || elapsed >= Duration::from_millis(150)
    }
    /// Rolls changed digits through three intermediate digits, preserving all punctuation.
    /// Use only in a dedicated graphic; `render` always keeps exact accounting labels visible.
    pub fn odometer(self, previous: &str, current: &str, elapsed: Duration) -> String {
        if self.odometer_done(elapsed) {
            return current.into();
        }
        let step = (elapsed.as_millis() / 50) as u32 + 1;
        let old: Vec<char> = previous.chars().collect();
        current
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if c.is_ascii_digit() && old.get(i) != Some(&c) {
                    char::from_digit((c.to_digit(10).unwrap_or(0) + step * 3) % 10, 10).unwrap_or(c)
                } else {
                    c
                }
            })
            .collect()
    }
    pub fn next_wake(self, spend: Spend) -> Option<Duration> {
        (self.level == Level::Full && !self.no_color && spend.heat() == Heat::Hot)
            .then_some(Duration::from_secs(1))
    }
    /// Retarget from the currently sampled graphic when an accounting update replaces it.
    /// Labels always use `current`; rollover and max changes settle immediately.
    pub fn transition(self, previous: Spend, current: Spend, elapsed: Duration) -> Spend {
        if self.odometer_done(elapsed) || previous.day != current.day || previous.max != current.max
        {
            return current;
        }
        let t = elapsed.as_millis().min(150) as i128;
        let tween =
            |a: u64, b: u64| (i128::from(a) + (i128::from(b) - i128::from(a)) * t / 150) as u64;
        Spend {
            actual: tween(previous.actual, current.actual),
            pending: tween(previous.pending, current.pending),
            ..current
        }
    }
    /// Draw an animated graphic beside exact authoritative labels. Thresholds always use actual state.
    pub fn render_transition(
        self,
        buf: &mut Buffer,
        rect: Rect,
        previous: Spend,
        current: Spend,
        elapsed: Duration,
    ) {
        self.render_graphic(
            buf,
            rect,
            current,
            self.transition(previous, current, elapsed),
            elapsed,
        );
    }
    /// `rect` is a dedicated footer, outside message-body rects. Two rows retain labels at narrow widths.
    pub fn render(self, buf: &mut Buffer, rect: Rect, spend: Spend, elapsed: Duration) {
        self.render_graphic(buf, rect, spend, spend, elapsed);
    }
    fn render_graphic(
        self,
        buf: &mut Buffer,
        rect: Rect,
        spend: Spend,
        graphic: Spend,
        elapsed: Duration,
    ) {
        let rect = rect.intersection(buf.area);
        if rect.is_empty() {
            return;
        }
        let base = Style::default();
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                buf[(x, y)].reset();
            }
        }
        let compact = rect.width < 40;
        let label = if compact {
            format!(
                "{}{} / {}{}",
                if spend.unknown_calls > 0 { "~" } else { "" },
                money(spend.actual),
                money(spend.max),
                if spend.unknown_calls > 0 {
                    " unknown"
                } else {
                    ""
                }
            )
        } else {
            spend.labels()
        };
        buf.set_stringn(rect.x, rect.y, &label, usize::from(rect.width), base);
        let bar_y = if rect.height > 1 { rect.y + 1 } else { rect.y };
        let prefix = if rect.height == 1 {
            format!("{label} ")
        } else if compact {
            format!("pending {}", money(spend.pending))
        } else {
            String::new()
        };
        let prefix_width = unicode_width::UnicodeWidthStr::width(prefix.as_str())
            .min(usize::from(rect.width)) as u16;
        if !prefix.is_empty() {
            buf.set_stringn(rect.x, bar_y, &prefix, usize::from(rect.width), base);
        }
        let bar_x = rect.x + prefix_width;
        let available = rect.width - prefix_width;
        let heat = spend.heat();
        let warning = match heat {
            Heat::Normal => "",
            Heat::Warm => " !",
            Heat::Hot => " !!",
            Heat::Max => " DAILY MAX",
        };
        let warning_width = warning.len() as u16;
        let width = available.saturating_sub(warning_width).min(30);
        let denominator = u128::from(spend.max.max(1));
        let filled = ((u128::from(graphic.actual) * u128::from(width) / denominator)
            .min(u128::from(width))) as u16;
        let reserved = ((u128::from(graphic.actual.saturating_add(graphic.pending))
            * u128::from(width)
            / denominator)
            .min(u128::from(width))) as u16;
        let color = match heat {
            Heat::Normal => self.palette.gold,
            Heat::Warm => self.palette.orange,
            Heat::Hot | Heat::Max => self.palette.red,
        };
        let mut style = if self.no_color { base } else { base.fg(color) };
        if self.level == Level::Full
            && !self.no_color
            && heat == Heat::Hot
            && elapsed.as_secs() % 2 == 1
        {
            style = style.add_modifier(Modifier::DIM);
        }
        for i in 0..width {
            let pending = i >= filled && i < reserved;
            let symbol = if i < filled {
                "▰"
            } else if pending {
                "▒"
            } else {
                "▱"
            };
            let cell_style = if pending && !self.no_color {
                base.fg(self.palette.pending)
            } else {
                style
            };
            buf[(bar_x + i, bar_y)]
                .set_symbol(symbol)
                .set_style(cell_style);
        }
        let warning_style = if heat == Heat::Max {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        };
        buf.set_stringn(
            bar_x + width,
            bar_y,
            warning,
            usize::from(available - width),
            warning_style,
        );
    }
    /// One red participant sweep upon crossing the max; caller supplies transition age.
    /// Rollover and raised maxima remove the obsolete warning immediately.
    pub fn max_sweep(
        self,
        buf: &mut Buffer,
        rect: Rect,
        previous: Spend,
        current: Spend,
        elapsed: Duration,
        protected: &[Rect],
    ) {
        if self.level != Level::Full
            || self.no_color
            || previous.heat() == Heat::Max
            || current.heat() != Heat::Max
            || elapsed >= Duration::from_millis(280)
        {
            return;
        }
        let clipped = rect.intersection(buf.area);
        let column = rect.x + (u128::from(rect.width) * elapsed.as_millis() / 280) as u16;
        for y in clipped.y..clipped.bottom() {
            for x in clipped.x..clipped.right() {
                if x.abs_diff(column) <= 1 && !protected.iter().any(|r| r.contains((x, y).into())) {
                    buf[(x, y)].set_fg(self.palette.red);
                }
            }
        }
    }
    /// Counts are supplied by the scoreboard reducer; this function never increments them.
    pub fn record_tick(
        self,
        previous: u64,
        current: u64,
        winner: bool,
        elapsed: Duration,
    ) -> (String, Style) {
        let done = previous == current
            || self.level == Level::Off
            || self.no_color
            || elapsed >= Duration::from_millis(150);
        let text = if done {
            current.to_string()
        } else {
            // Record ticks retain count-ups in lite, unlike cost digits.
            let full = Self {
                level: Level::Full,
                ..self
            };
            full.odometer(&previous.to_string(), &current.to_string(), elapsed)
        };
        let style = if !done && winner {
            Style::default()
                .fg(self.palette.gold)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        (text, style)
    }
}
