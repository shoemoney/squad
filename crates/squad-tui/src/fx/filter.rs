//! Pure, position/time sampled banner colours. Coordinates are local to a banner.
use ratatui::style::Color;
use std::time::Duration;

pub use super::ColorTier;

/// A sampler closes over dimensions and palette, leaving `(x, y, t) -> Color`.
#[derive(Clone, Copy, Debug)]
pub struct Filter {
    pub width: u16,
    pub height: u16,
    pub tier: ColorTier,
}

impl Filter {
    pub fn gay(self, x: u16, y: u16, t: Duration) -> Color {
        let hue =
            (f64::from(x) * 9.0 + f64::from(y) * 14.0 + t.as_secs_f64() * 120.0).rem_euclid(360.0);
        match self.tier {
            ColorTier::NoColor => Color::Reset,
            ColorTier::Ansi16 => [
                Color::LightRed,
                Color::LightYellow,
                Color::LightGreen,
                Color::LightCyan,
                Color::LightBlue,
                Color::LightMagenta,
            ][(hue / 60.0) as usize],
            _ => self.finish(oklch(0.78, 0.16, hue)),
        }
    }

    pub fn metal(self, x: u16, y: u16, t: Duration) -> Color {
        if self.tier == ColorTier::NoColor {
            return Color::Reset;
        }
        if self.highlight(x, y, t) {
            return if self.tier == ColorTier::Ansi16 {
                Color::White
            } else {
                self.finish((255, 255, 255))
            };
        }
        let row = self.row(y);
        if self.tier == ColorTier::Ansi16 {
            return if row < 0.5 {
                Color::LightBlue
            } else {
                Color::Blue
            };
        }
        self.finish(if row <= 0.5 {
            mix((232, 241, 255), (127, 166, 217), row * 2.0)
        } else {
            mix((127, 166, 217), (39, 64, 107), (row - 0.5) * 2.0)
        })
    }

    /// Owned RGB hues are blended in OKLab (the rectangular form of OKLCH),
    /// then shaded by the metal ramp. ANSI callers supply their mapped hues.
    pub fn duotone(self, x: u16, y: u16, t: Duration, a: Color, b: Color) -> Color {
        if self.tier == ColorTier::NoColor {
            return Color::Reset;
        }
        let fraction = f64::from(x) / f64::from(self.width.saturating_sub(1).max(1));
        if self.tier == ColorTier::Ansi16 {
            return ansi16(if fraction < 0.5 { a } else { b });
        }
        if self.highlight(x, y, t) {
            return self.finish((255, 255, 255));
        }
        let color = super::palette::darken(
            super::palette::interpolate(a, b, fraction),
            1.0 - self.row(y) * 0.55,
        );
        super::palette::map_color(color, self.tier)
    }

    fn row(self, y: u16) -> f64 {
        (f64::from(y) / f64::from(self.height.saturating_sub(1).max(1))).clamp(0.0, 1.0)
    }
    fn highlight(self, x: u16, y: u16, t: Duration) -> bool {
        let horizon = (f64::from(self.height.saturating_sub(1)) * 0.45).round() as u16;
        let secs = t.as_secs_f64();
        y == horizon
            || ((0.6..0.9).contains(&secs)
                && x == (((secs - 0.6) / 0.3) * f64::from(self.width.saturating_sub(1))).round()
                    as u16)
    }
    fn finish(self, rgb: (u8, u8, u8)) -> Color {
        super::palette::map_color(Color::Rgb(rgb.0, rgb.1, rgb.2), self.tier)
    }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    match super::palette::interpolate(Color::Rgb(a.0, a.1, a.2), Color::Rgb(b.0, b.1, b.2), t) {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => unreachable!(),
    }
}

fn oklch(l: f64, c: f64, h: f64) -> (u8, u8, u8) {
    match (super::palette::Oklch { l, c, h }).color() {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => unreachable!(),
    }
}

fn ansi16(c: Color) -> Color {
    super::palette::map_color(c, ColorTier::Ansi16)
}
