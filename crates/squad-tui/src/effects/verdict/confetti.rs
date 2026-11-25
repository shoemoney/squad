//! Swap this adapter for effects/particles.rs during integration. Fixed storage,
//! seeded ballistic sampling: no per-frame RNG, allocation, or mutable clock.
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use std::time::Duration;

pub(super) trait Confetti {
    fn draw(&self, buf: &mut Buffer, card: Rect, protected: &[Rect], t: Duration, hues: [Color; 2]);
}
#[derive(Clone, Copy, Default)]
struct Particle {
    side: bool,
    vx: f32,
    vy: f32,
    life: f32,
    glyph: usize,
    hue: usize,
}
pub(super) struct LocalConfetti {
    particles: [Particle; 160],
}
impl LocalConfetti {
    pub fn new(seed: u64) -> Self {
        let mut state = seed.max(1);
        let mut random = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 32) as u32 as f32 / u32::MAX as f32
        };
        let particles = std::array::from_fn(|i| Particle {
            side: i % 2 != 0,
            vx: 2.0 + random() * 23.0,
            vy: -4.0 - random() * 12.0,
            life: 1.8 + random() * 0.8,
            glyph: i % 5,
            hue: i % 3,
        });
        Self { particles }
    }
}
impl Confetti for LocalConfetti {
    fn draw(
        &self,
        buf: &mut Buffer,
        card: Rect,
        protected: &[Rect],
        t: Duration,
        hues: [Color; 2],
    ) {
        let seconds = t.as_secs_f32();
        for p in &self.particles {
            if seconds >= p.life {
                continue;
            }
            let origin = if p.side {
                card.right().saturating_sub(2)
            } else {
                card.x + 1
            };
            let x = origin as f32 + if p.side { -p.vx } else { p.vx } * seconds;
            let y = card.y as f32 + 1.0 + p.vy * seconds + 15.0 * seconds * seconds;
            if x < 0.0 || y < 0.0 {
                continue;
            }
            let pos = (x.round() as u16, y.round() as u16);
            if !card.contains(pos.into()) || protected.iter().any(|r| r.contains(pos.into())) {
                continue;
            }
            if let Some(cell) = buf.cell_mut(pos)
                && cell.symbol() == " "
            {
                cell.set_symbol(["▪", "●", "◆", "▴", "*"][p.glyph])
                    .set_fg(if p.hue == 2 {
                        super::gold_for(hues[0])
                    } else {
                        hues[p.hue]
                    });
            }
        }
    }
}
