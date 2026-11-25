//! Permanent inline banner entrances. Time and committed identities come from the caller.
use std::{collections::HashMap, sync::Arc, time::Duration};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use squad_core::banner::{self, Banner, BannerMode, RenderedBanner};
use unicode_width::UnicodeWidthChar;

pub use crate::fx::FxLevel as Level;

pub use super::ColorTier as ColourTier;

/// Colours are supplied by the TUI palette, including its terminal-tier mapping.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub left: Color,
    pub right: Color,
    pub gold: Color,
    pub background: Color,
    pub tier: ColourTier,
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            left: super::palette::SIGNATURES[0],
            right: super::palette::SIGNATURES[3],
            gold: super::palette::GOLD,
            background: Color::Rgb(12, 15, 22),
            tier: ColourTier::TrueColor,
        }
    }
}

/// Cache misses call core; frames only borrow the resulting rows. `font` is the
/// core font-selection policy (On/Compact/Off), not a second FIGlet renderer.
#[derive(Default)]
pub struct BannerCache {
    entries: HashMap<(String, u8, u16), Arc<RenderedBanner>>,
}
impl BannerCache {
    pub fn get(&mut self, text: &str, font: BannerMode, width: u16) -> Arc<RenderedBanner> {
        let mode = match font {
            BannerMode::On => 0,
            BannerMode::Compact => 1,
            BannerMode::Off => 2,
        };
        self.entries
            .entry((text.to_owned(), mode, width))
            .or_insert_with(|| Arc::new(banner::render(text, usize::from(width), font)))
            .clone()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Dim,
    Horizon,
    Unfold,
    Glint,
    Hold,
    Settle,
    Static,
}

pub struct Entrance {
    pub art: Arc<RenderedBanner>,
    pub kind: Banner,
    pub level: Level,
    pub palette: Palette,
    settled: bool,
    banners_enabled: bool,
}
impl Entrance {
    pub fn new(
        cache: &mut BannerCache,
        kind: Banner,
        font: BannerMode,
        width: u16,
        level: Level,
        palette: Palette,
    ) -> Self {
        Self {
            art: cache.get(
                &kind.text(),
                if font == BannerMode::Off
                    || width < 60
                    || (width < 100
                        && matches!(
                            kind,
                            Banner::Census | Banner::Crosscheck | Banner::Survey | Banner::Probe
                        ))
                {
                    BannerMode::Off
                } else if font == BannerMode::On && width < 100 {
                    BannerMode::Compact
                } else {
                    font
                },
                width,
            ),
            kind,
            level,
            palette,
            settled: false,
            banners_enabled: font != BannerMode::Off,
        }
    }
    /// Use for first output, verdict arrival, stop, collapse, resize or replay.
    pub fn settle(&mut self) {
        self.settled = true;
    }
    /// Resize never replays an entrance. Rebuild the cached rows for the new width.
    pub fn resize(&mut self, cache: &mut BannerCache, width: u16, font: BannerMode) {
        let replacement = Self::new(cache, self.kind, font, width, self.level, self.palette);
        self.art = replacement.art;
        self.banners_enabled = replacement.banners_enabled;
        self.settle();
    }
    pub fn duration(&self) -> Duration {
        Duration::from_millis(
            if self.settled
                || !self.banners_enabled
                || self.level == Level::Off
                || self.palette.tier == ColourTier::NoColor
            {
                0
            } else if self.level == Level::Lite {
                850
            } else {
                1900
            },
        )
    }
    pub fn is_done(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration()
    }
    pub fn phase(&self, elapsed: Duration) -> Phase {
        if self.is_done(elapsed) {
            return Phase::Static;
        }
        match elapsed.as_millis() {
            0..120 => Phase::Dim,
            120..280 => Phase::Horizon,
            280..600 => Phase::Unfold,
            600..900 => Phase::Glint,
            900..1600 => Phase::Hold,
            _ => Phase::Settle,
        }
    }
    /// The caller owns the score subline and its glyphs. Only its fade is exposed.
    pub fn subline_visibility(&self, elapsed: Duration) -> f32 {
        if self.is_done(elapsed) {
            1.0
        } else {
            ((elapsed.as_secs_f32() - 0.9) / 0.15).clamp(0.0, 1.0)
        }
    }
    /// Draw only the registered inline art rectangle. Protected rectangles are
    /// excluded even if stale geometry accidentally overlaps a message body.
    pub fn render(&self, buf: &mut Buffer, area: Rect, elapsed: Duration, protected: &[Rect]) {
        let area = area.intersection(buf.area);
        if area.is_empty() {
            return;
        }
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if !protected.iter().any(|r| r.contains((x, y).into())) {
                    buf[(x, y)].reset();
                }
            }
        }
        let phase = self.phase(elapsed);
        let ms = elapsed.as_secs_f32() * 1000.0;
        let height = self.art.height().min(usize::from(area.height));
        if height == 0 {
            return;
        }
        if phase != Phase::Static && self.level == Level::Full {
            let dim = match phase {
                Phase::Dim => 1.0 - 0.4 * (ms / 120.0),
                Phase::Settle => 0.6 + 0.4 * ((ms - 1600.0) / 300.0),
                _ => 0.6,
            };
            let background = map_colour(shade(self.palette.background, dim), self.palette.tier);
            for y in area.y..area.y + height as u16 {
                for x in area.x..area.right() {
                    if !protected.iter().any(|r| r.contains((x, y).into())) {
                        buf[(x, y)].set_bg(background);
                    }
                }
            }
        }
        let horizon = (height - 1) as f32 * 0.45;
        let progress = ((ms - 280.0) / 320.0).clamp(0.0, 1.0);
        let back = 1.0 + 2.70158 * (progress - 1.0).powi(3) + 1.70158 * (progress - 1.0).powi(2);
        for (row, line) in self.art.lines.iter().take(height).enumerate() {
            let y = area.y + row as u16;
            let mut col = 0;
            for ch in line.chars() {
                let width = ch.width().unwrap_or(0) as u16;
                if width == 0 {
                    continue;
                }
                if col + width > area.width {
                    break;
                }
                let x = area.x + col;
                if !(0..width)
                    .any(|offset| protected.iter().any(|r| r.contains((x + offset, y).into())))
                {
                    let mut glyph = ch;
                    let mut colour =
                        self.colour(col, row as u16, area.width, height as u16, elapsed);
                    let mut style = Style::default().fg(colour).add_modifier(Modifier::BOLD);
                    if self.palette.tier == ColourTier::NoColor {
                        style = Style::reset().add_modifier(Modifier::BOLD);
                    }
                    if phase != Phase::Static && self.level == Level::Full {
                        match phase {
                            Phase::Dim | Phase::Horizon => {
                                glyph = ' ';
                                if phase == Phase::Horizon && row == horizon.round() as usize {
                                    let radius = ((ms - 120.0) / 160.0).clamp(0.0, 1.0)
                                        * f32::from(area.width)
                                        / 2.0;
                                    if (f32::from(col) - f32::from(area.width) / 2.0).abs()
                                        <= radius
                                    {
                                        glyph = '━';
                                        colour = Color::White;
                                    }
                                }
                            }
                            Phase::Unfold => {
                                let distance = (row as f32 - horizon).abs();
                                let limit = 0.5
                                    + horizon.max(height as f32 - 1.0 - horizon)
                                        * back.clamp(0.0, 1.0);
                                if distance > limit {
                                    glyph = ' ';
                                } else if ch != ' '
                                    && progress < 0.82
                                    && distance > (limit - 0.7).max(0.0)
                                {
                                    glyph = '▓';
                                }
                            }
                            _ => {}
                        }
                        let dim = match phase {
                            Phase::Dim => 1.0 - 0.4 * (ms / 120.0),
                            Phase::Settle => 0.6 + 0.4 * ((ms - 1600.0) / 300.0),
                            _ => 0.6,
                        };
                        style = style.fg(colour).bg(map_colour(
                            shade(self.palette.background, dim),
                            self.palette.tier,
                        ));
                    }
                    if self.level == Level::Lite && phase != Phase::Static && ms < 150.0 {
                        style = style.fg(map_colour(
                            shade(colour, (ms / 150.0).max(0.2)),
                            self.palette.tier,
                        ));
                    }
                    buf[(x, y)].set_char(glyph).set_style(style);
                    // FIGlet is ASCII; the label fallback may contain wide glyphs.
                    for offset in 1..width {
                        buf[(x + offset, y)].set_symbol(" ").set_style(style);
                    }
                }
                col += width;
            }
            // Horizon spans the entire owned band, including core's trimmed right margin.
            if phase == Phase::Horizon && row == horizon.round() as usize {
                let radius = ((ms - 120.0) / 160.0).clamp(0.0, 1.0) * f32::from(area.width) / 2.0;
                for x in area.x..area.right() {
                    if (f32::from(x - area.x) - f32::from(area.width) / 2.0).abs() <= radius
                        && !protected.iter().any(|r| r.contains((x, y).into()))
                    {
                        buf[(x, y)]
                            .set_char('━')
                            .set_style(Style::default().fg(Color::White));
                    }
                }
            }
        }
    }
    fn colour(&self, x: u16, y: u16, width: u16, height: u16, elapsed: Duration) -> Color {
        let tier = self.palette.tier;
        let mode = matches!(
            self.kind,
            Banner::Census
                | Banner::Crosscheck
                | Banner::Survey
                | Banner::Probe
                | Banner::ChooseDebate
                | Banner::ChooseGrill
        );
        let base = if mode {
            self.palette.gold
        } else if matches!(self.kind, Banner::FinalRound) {
            if x < width / 2 {
                Color::Rgb(235, 65, 54)
            } else {
                self.palette.gold
            }
        } else if x < width / 2 {
            self.palette.left
        } else {
            self.palette.right
        };
        if tier == ColourTier::Ansi16 {
            return map_colour(base, tier);
        }
        let done = self.is_done(elapsed) || self.level != Level::Full;
        let t = if done {
            1.6
        } else {
            elapsed.as_secs_f32().min(1.6)
        };
        let colour = if !mode && matches!(self.kind, Banner::Opening | Banner::Verdict) {
            rainbow((f32::from(x) * 9.0 + f32::from(y) * 14.0 + t * 120.0) % 360.0)
        } else {
            let ramp = 1.0 - 0.45 * f32::from(y) / f32::from(height.max(1));
            let drift = if done {
                0.0
            } else {
                ((f32::from(x) - t * 4.0) * 0.15).sin()
                    * 0.07
                    * ((1.9 - elapsed.as_secs_f32()) / 0.3).clamp(0.0, 1.0)
            };
            shade(base, ramp + drift)
        };
        let ms = elapsed.as_secs_f32() * 1000.0;
        let glint = (ms - 600.0) / 300.0 * f32::from(width);
        map_colour(
            if !done && (600.0..900.0).contains(&ms) && (f32::from(x) - glint).abs() < 1.5 {
                Color::White
            } else {
                colour
            },
            tier,
        )
    }
}

/// One record per committed boundary. A verdict payload settles the existing
/// JUDGING banner; it never creates a second boundary. Replay records are static.
#[derive(Default)]
pub struct Boundaries {
    entries: HashMap<String, Entrance>,
}
impl Boundaries {
    pub fn insert(
        &mut self,
        identity: impl Into<String>,
        mut entrance: Entrance,
        live: bool,
    ) -> bool {
        let identity = identity.into();
        if self.entries.contains_key(&identity) {
            return false;
        }
        if !live {
            entrance.settle();
        }
        self.entries.insert(identity, entrance);
        true
    }
    pub fn get(&self, identity: &str) -> Option<&Entrance> {
        self.entries.get(identity)
    }
    pub fn output_started(&mut self, identity: &str) {
        if let Some(entrance) = self.entries.get_mut(identity) {
            entrance.settle();
        }
    }
    #[allow(clippy::too_many_arguments)] // Mirrors the integration boundary and palette inputs.
    pub fn entering_judging(
        &mut self,
        contest: &str,
        cache: &mut BannerCache,
        width: u16,
        level: Level,
        palette: Palette,
        live: bool,
    ) -> bool {
        self.insert(
            format!("{contest}:verdict"),
            Entrance::new(
                cache,
                Banner::Verdict,
                BannerMode::On,
                width,
                level,
                palette,
            ),
            live,
        )
    }
    pub fn verdict_arrived(&mut self, contest: &str) {
        self.output_started(&format!("{contest}:verdict"));
    }
}

/// Style-only, one-shot step accent; step labels and divider geometry stay put.
pub fn accent_step(
    buf: &mut Buffer,
    area: Rect,
    elapsed: Duration,
    level: Level,
    palette: Palette,
) {
    if level == Level::Off
        || palette.tier == ColourTier::NoColor
        || elapsed >= Duration::from_millis(120)
    {
        return;
    }
    for y in area.intersection(buf.area).y..area.intersection(buf.area).bottom() {
        for x in area.intersection(buf.area).x..area.intersection(buf.area).right() {
            buf[(x, y)]
                .set_fg(map_colour(palette.gold, palette.tier))
                .set_style(Style::default().add_modifier(Modifier::BOLD));
        }
    }
}
fn shade(colour: Color, factor: f32) -> Color {
    super::palette::darken(colour, f64::from(factor))
}
fn map_colour(colour: Color, tier: ColourTier) -> Color {
    super::palette::map_color(colour, tier)
}
// OKLCH L=.78 C=.16, converted to linear sRGB and gamut clipped.
fn rainbow(hue: f32) -> Color {
    super::palette::Oklch {
        l: 0.78,
        c: 0.16,
        h: f64::from(hue),
    }
    .color()
}
