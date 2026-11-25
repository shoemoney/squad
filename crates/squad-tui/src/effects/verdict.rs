//! Deterministic, inline verdict reveal. Render after the static transcript on a fresh
//! buffer each frame. `card` is a TUI-owned decoration rect, never a message body.
//! Banner art is supplied by the caller (the shared core banner cache at integration).
use std::time::Duration;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

mod confetti;
use confetti::{Confetti, LocalConfetti};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Full,
    Lite,
    Off,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Accent,
    Blackout,
    Criteria,
    Suspense,
    FinishTotals,
    Winner,
    Respect,
    Settle,
    Done,
}

#[derive(Clone, Debug)]
pub struct Contender {
    pub nick: String,
    pub hue: Color,
    /// Already rendered slant art; compact art is supplied for narrow terminals.
    pub art: Vec<String>,
    pub compact_art: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Criterion {
    pub name: String,
    pub scores: [u32; 2],
}

#[derive(Clone, Debug)]
pub struct Verdict {
    pub contenders: [Contender; 2],
    pub criteria: [Criterion; 4],
    pub scale: u32,
    pub totals: [u32; 2],
    /// Required authoritative winner index, including equal totals or round tallies.
    pub winner: usize,
    pub round_tally: [u32; 2],
    pub override_reason: Option<String>,
    pub reasoning: String,
    pub steal: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tick {
    DebateRecord,
    Trophy,
}

/// Shared winner/champion treatment, deliberately independent of room counters.
pub struct Celebration {
    level: Level,
    no_color: bool,
    skipped: bool,
    tick_claimed: bool,
    tick: Tick,
    particles: LocalConfetti,
}

impl Celebration {
    pub fn new(level: Level, no_color: bool, live: bool, tick: Tick, seed: u64) -> Self {
        Self {
            level,
            no_color,
            skipped: !live,
            tick_claimed: !live,
            tick,
            particles: LocalConfetti::new(seed),
        }
    }

    /// The original key is returned unchanged for the normal TUI input handler.
    pub fn skip_on_key<K>(&mut self, key: K) -> K {
        self.skipped = true;
        key
    }
    pub fn settle(&mut self) {
        self.skipped = true;
    }
    pub fn duration(&self) -> Duration {
        Duration::from_millis(match self.level {
            Level::Full => 6500,
            Level::Lite => 900,
            Level::Off => 0,
        })
    }
    pub fn is_done(&self, t: Duration) -> bool {
        self.skipped || t >= self.duration()
    }
    pub fn phase(&self, t: Duration) -> Phase {
        if self.is_done(t) {
            return Phase::Done;
        }
        if self.level == Level::Lite {
            return Phase::Criteria;
        }
        match t.as_millis() {
            0..600 => Phase::Accent,
            600..900 => Phase::Blackout,
            900..2300 => Phase::Criteria,
            2300..2800 => Phase::Suspense,
            2800..3100 => Phase::FinishTotals,
            3100..4200 => Phase::Winner,
            4200..5600 => Phase::Respect,
            _ => Phase::Settle,
        }
    }
    /// Presentation cue only: callers continue to display authoritative counters.
    pub fn take_tick(&mut self, t: Duration) -> Option<Tick> {
        let due = self.is_done(t) || t >= Duration::from_millis(5600);
        if due && !self.tick_claimed {
            self.tick_claimed = true;
            Some(self.tick)
        } else {
            None
        }
    }
    fn winner_ready(&self, t: Duration) -> bool {
        self.is_done(t)
            || if self.level == Level::Lite {
                t.as_millis() >= 400
            } else {
                t.as_millis() >= 3100
            }
    }
    fn style(&self, hue: Color) -> Style {
        if self.no_color {
            Style::default()
        } else {
            Style::default().fg(hue)
        }
    }
    /// Reused by tournaments. Only blank, unprotected cells receive particles.
    #[allow(clippy::too_many_arguments)]
    pub fn render_winner(
        &self,
        buf: &mut Buffer,
        card: Rect,
        protected: &[Rect],
        t: Duration,
        winner: &Contender,
        label: &str,
        row: u16,
        hues: [Color; 2],
    ) {
        if !self.winner_ready(t) {
            return;
        }
        let narrow = card.width < 100 || card.height < 30;
        let art = if narrow {
            &winner.compact_art
        } else {
            &winner.art
        };
        let mut style = self.style(winner.hue).add_modifier(Modifier::BOLD);
        if self.no_color || !matches!(winner.hue, Color::Rgb(..)) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        put(
            buf,
            card,
            protected,
            3,
            row,
            &format!("♛ {label} · {}", winner.nick),
            style,
        );
        for (i, line) in art.iter().enumerate() {
            put(
                buf,
                card,
                protected,
                3,
                row.saturating_add(1 + i as u16),
                line,
                style,
            );
            // One diagonal spectrum pass across the supplied slant glyphs.
            // This is a final celebration, where the palette permits rainbow.
            if self.level == Level::Full
                && !self.no_color
                && matches!(winner.hue, Color::Rgb(..))
                && !self.is_done(t)
                && (3100..4000).contains(&t.as_millis())
            {
                let front = progress((t.as_millis() - 3100) as u64, 900) * card.width as f32;
                let y = card.y.saturating_add(row).saturating_add(1 + i as u16);
                for x in card.x.saturating_add(3)..card.right() {
                    if protected.iter().any(|r| r.contains((x, y).into())) {
                        continue;
                    }
                    if (x.saturating_sub(card.x) as f32 - front).abs() < 8.0
                        && let Some(cell) = buf.cell_mut((x, y))
                        && cell.symbol() != " "
                    {
                        cell.set_fg(spectrum(
                            (x as f32 * 9.0 + y as f32 * 14.0 + t.as_secs_f32() * 120.0) % 360.0,
                        ));
                    }
                }
            }
        }
        if self.level == Level::Full
            && !self.no_color
            && !self.is_done(t)
            && t.as_millis() >= 3100
            && t.as_millis() < 5700
        {
            self.particles
                .draw(buf, card, protected, t - Duration::from_millis(3100), hues);
        }
    }
}

pub struct VerdictReveal {
    pub celebration: Celebration,
}
impl VerdictReveal {
    pub fn new(level: Level, no_color: bool, live: bool, seed: u64) -> Self {
        Self {
            celebration: Celebration::new(level, no_color, live, Tick::DebateRecord, seed),
        }
    }
    pub fn skip_on_key<K>(&mut self, key: K) -> K {
        self.celebration.skip_on_key(key)
    }
    pub fn is_done(&self, t: Duration) -> bool {
        self.celebration.is_done(t)
    }
    pub fn duration(&self) -> Duration {
        self.celebration.duration()
    }
    pub fn phase(&self, t: Duration) -> Phase {
        self.celebration.phase(t)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        buf: &mut Buffer,
        stage: Rect,
        card: Rect,
        protected: &[Rect],
        t: Duration,
        data: &Verdict,
    ) {
        let fx = &self.celebration;
        let done = fx.is_done(t);
        let ms = t.as_millis() as u64;
        let full = fx.level == Level::Full && !done;
        if full && ms >= 600 && !fx.no_color {
            let light = if ms < 5600 {
                0.35
            } else {
                0.35 + 0.65 * progress(ms - 5600, 400)
            };
            for y in stage.y..stage.bottom() {
                for x in stage.x..stage.right() {
                    if card.contains((x, y).into()) {
                        continue;
                    }
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        if let Color::Rgb(r, g, b) = cell.fg {
                            cell.set_fg(Color::Rgb(
                                (r as f32 * light) as u8,
                                (g as f32 * light) as u8,
                                (b as f32 * light) as u8,
                            ));
                        } else if light < 0.7 {
                            cell.set_style(Style::default().add_modifier(Modifier::DIM));
                        }
                    }
                }
            }
        }
        let winner = data.winner.min(1);
        let loser = 1 - winner;
        let gold = gold_for(data.contenders[winner].hue);
        let border = fx.style(if fx.winner_ready(t) {
            data.contenders[winner].hue
        } else {
            gold
        });
        let frame_progress = if full {
            progress(ms.saturating_sub(600), 280)
        } else {
            1.0
        };
        if full && ms < 600 && card.height > 0 {
            let centre = (progress(ms, 600) * card.width as f32) as u16;
            for x in centre.saturating_sub(5)..centre.saturating_add(5).min(card.width) {
                put(buf, card, protected, x, 0, "━", fx.style(gold));
            }
        }
        frame(buf, card, protected, frame_progress, border);
        put(
            buf,
            card,
            protected,
            3,
            0,
            " VERDICT ",
            border.add_modifier(Modifier::BOLD),
        );
        if full && ms < 900 {
            return;
        }
        let name_style = |index: usize| {
            let style = fx
                .style(data.contenders[index].hue)
                .add_modifier(Modifier::BOLD);
            if full
                && ((2300..2800).contains(&ms) && ((ms - 2300) / 125) as usize % 2 == index
                    || (2980..3100).contains(&ms) && index == winner)
            {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            }
        };
        put(
            buf,
            card,
            protected,
            3,
            2,
            &data.contenders[0].nick,
            name_style(0),
        );
        // Both nameplates retain their identity and have separate suspense pulses.
        put(
            buf,
            card,
            protected,
            card.width / 2,
            2,
            &data.contenders[1].nick,
            name_style(1),
        );
        let narrow = card.width < 100 || card.height < 30;
        for (i, criterion) in data.criteria.iter().enumerate() {
            let delay = if full { 900 + i as u64 * 350 } else { 0 };
            if !done && ms < delay {
                continue;
            }
            let p = if done {
                1.0
            } else {
                ease(progress(ms.saturating_sub(delay), 400))
            };
            let scores = criterion.scores.map(|v| (v as f32 * p).round() as u32);
            let y = 4 + i as u16;
            put(
                buf,
                card,
                protected,
                3,
                y,
                &format!(
                    "{}  {} : {} / {}",
                    criterion.name, scores[0], scores[1], data.scale
                ),
                fx.style(gold),
            );
            if !narrow {
                let bars = criterion.scores.map(|v| {
                    ((v as f32 / data.scale.max(1) as f32 * 12.0 * p).round() as usize).min(12)
                });
                let glyph = if fx.no_color || !matches!(data.contenders[0].hue, Color::Rgb(..)) {
                    "#"
                } else {
                    "▰"
                };
                put(
                    buf,
                    card,
                    protected,
                    34,
                    y,
                    &format!("{}{}", " ".repeat(12 - bars[0]), glyph.repeat(bars[0])),
                    fx.style(data.contenders[0].hue),
                );
                put(buf, card, protected, 46, y, "╋", fx.style(gold));
                put(
                    buf,
                    card,
                    protected,
                    47,
                    y,
                    &glyph.repeat(bars[1]),
                    fx.style(data.contenders[1].hue),
                );
            }
        }
        if done || !full || ms >= 2300 {
            let totals = if full && ms < 3100 {
                let low = data.totals[0].min(data.totals[1]) as f32 * 0.8;
                data.totals.map(|v| {
                    (low + (v as f32 - low) * ease(progress(ms.saturating_sub(2800), 300))).round()
                        as u32
                })
            } else if !done && !full {
                data.totals
                    .map(|v| (v as f32 * ease(progress(ms, 400))).round() as u32)
            } else {
                data.totals
            };
            put(
                buf,
                card,
                protected,
                3,
                9,
                &format!("TOTAL  {} : {}", totals[0], totals[1]),
                fx.style(gold).add_modifier(Modifier::BOLD),
            );
            put(
                buf,
                card,
                protected,
                3,
                10,
                &format!(
                    "Round tally  {} : {}",
                    data.round_tally[0], data.round_tally[1]
                ),
                fx.style(gold),
            );
        }
        let art = if narrow {
            &data.contenders[winner].compact_art
        } else {
            &data.contenders[winner].art
        };
        // Shared celebration deliberately leaves the existing VERDICT banner alone.
        fx.render_winner(
            buf,
            card,
            protected,
            t,
            &data.contenders[winner],
            "WINNER",
            12,
            data.contenders.each_ref().map(|c| c.hue),
        );
        let body_row = 14 + art.len() as u16;
        if fx.winner_ready(t) {
            put(
                buf,
                card,
                protected,
                3,
                body_row,
                &data.reasoning,
                Style::default(),
            );
            if let Some(reason) = &data.override_reason {
                put(
                    buf,
                    card,
                    protected,
                    3,
                    body_row + 1,
                    &format!("Tally override: {reason}"),
                    Style::default(),
                );
            }
        }
        if done || !full || ms >= 4200 {
            let heading = format!(
                "✦ respect · what to steal from {}",
                data.contenders[loser].nick
            );
            let count = if full {
                ((ms - 4200) / 12) as usize
            } else {
                usize::MAX
            };
            let heading: String = heading.graphemes(true).take(count).collect();
            let style = fx.style(data.contenders[loser].hue);
            put(
                buf,
                card,
                protected,
                3,
                body_row + 3,
                &heading,
                style.add_modifier(Modifier::BOLD),
            );
            put(buf, card, protected, 3, body_row + 4, &data.steal, style);
            if let Some(best) = data.criteria.iter().max_by_key(|c| c.scores[loser]) {
                let style = if full && (4200..4600).contains(&ms) {
                    style.add_modifier(Modifier::UNDERLINED)
                } else {
                    style
                };
                put(
                    buf,
                    card,
                    protected,
                    3,
                    body_row + 5,
                    &format!("Best: {} {}", best.name, best.scores[loser]),
                    style,
                );
            }
        }
        if fx.level == Level::Lite && !done && ms < 150 {
            for y in card.y..card.bottom() {
                for x in card.x..card.right() {
                    if !protected.iter().any(|r| r.contains((x, y).into()))
                        && let Some(cell) = buf.cell_mut((x, y))
                    {
                        cell.set_style(Style::default().add_modifier(Modifier::DIM));
                    }
                }
            }
        }
    }
}

pub(crate) fn gold_for(hue: Color) -> Color {
    if matches!(hue, Color::Rgb(..)) {
        Color::Rgb(255, 210, 63)
    } else {
        Color::Yellow
    }
}

fn spectrum(degrees: f32) -> Color {
    // Storyboard gay filter: OKLCH L=.78, C=.16; clamp only at sRGB output.
    let angle = f64::from(degrees).to_radians();
    let a = 0.16 * angle.cos();
    let b = 0.16 * angle.sin();
    let l = (0.78 + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (0.78 - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (0.78 - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let channel = |v: f64| {
        let v = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Color::Rgb(
        channel(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
        channel(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
        channel(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s),
    )
}

pub(crate) fn progress(ms: u64, duration: u64) -> f32 {
    (ms as f32 / duration as f32).clamp(0.0, 1.0)
}
pub(crate) fn ease(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(2)
}

/// Clipped, grapheme-aware writing that also honours protected rectangles.
pub(crate) fn put(
    buf: &mut Buffer,
    area: Rect,
    protected: &[Rect],
    dx: u16,
    dy: u16,
    text: &str,
    style: Style,
) {
    let y = area.y.saturating_add(dy);
    if y >= area.bottom() {
        return;
    }
    let mut x = area.x.saturating_add(dx);
    for glyph in text.graphemes(true) {
        let width = glyph.width() as u16;
        if width == 0 {
            continue;
        }
        if x.saturating_add(width) > area.right() {
            break;
        }
        if (x..x + width).all(|cx| {
            buf.area.contains((cx, y).into())
                && !protected.iter().any(|p| p.contains((cx, y).into()))
        }) {
            buf[(x, y)].set_symbol(glyph).set_style(style);
            for cx in x + 1..x + width {
                buf[(cx, y)].reset();
            }
        }
        x += width;
    }
}

pub(crate) fn frame(buf: &mut Buffer, area: Rect, protected: &[Rect], p: f32, style: Style) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let half = (area.width as f32 * p / 2.0).ceil() as u16;
    let mid = area.width / 2;
    for x in mid.saturating_sub(half)..mid.saturating_add(half).min(area.width) {
        put(
            buf,
            area,
            protected,
            x,
            0,
            if x == 0 {
                "╔"
            } else if x == area.width - 1 {
                "╗"
            } else {
                "═"
            },
            style,
        );
        put(
            buf,
            area,
            protected,
            x,
            area.height - 1,
            if x == 0 {
                "╚"
            } else if x == area.width - 1 {
                "╝"
            } else {
                "═"
            },
            style,
        );
    }
    for y in 1..area.height - 1 {
        if p >= 1.0 {
            put(buf, area, protected, 0, y, "║", style);
            put(buf, area, protected, area.width - 1, y, "║", style);
        }
    }
}
