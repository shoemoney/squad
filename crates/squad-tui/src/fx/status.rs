//! Status samples use the caller's clock and real token counters, never a cosmetic rate.
pub use crate::fx::FxLevel as Level;
use ratatui::style::{Color, Modifier, Style};
use std::{collections::VecDeque, time::Duration};
#[derive(Clone, Copy, Debug)]
pub struct StatusStyle {
    pub level: Level,
    pub no_color: bool,
    pub ansi16: bool,
    pub hue: Color,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indicator {
    pub text: String,
    pub style: Style,
    pub next_wake: Option<Duration>,
}
impl StatusStyle {
    fn indicator(self, text: String, moving: bool) -> Indicator {
        Indicator {
            text,
            style: if self.no_color {
                Style::default()
            } else {
                Style::default().fg(self.hue)
            },
            next_wake: moving.then_some(Duration::from_millis(100)),
        }
    }
    pub fn thinking(self, elapsed: Duration) -> Indicator {
        let mut out = self.indicator(
            if self.level == Level::Off {
                "T".into()
            } else if self.no_color {
                "...".into()
            } else {
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                    [(elapsed.as_millis() / 100 % 10) as usize]
                    .into()
            },
            self.level != Level::Off && !self.no_color,
        );
        if !self.no_color {
            out.style = out.style.add_modifier(Modifier::DIM);
        }
        out
    }
    pub fn streaming(self, rate: Option<f64>) -> Indicator {
        if self.level == Level::Off {
            return self.indicator("S".into(), false);
        }
        if self.ansi16 && !self.no_color {
            let mut out = self.indicator(">>".into(), false);
            out.style = out.style.add_modifier(Modifier::BOLD);
            return out;
        }
        let text = match rate.filter(|v| v.is_finite() && *v >= 0.0) {
            None => "?".into(),
            Some(v) if self.no_color => format!("{v:.0} tok/s"),
            Some(v) => ["▁", "▂", "▃", "▅", "▇"][if v < 5.0 {
                0
            } else if v < 15.0 {
                1
            } else if v < 30.0 {
                2
            } else if v < 60.0 {
                3
            } else {
                4
            }]
            .into(),
        };
        self.indicator(text, false)
    }
    pub fn backoff(self, elapsed: Duration, duration: Duration) -> Indicator {
        let remaining = duration.saturating_sub(elapsed);
        if self.no_color {
            return self.indicator(
                format!(
                    "retry {}s",
                    remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)
                ),
                self.level != Level::Off && !remaining.is_zero(),
            );
        }
        let index = if self.level == Level::Off || duration.is_zero() {
            3
        } else {
            ((elapsed.as_secs_f64() / duration.as_secs_f64() * 4.0) as usize).min(3)
        };
        self.indicator(
            ["◔", "◑", "◕", "●"][index].into(),
            self.level != Level::Off && elapsed < duration,
        )
    }
    pub fn pips(self, previous: usize, depth: usize, elapsed: Duration) -> Indicator {
        if self.no_color {
            return self.indicator(format!("q{depth}"), false);
        }
        let mut text = "▪".repeat(depth.min(5));
        if depth > 5 {
            text.push_str(&format!("+{}", depth - 5));
        }
        let active =
            self.level == Level::Full && previous != depth && elapsed < Duration::from_millis(120);
        if active && depth > previous && depth <= 5 {
            text.pop();
            text.push_str(if elapsed < Duration::from_millis(60) {
                " ▪"
            } else {
                "▪"
            });
        }
        if active && depth < previous {
            text.push('·');
        }
        self.indicator(text, active)
    }
}
/// Timestamped token increments. At most 501 buckets, coalesced to milliseconds.
#[derive(Default, Debug)]
pub struct TokenRate {
    samples: VecDeque<(Duration, u64)>,
    last: Option<Duration>,
}
impl TokenRate {
    pub fn record(&mut self, now: Duration, tokens: u64) {
        if self.last.is_some_and(|last| now < last) {
            self.samples.clear();
        }
        self.last = Some(now);
        self.prune(now);
        let bucket = Duration::from_millis(now.as_millis().min(u128::from(u64::MAX)) as u64);
        if let Some((_, count)) = self.samples.back_mut().filter(|(at, _)| *at == bucket) {
            *count = count.saturating_add(tokens);
        } else {
            self.samples.push_back((bucket, tokens));
        }
    }
    fn prune(&mut self, now: Duration) {
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.saturating_sub(*at) >= Duration::from_millis(500))
        {
            self.samples.pop_front();
        }
    }
    pub fn per_second(&mut self, now: Duration) -> Option<f64> {
        if self.last.is_none() || self.last.is_some_and(|last| now < last) {
            return None;
        }
        self.prune(now);
        Some(self.samples.iter().map(|(_, n)| *n as f64).sum::<f64>() * 2.0)
    }
}
