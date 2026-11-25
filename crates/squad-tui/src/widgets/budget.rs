//! Always-visible daily accounting. Never sum message usage in this widget.
use crate::fx::{
    ColorTier, FxLevel, Palette,
    meter::{Heat, Meter, Palette as MeterPalette, Spend},
};
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::BudgetStateEvent;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DailySpendView {
    pub day: String,
    pub actual: Option<u64>,
    pub pending: Option<u64>,
    pub max: Option<u64>,
    pub unknown_calls: u32,
    /// Exact router refusal, including its effective-max wording; never invented locally.
    pub refusal: Option<String>,
}
fn micros(n: f64) -> Option<u64> {
    (n.is_finite() && n >= 0. && n < u64::MAX as f64 / 1_000_000.)
        .then(|| (n * 1_000_000.).round() as u64)
}
impl From<&BudgetStateEvent> for DailySpendView {
    fn from(event: &BudgetStateEvent) -> Self {
        Self {
            day: event.day.clone(),
            actual: micros(event.spent_usd),
            pending: micros(event.reserved_usd),
            max: micros(event.limit_usd),
            unknown_calls: event.unknown_calls,
            refusal: None,
        }
    }
}
impl DailySpendView {
    pub fn remaining(&self) -> Option<u64> {
        Some(
            self.max?
                .saturating_sub(self.actual?)
                .saturating_sub(self.pending?),
        )
    }
    pub fn spend(&self) -> Option<Spend> {
        Some(Spend {
            day: self.day.replace('-', "").parse().ok()?,
            actual: self.actual?,
            pending: self.pending?,
            max: self.max?,
            unknown_calls: u64::from(self.unknown_calls),
        })
    }
    pub fn heat(&self) -> Option<Heat> {
        self.spend().map(Spend::heat)
    }
    pub fn labels(&self) -> String {
        format!(
            "{}{} / {} · pending {} · remaining {}{}",
            if self.unknown_calls > 0 { "~" } else { "" },
            money(self.actual),
            money(self.max),
            money(self.pending),
            money(self.remaining()),
            if self.unknown_calls > 0 {
                format!(" · {} unknown", self.unknown_calls)
            } else {
                String::new()
            }
        )
    }
}
fn money(n: Option<u64>) -> String {
    n.map_or("unavailable".into(), |n| {
        format!("${}.{:02}", n / 1_000_000, n % 1_000_000 / 10_000)
    })
}
#[derive(Clone, Copy, Debug, Default)]
pub struct BudgetRects {
    pub labels: Rect,
    pub meter: Rect,
    pub amounts: Rect,
    pub refusal: Rect,
    pub warning: Rect,
}
/// Exact labels stay protected. Only `meter` may be animated; caller owns placement
/// in the participants footer, narrow status line, or undersized-terminal display.
pub fn render(
    view: &DailySpendView,
    area: Rect,
    buf: &mut Buffer,
    palette: &Palette,
) -> BudgetRects {
    render_effect(view, area, buf, palette, FxLevel::Off, Duration::ZERO)
}
pub fn render_effect(
    view: &DailySpendView,
    area: Rect,
    buf: &mut Buffer,
    palette: &Palette,
    level: FxLevel,
    elapsed: Duration,
) -> BudgetRects {
    let area = area.intersection(buf.area);
    if area.is_empty() {
        return BudgetRects::default();
    }
    let mut rects = BudgetRects {
        labels: Rect::new(area.x, area.y, area.width, 1),
        ..BudgetRects::default()
    };
    // In a one-row status line keep a dedicated, disjoint graphic between
    // actual and max; the rest of the amounts remain exact text.
    if area.height == 1 {
        let prefix = format!(
            "{}{} ",
            if view.unknown_calls > 0 { "~" } else { "" },
            money(view.actual)
        );
        let left = (prefix.len() as u16).min(area.width);
        let bar_width = area.width.saturating_sub(left).saturating_sub(55).min(8);
        buf.set_stringn(area.x, area.y, prefix, usize::from(left), Style::default());
        rects.labels.width = left;
        rects.meter = Rect::new(area.x + left, area.y, bar_width, 1);
        for x in 0..bar_width {
            let symbol = if let Some(spend) = view.spend() {
                let scale = u128::from(bar_width);
                let denominator = u128::from(spend.max.max(1));
                let actual = u128::from(spend.actual) * scale / denominator;
                let reserved =
                    u128::from(spend.actual.saturating_add(spend.pending)) * scale / denominator;
                if u128::from(x) < actual {
                    "▰"
                } else if u128::from(x) < reserved {
                    "▒"
                } else {
                    "▱"
                }
            } else {
                "?"
            };
            buf.set_string(
                rects.meter.x + x,
                area.y,
                symbol,
                Style::default().fg(palette.chrome()),
            );
        }
        rects.amounts = Rect::new(
            rects.meter.right(),
            area.y,
            area.width - left - bar_width,
            1,
        );
        let suffix = format!(
            " {}{} · pending {} · remaining {}{}",
            money(view.max),
            if view.refusal.is_some() || view.heat() == Some(Heat::Max) {
                " · DAILY_MAX_REACHED"
            } else {
                ""
            },
            money(view.pending),
            money(view.remaining()),
            if view.unknown_calls > 0 {
                format!(" · {} unknown", view.unknown_calls)
            } else {
                String::new()
            }
        );
        buf.set_stringn(
            rects.amounts.x,
            area.y,
            suffix,
            usize::from(rects.amounts.width),
            Style::default(),
        );
        return rects;
    }
    let label = if area.width < 65 {
        format!(
            "{}{} / {} +{} pending",
            if view.unknown_calls > 0 { "~" } else { "" },
            money(view.actual),
            money(view.max),
            money(view.pending)
        )
    } else {
        view.labels()
    };
    buf.set_stringn(
        area.x,
        area.y,
        label,
        usize::from(area.width),
        Style::default(),
    );
    if area.height > 1
        && let Some(spend) = view.spend()
    {
        let prefix_width = if area.width < 40 {
            format!("pending {}", money(view.pending))
                .len()
                .min(usize::from(area.width)) as u16
        } else {
            0
        };
        let warning_width = match spend.heat() {
            Heat::Normal => 0,
            Heat::Warm => 2,
            Heat::Hot => 3,
            Heat::Max => 10,
        };
        let available = area.width - prefix_width;
        let bar_width = available.saturating_sub(warning_width).min(30);
        rects.amounts = Rect::new(area.x, area.y + 1, prefix_width, 1);
        rects.meter = Rect::new(area.x + prefix_width, area.y + 1, bar_width, 1);
        rects.warning = Rect::new(rects.meter.right(), area.y + 1, available - bar_width, 1);
        let meter = Meter {
            level,
            no_color: palette.tier == ColorTier::NoColor,
            palette: MeterPalette {
                gold: palette.chrome(),
                orange: crate::fx::palette::map_color(
                    ratatui::style::Color::Rgb(255, 150, 50),
                    palette.tier,
                ),
                red: crate::fx::palette::map_color(ratatui::style::Color::Red, palette.tier),
                pending: crate::fx::palette::map_color(ratatui::style::Color::Gray, palette.tier),
            },
        };
        // The existing effect renders labels on row one and graphics on row two.
        // A small scratch buffer lets us expose only its graphic as animatable.
        let mut scratch = Buffer::empty(Rect::new(0, 0, area.width, 2));
        meter.render(&mut scratch, Rect::new(0, 0, area.width, 2), spend, elapsed);
        for x in 0..area.width {
            buf[(area.x + x, area.y + 1)] = scratch[(x, 1)].clone();
        }
    }
    if area.height > 2 {
        let refusal = view.refusal.clone().or_else(|| {
            (view.heat() == Some(Heat::Max)).then(|| {
                format!(
                    "Daily max of {} reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow.",
                    money(view.max)
                )
            })
        });
        let text = refusal.as_ref().map_or_else(
            || {
                format!(
                    "remaining {} · in-flight calls continue",
                    money(view.remaining())
                )
            },
            |s| {
                format!(
                    "DAILY_MAX_REACHED · {} · In-flight calls continue.",
                    crate::markdown::sanitize(s)
                )
            },
        );
        let rows = crate::text::wrap(&text, usize::from(area.width));
        let height = rows.len().min(usize::from(area.height - 2)) as u16;
        rects.refusal = Rect::new(area.x, area.y + 2, area.width, height);
        for (i, row) in rows.iter().take(usize::from(height)).enumerate() {
            let text: String = row.cells.iter().map(|cell| cell.display.as_str()).collect();
            buf.set_stringn(
                area.x,
                area.y + 2 + i as u16,
                text,
                usize::from(area.width),
                Style::default(),
            );
        }
    }
    rects
}
