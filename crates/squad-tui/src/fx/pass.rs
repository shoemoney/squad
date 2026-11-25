//! A bounded tachyonfx adapter. Every pass is isolated to a clipped scratch buffer.
use super::{
    FxLevel,
    caps::ColorTier,
    cue::{TimedCue, seed_bytes},
    governor::{FxPolicy, Governor},
    manager::FxKey,
    palette::map_color,
};
use ratatui::{buffer::Buffer, layout::Rect};
use std::time::Duration;
use tachyonfx::{Effect, EffectManager, SimpleRng};

/// Only these reviewed colour-only operations may style protected message bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorOnly {
    Dim,
    CometTail,
    ClearWipe,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassAccess {
    Unprotected,
    ColorOnly(ColorOnly),
    BlankCells,
}

type Painter = dyn FnMut(Duration, &mut Buffer, FxPolicy);
struct Catalogue {
    duration: Duration,
    paint: Box<Painter>,
}
impl std::fmt::Debug for Catalogue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalogue")
            .field("duration", &self.duration)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct Pass {
    effects: EffectManager<FxKey>,
    area: Rect,
    pending: Option<(FxKey, Effect)>,
    pub access: PassAccess,
    pub full_buffer: bool,
    elapsed: Duration,
    scratch: Buffer,
    catalogue: Option<Catalogue>,
}
impl Pass {
    pub fn new(key: FxKey, effect: Effect, area: Rect, access: PassAccess) -> Self {
        let effect = effect.with_rng(SimpleRng::new(seed_bytes(format!("{key:?}").as_bytes())));
        let effects = EffectManager::default();
        Self {
            effects,
            area,
            pending: Some((key, effect)),
            access,
            full_buffer: false,
            elapsed: Duration::ZERO,
            scratch: Buffer::empty(Rect::default()),
            catalogue: None,
        }
    }
    /// Adapt any in-house effect to the same protected, tier-aware, bounded runner.
    /// The caller draws the complete authoritative static widget first. The painter
    /// receives absolute elapsed time and current governor policy; no final paint is
    /// performed after duration, cancellation or settlement.
    pub fn catalogue(
        key: FxKey,
        area: Rect,
        access: PassAccess,
        duration: Duration,
        paint: impl FnMut(Duration, &mut Buffer, FxPolicy) + 'static,
    ) -> Self {
        let mut pass = Self::new(key, tachyonfx::fx::sleep(0), area, access);
        pass.pending = None;
        pass.catalogue = Some(Catalogue {
            duration,
            paint: Box::new(paint),
        });
        pass
    }
    /// Production catalogue adapter: seed every stochastic shader from its cue identity.
    pub fn for_cue(cue: &TimedCue, effect: Effect, area: Rect, access: PassAccess) -> Self {
        let mut pass = Self::new(cue.cue.key(), effect, area, access);
        if let Some((_, effect)) = &mut pass.pending {
            *effect = effect.clone().with_rng(SimpleRng::new(cue.rng_seed()));
        }
        pass
    }
    pub fn full_buffer(mut self) -> Self {
        self.full_buffer = true;
        self
    }
    pub(crate) fn catalogue_duration(&self) -> Option<Duration> {
        self.catalogue.as_ref().map(|c| c.duration)
    }
    pub fn running(&self) -> bool {
        self.catalogue
            .as_ref()
            .is_some_and(|p| self.elapsed < p.duration)
            || self.pending.is_some()
            || self.effects.is_running()
    }
    /// Absolute elapsed time prevents frame jitter and skipped draws changing duration.
    pub fn process(
        &mut self,
        elapsed: Duration,
        buffer: &mut Buffer,
        viewport: Rect,
        protected: &[Rect],
    ) {
        self.process_in_tier(elapsed, buffer, viewport, protected, ColorTier::TrueColor);
    }
    pub(crate) fn process_in_tier(
        &mut self,
        elapsed: Duration,
        buffer: &mut Buffer,
        viewport: Rect,
        protected: &[Rect],
        tier: ColorTier,
    ) {
        self.process_with_policy(
            elapsed,
            buffer,
            viewport,
            protected,
            tier,
            Governor::default().policy(FxLevel::Full, true),
        );
    }
    pub(crate) fn process_with_policy(
        &mut self,
        elapsed: Duration,
        buffer: &mut Buffer,
        viewport: Rect,
        protected: &[Rect],
        tier: ColorTier,
        policy: FxPolicy,
    ) {
        let area = self.area.intersection(viewport).intersection(buffer.area);
        if self.catalogue.is_some() && self.scratch.area != area {
            self.scratch.resize(area);
        }
        let delta_ms = elapsed.as_millis().saturating_sub(self.elapsed.as_millis());
        self.elapsed = elapsed;
        // Bound allocations to the viewport; the TUI settles passes before resize.
        if let Some((key, effect)) = self.pending.take() {
            self.scratch.resize(area);
            self.effects.add_unique_effect(key, effect.with_area(area));
        }
        let area = area.intersection(self.scratch.area);
        // Every scratch cell in the clipped area is overwritten by the static frame.
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                self.scratch[(x, y)] = buffer[(x, y)].clone();
            }
        }
        if let Some(catalogue) = &mut self.catalogue {
            if elapsed >= catalogue.duration {
                return;
            }
            (catalogue.paint)(elapsed, &mut self.scratch, policy);
        } else {
            self.effects.process_effects(
                tachyonfx::Duration::from_millis(delta_ms.min(u128::from(u32::MAX)) as u32),
                &mut self.scratch,
                area,
            );
            // The complete static widget is the final frame, even for destructive shaders.
            if !self.effects.is_running() {
                return;
            }
        }
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                // A wide glyph continuation is blank but cannot accept a glyph.
                let continuation = x > buffer.area.x
                    && unicode_width::UnicodeWidthStr::width(buffer[(x - 1, y)].symbol()) > 1;
                let original = &mut buffer[(x, y)];
                if !matches!(self.access, PassAccess::ColorOnly(_))
                    && (continuation
                        || original.diff_option == ratatui::buffer::CellDiffOption::Skip)
                {
                    continue;
                }
                let (fg, bg) = (original.fg, original.bg);
                let is_protected = protected.iter().any(|r| r.contains((x, y).into()));
                let painted = &self.scratch[(x, y)];
                match self.access {
                    PassAccess::Unprotected if !is_protected => *original = painted.clone(),
                    PassAccess::BlankCells if !is_protected && original.symbol() == " " => {
                        *original = painted.clone()
                    }
                    // Never copy symbols, width metadata, or skip flags from a colour-only pass.
                    PassAccess::ColorOnly(_) => {
                        original.fg = painted.fg;
                        original.bg = painted.bg;
                    }
                    _ => continue,
                }
                // Every painter, including custom catalogue effects, shares tier enforcement.
                if tier == ColorTier::Ansi16 {
                    if matches!(
                        original.fg,
                        ratatui::style::Color::Rgb(..) | ratatui::style::Color::Indexed(16..=255)
                    ) {
                        original.fg = fg;
                    }
                    if matches!(
                        original.bg,
                        ratatui::style::Color::Rgb(..) | ratatui::style::Color::Indexed(16..=255)
                    ) {
                        original.bg = bg;
                    }
                }
                original.fg = map_color(original.fg, tier);
                original.bg = map_color(original.bg, tier);
                if tier == ColorTier::NoColor {
                    original.fg = fg;
                    original.bg = bg;
                }
            }
        }
    }
}
