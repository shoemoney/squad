//! Bounded, deterministic particles shared by sparks and verdict confetti.
use std::time::Duration;

use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::Rect,
    style::Color,
};
use unicode_width::UnicodeWidthChar;

pub const CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub position: Point,
    pub velocity: Point,
    pub color: Color,
    pub glyph: char,
    pub remaining: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            position: Point::default(),
            velocity: Point::default(),
            color: Color::Reset,
            glyph: '*',
            remaining: 0.0,
        }
    }
}

/// Spread is the +/- deviation from velocity; lifetimes are sampled inclusively.
/// A full pool drops new particles rather than replacing a live celebration.
#[derive(Clone, Copy, Debug)]
pub struct Burst<'a> {
    pub count: usize,
    pub origin: Point,
    pub velocity: Point,
    pub spread: Point,
    pub colors: &'a [Color],
    pub glyphs: &'a [char],
    pub lifetime: (Duration, Duration),
}

#[derive(Clone, Debug)]
pub struct ParticlePool {
    particles: [Particle; CAPACITY],
    rng: u64,
    gravity: f32,
}

impl ParticlePool {
    pub fn new(seed: u64, gravity: f32) -> Self {
        Self {
            particles: [Particle::default(); CAPACITY],
            rng: if seed == 0 { 0x9e3779b97f4a7c15 } else { seed },
            gravity: if gravity.is_finite() { gravity } else { 0.0 },
        }
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / 16_777_215.0
    }

    pub fn spawn(&mut self, burst: Burst<'_>) -> usize {
        if ![
            burst.origin.x,
            burst.origin.y,
            burst.velocity.x,
            burst.velocity.y,
            burst.spread.x,
            burst.spread.y,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return 0;
        }
        let low = burst.lifetime.0.min(burst.lifetime.1).as_secs_f32();
        let high = burst.lifetime.0.max(burst.lifetime.1).as_secs_f32();
        if high == 0.0 {
            return 0;
        }
        let mut spawned = 0;
        for i in 0..CAPACITY {
            if spawned == burst.count {
                break;
            }
            if self.particles[i].remaining > 0.0 {
                continue;
            }
            let vx = burst.velocity.x + (self.random() * 2.0 - 1.0) * burst.spread.x.abs();
            let vy = burst.velocity.y + (self.random() * 2.0 - 1.0) * burst.spread.y.abs();
            let remaining = low + self.random() * (high - low);
            let color = if burst.colors.is_empty() {
                Color::Reset
            } else {
                burst.colors
                    [(self.random() * burst.colors.len() as f32) as usize % burst.colors.len()]
            };
            let glyph = if burst.glyphs.is_empty() {
                '*'
            } else {
                burst.glyphs
                    [(self.random() * burst.glyphs.len() as f32) as usize % burst.glyphs.len()]
            };
            self.particles[i] = Particle {
                position: burst.origin,
                velocity: Point { x: vx, y: vy },
                color,
                glyph: if glyph.width() == Some(1) && !glyph.is_control() {
                    glyph
                } else {
                    '*'
                },
                remaining,
            };
            spawned += 1;
        }
        spawned
    }

    /// Exact constant-acceleration integration, independent of frame subdivision.
    pub fn step(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32();
        for p in &mut self.particles {
            if p.remaining <= 0.0 {
                continue;
            }
            let t = dt.min(p.remaining);
            p.position.x += p.velocity.x * t;
            p.position.y += p.velocity.y * t + 0.5 * self.gravity * t * t;
            p.velocity.y += self.gravity * t;
            p.remaining = (p.remaining - dt).max(0.0);
        }
    }

    pub fn active(&self) -> impl Iterator<Item = &Particle> {
        self.particles.iter().filter(|p| p.remaining > 0.0)
    }

    pub fn is_done(&self) -> bool {
        self.active().next().is_none()
    }

    pub fn clear(&mut self) {
        for p in &mut self.particles {
            p.remaining = 0.0;
        }
    }

    pub fn draw(&self, area: Rect, buffer: &mut Buffer) {
        self.draw_protected(area, buffer, &[], false);
    }

    /// Call only for Full; Lite/Off omit the pool. Redraw static widgets before
    /// every frame. NO_COLOR preserves particles' glyphs using default foreground.
    pub fn draw_protected(
        &self,
        area: Rect,
        buffer: &mut Buffer,
        protected: &[Rect],
        no_color: bool,
    ) {
        let area = area.intersection(buffer.area);
        for p in self.active() {
            let x = p.position.x.round();
            let y = p.position.y.round();
            if x < f32::from(area.x)
                || x >= f32::from(area.right())
                || y < f32::from(area.y)
                || y >= f32::from(area.bottom())
            {
                continue;
            }
            let pos = (x as u16, y as u16);
            if protected.iter().any(|r| r.contains(pos.into())) {
                continue;
            }
            // Wide glyph continuation cells are blank too; never decorate those.
            if pos.0 > buffer.area.x
                && buffer[(pos.0 - 1, pos.1)]
                    .symbol()
                    .chars()
                    .map(|c| c.width().unwrap_or(0))
                    .sum::<usize>()
                    > 1
            {
                continue;
            }
            let cell = &mut buffer[pos];
            if cell.symbol() == " " && cell.diff_option != CellDiffOption::Skip {
                cell.set_char(p.glyph)
                    .set_fg(if no_color { Color::Reset } else { p.color });
            }
        }
    }
}
