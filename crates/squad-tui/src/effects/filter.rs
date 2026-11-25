//! Pure, position/time sampled banner colours. Coordinates are local to a banner.
use ratatui::style::Color;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTier {
    TrueColor,
    Ansi256,
    Ansi16,
    NoColor,
}

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
        let a = to_lab(rgb(a));
        let b = to_lab(rgb(b));
        let blend = fraction.clamp(0.0, 1.0);
        let shade = 1.0 - self.row(y) * 0.55;
        self.finish(from_lab([
            (a[0] + (b[0] - a[0]) * blend) * shade,
            (a[1] + (b[1] - a[1]) * blend) * shade,
            (a[2] + (b[2] - a[2]) * blend) * shade,
        ]))
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
        if self.tier == ColorTier::Ansi256 {
            let cube = |c: u8| {
                [0_u8, 95, 135, 175, 215, 255]
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| c.abs_diff(**v))
                    .map(|(i, _)| i as u8)
                    .unwrap_or(0)
            };
            Color::Indexed(16 + 36 * cube(rgb.0) + 6 * cube(rgb.1) + cube(rgb.2))
        } else {
            Color::Rgb(rgb.0, rgb.1, rgb.2)
        }
    }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let lerp = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round() as u8;
    (lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}
fn oklch(l: f64, c: f64, h: f64) -> (u8, u8, u8) {
    from_lab([l, c * h.to_radians().cos(), c * h.to_radians().sin()])
}
fn from_lab(v: [f64; 3]) -> (u8, u8, u8) {
    let [l, a, b] = v;
    let ll = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
    let channel = |v: f64| {
        let v = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    (
        channel(4.0767416621 * ll - 3.3077115913 * m + 0.2309699292 * s),
        channel(-1.2684380046 * ll + 2.6097574011 * m - 0.3413193965 * s),
        channel(-0.0041960863 * ll - 0.7034186147 * m + 1.707614701 * s),
    )
}
fn to_lab(c: (u8, u8, u8)) -> [f64; 3] {
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(c.0), linear(c.1), linear(c.2));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ]
}
const ANSI: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];
const COLORS: [Color; 16] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];
fn rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) if i < 16 => ANSI[usize::from(i)],
        Color::Indexed(i) if i >= 232 => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
        Color::Indexed(i) => {
            let n = i - 16;
            let v = |x: u8| if x == 0 { 0 } else { 55 + 40 * x };
            (v(n / 36), v(n / 6 % 6), v(n % 6))
        }
        _ => COLORS
            .iter()
            .position(|v| *v == c)
            .map(|i| ANSI[i])
            .unwrap_or((255, 255, 255)),
    }
}
fn ansi16(c: Color) -> Color {
    if COLORS.contains(&c) || c == Color::Reset {
        return c;
    }
    let c = rgb(c);
    let index = ANSI
        .iter()
        .enumerate()
        .min_by_key(|(_, v)| {
            (i32::from(c.0) - i32::from(v.0)).pow(2)
                + (i32::from(c.1) - i32::from(v.1)).pow(2)
                + (i32::from(c.2) - i32::from(v.2)).pow(2)
        })
        .map(|(i, _)| i)
        .unwrap_or(15);
    COLORS[index]
}
