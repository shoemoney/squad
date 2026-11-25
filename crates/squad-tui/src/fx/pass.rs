//! A bounded tachyonfx adapter. Every pass is isolated to a clipped scratch buffer.
use super::{
    caps::ColorTier,
    cue::{TimedCue, seed_bytes},
    manager::FxKey,
    palette::cube256,
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

#[derive(Debug)]
pub struct Pass {
    effects: EffectManager<FxKey>,
    area: Rect,
    pending: Option<(FxKey, Effect)>,
    pub access: PassAccess,
    pub full_buffer: bool,
    elapsed: Duration,
    scratch: Buffer,
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
        }
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
    pub fn running(&self) -> bool {
        self.pending.is_some() || self.effects.is_running()
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
        let area = self.area.intersection(viewport).intersection(buffer.area);
        let delta_ms = elapsed.as_millis().saturating_sub(self.elapsed.as_millis());
        self.elapsed = elapsed;
        // Bound allocations to the viewport; the TUI settles passes before resize.
        if let Some((key, effect)) = self.pending.take() {
            self.scratch.resize(area);
            self.effects.add_unique_effect(key, effect.with_area(area));
        }
        let area = area.intersection(self.scratch.area);
        self.scratch.reset();
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                self.scratch[(x, y)] = buffer[(x, y)].clone();
            }
        }
        self.effects.process_effects(
            tachyonfx::Duration::from_millis(delta_ms.min(u128::from(u32::MAX)) as u32),
            &mut self.scratch,
            area,
        );
        // The complete static widget is the final frame, even for destructive shaders.
        if !self.effects.is_running() {
            return;
        }
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let original = &mut buffer[(x, y)];
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
                // A third-party shader cannot escape the terminal's resolved colour tier.
                match tier {
                    ColorTier::NoColor => {
                        original.fg = fg;
                        original.bg = bg;
                    }
                    ColorTier::Indexed256 => {
                        original.fg = cube256(original.fg);
                        original.bg = cube256(original.bg);
                    }
                    ColorTier::Ansi16 => {
                        // ANSI gradients are two-step styling; preserve the static member
                        // assignment if a shader tries to introduce an RGB blend.
                        if matches!(
                            original.fg,
                            ratatui::style::Color::Rgb(..)
                                | ratatui::style::Color::Indexed(16..=255)
                        ) {
                            original.fg = fg;
                        }
                        if matches!(
                            original.bg,
                            ratatui::style::Color::Rgb(..)
                                | ratatui::style::Color::Indexed(16..=255)
                        ) {
                            original.bg = bg;
                        }
                    }
                    ColorTier::TrueColor => {}
                }
            }
        }
    }
}
