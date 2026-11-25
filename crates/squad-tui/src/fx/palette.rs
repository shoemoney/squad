//! Broadcast-night colour identity. Interpolation is perceptual, never RGB rainbow mixing.
use super::caps::ColorTier;
use ratatui::style::{Color, Modifier, Style};

pub const GOLD: Color = Color::Rgb(255, 210, 63);
pub const MEMBERS: [&str; 6] = ["claude", "fable", "openai", "gemini", "grok", "muse"];
pub const SIGNATURES: [Color; 6] = [
    Color::Rgb(224, 130, 90),
    Color::Rgb(167, 139, 250),
    Color::Rgb(25, 195, 154),
    Color::Rgb(91, 155, 255),
    Color::Rgb(230, 233, 239),
    Color::Rgb(255, 79, 163),
];
pub const ANSI_MEMBERS: [Color; 12] = [
    Color::Yellow,
    Color::Magenta,
    Color::Green,
    Color::Cyan,
    Color::Blue,
    Color::LightMagenta,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightCyan,
    Color::White,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}
impl Oklch {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        fn linear(v: u8) -> f64 {
            let v = f64::from(v) / 255.;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        let (r, g, b) = (linear(r), linear(g), linear(b));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
        let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
        Self {
            l: 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            c: a.hypot(b),
            h: b.atan2(a).to_degrees().rem_euclid(360.),
        }
    }
    fn linear_rgb(self) -> [f64; 3] {
        let (a, b) = (
            self.c * self.h.to_radians().cos(),
            self.c * self.h.to_radians().sin(),
        );
        let l = (self.l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m = (self.l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s = (self.l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        [
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        ]
    }
    /// Reduce chroma at fixed hue/lightness to fit sRGB, avoiding clipped gradient bands.
    pub fn color(self) -> Color {
        let mut mapped = Self {
            l: self.l.clamp(0., 1.),
            c: self.c.max(0.),
            ..self
        };
        let (mut lo, mut hi) = (0., mapped.c);
        // In-gamut colours need no chroma search. The old loop repeatedly
        // tested the identical value sixteen times in this common case.
        let in_gamut = mapped.linear_rgb().iter().all(|v| (0. ..=1.).contains(v));
        if in_gamut {
            lo = mapped.c;
        }
        for _ in 0..if in_gamut { 0 } else { 16 } {
            if mapped.linear_rgb().iter().all(|v| (0. ..=1.).contains(v)) {
                lo = mapped.c;
            } else {
                hi = mapped.c;
            }
            mapped.c = (lo + hi) * 0.5;
        }
        mapped.c = lo;
        let rgb = mapped.linear_rgb().map(|v| {
            let v = v.clamp(0., 1.);
            let encoded = if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            };
            (encoded * 255.).round() as u8
        });
        Color::Rgb(rgb[0], rgb[1], rgb[2])
    }
    pub fn mix(self, other: Self, t: f64) -> Self {
        let t = t.clamp(0., 1.);
        let mut a = self;
        let mut b = other;
        if a.c < 0.0001 {
            a.h = b.h;
        }
        if b.c < 0.0001 {
            b.h = a.h;
        }
        let arc = (b.h - a.h + 180.).rem_euclid(360.) - 180.;
        Self {
            l: a.l + (b.l - a.l) * t,
            c: a.c + (b.c - a.c) * t,
            h: (a.h + arc * t).rem_euclid(360.),
        }
    }
}
pub fn interpolate(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(r, g, b), Color::Rgb(r2, g2, b2)) => Oklch::from_rgb(r, g, b)
            .mix(Oklch::from_rgb(r2, g2, b2), t)
            .color(),
        _ => {
            if t < 0.5 {
                a
            } else {
                b
            }
        }
    }
}
pub fn darken(color: Color, factor: f64) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let mut c = Oklch::from_rgb(r, g, b);
            c.l *= factor.clamp(0., 1.);
            c.color()
        }
        _ => color,
    }
}
/// Nearest xterm cube entry; deliberately independent of emulator-specific ANSI slots.
pub fn cube256(color: Color) -> Color {
    fn nearest(v: u8) -> u8 {
        [0u8, 95, 135, 175, 215, 255]
            .iter()
            .enumerate()
            .min_by_key(|(_, n)| (i16::from(v) - i16::from(**n)).abs())
            .map(|(i, _)| i as u8)
            .unwrap_or(0)
    }
    match color {
        Color::Rgb(r, g, b) => Color::Indexed(16 + 36 * nearest(r) + 6 * nearest(g) + nearest(b)),
        _ => color,
    }
}
#[derive(Debug, Clone)]
pub struct Palette {
    pub tier: ColorTier,
    slots: [Color; 12],
}
impl Palette {
    pub fn new(tier: ColorTier) -> Self {
        let mut slots = [Color::Reset; 12];
        slots[..6].copy_from_slice(&SIGNATURES);
        let mut hues: Vec<f64> = SIGNATURES
            .iter()
            .map(|c| match *c {
                Color::Rgb(r, g, b) => Oklch::from_rgb(r, g, b).h,
                _ => unreachable!(),
            })
            .collect();
        for slot in &mut slots[6..] {
            let distance = |h: f64| {
                hues.iter()
                    .map(|used| {
                        let d = (h - used).abs();
                        d.min(360. - d)
                    })
                    .fold(360., f64::min)
            };
            let hue = (0..360)
                .max_by(|a, b| distance(f64::from(*a)).total_cmp(&distance(f64::from(*b))))
                .unwrap_or(0) as f64;
            *slot = Oklch {
                l: 0.74,
                c: 0.15,
                h: hue,
            }
            .color();
            hues.push(hue);
        }
        Self { tier, slots }
    }
    pub fn member(&self, nick: &str, roster_slot: usize) -> Color {
        if nick == "me" {
            return Color::Reset;
        }
        let slot = MEMBERS
            .iter()
            .position(|member| *member == nick)
            .unwrap_or(roster_slot.min(11));
        match self.tier {
            ColorTier::TrueColor => self.slots[slot],
            ColorTier::Indexed256 => cube256(self.slots[slot]),
            ColorTier::Ansi16 => ANSI_MEMBERS[slot],
            ColorTier::NoColor => Color::Reset,
        }
    }
    pub fn chrome(&self) -> Color {
        match self.tier {
            ColorTier::TrueColor => GOLD,
            ColorTier::Indexed256 => cube256(GOLD),
            ColorTier::Ansi16 => Color::Yellow,
            ColorTier::NoColor => Color::Reset,
        }
    }
    pub fn body(&self) -> Color {
        Color::Reset
    }
    pub fn gradient(&self, from: Color, to: Color, t: f64) -> Style {
        match self.tier {
            ColorTier::TrueColor => Style::default().fg(interpolate(from, to, t)),
            ColorTier::Indexed256 => Style::default().fg(cube256(interpolate(from, to, t))),
            ColorTier::Ansi16 => Style::default()
                .fg(if t < 0.5 { from } else { to })
                .add_modifier(if t < 0.5 {
                    Modifier::DIM
                } else {
                    Modifier::BOLD
                }),
            ColorTier::NoColor => Style::default().add_modifier(if t < 0.5 {
                Modifier::DIM
            } else {
                Modifier::BOLD
            }),
        }
    }
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
fn nearest_ansi16(c: Color) -> Color {
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

/// Map any catalogue colour to the startup terminal tier.
pub fn map_color(color: Color, tier: ColorTier) -> Color {
    match tier {
        ColorTier::TrueColor => color,
        ColorTier::Indexed256 => cube256(color),
        ColorTier::Ansi16 => nearest_ansi16(color),
        ColorTier::NoColor => Color::Reset,
    }
}
