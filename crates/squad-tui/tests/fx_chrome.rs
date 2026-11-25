use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use squad_tui::effects::chrome::{Chrome, Level};
use std::time::Duration;
fn fx(level: Level, no_color: bool) -> Chrome {
    Chrome {
        level,
        no_color,
        ansi16: false,
        gold: Color::Yellow,
        rainbow: &[Color::Red, Color::Green, Color::Blue],
        seed: 42,
    }
}
fn stage() -> Buffer {
    let mut b = Buffer::empty(Rect::new(0, 0, 20, 5));
    for (y, line) in [
        "┌──────────────────┐",
        "│      SQUAD       │",
        "│ protected body   │",
        "│                  │",
        "└──────────────────┘",
    ]
    .iter()
    .enumerate()
    {
        b.set_string(0, y as u16, line, Style::default());
    }
    b
}
#[test]
fn storyboard() {
    let mut frames = Vec::new();
    for ms in [0, 275, 550, 700, 900, 1100, 1117] {
        let mut b = stage();
        fx(Level::Full, false).logo(
            &mut b,
            Rect::new(7, 1, 5, 1),
            Duration::from_millis(ms),
            &[],
        );
        frames.push(("logo", ms, b));
    }
    for ms in [0, 60, 120, 240, 257] {
        let mut b = stage();
        fx(Level::Full, false).roster(
            &mut b,
            Rect::new(2, 3, 16, 1),
            "claude",
            false,
            Color::Cyan,
            Duration::from_millis(ms),
        );
        frames.push(("roster", ms, b));
    }
    for ms in [0, 5000, 10000, 20000, 20017] {
        let mut b = stage();
        fx(Level::Full, false).breath(
            &mut b,
            Rect::new(0, 0, 20, 5),
            Duration::from_millis(ms),
            &[],
        );
        frames.push(("breath", ms, b));
    }
    for ms in [0, 62, 125, 250, 267] {
        let mut b = stage();
        fx(Level::Full, false).power_down(
            &mut b,
            Rect::new(0, 0, 20, 5),
            Duration::from_millis(ms),
            &[Rect::new(2, 2, 16, 1)],
        );
        frames.push(("quit", ms, b));
    }
    for ms in [0, 30, 60, 120, 137] {
        let mut b = stage();
        fx(Level::Full, false).resize(
            &mut b,
            Rect::new(0, 0, 20, 5),
            Duration::from_millis(ms),
            &[],
        );
        frames.push(("resize", ms, b));
    }
    insta::assert_debug_snapshot!(frames);
}
#[test]
fn end_states_protection_seed_and_fallbacks() {
    let rect = Rect::new(0, 0, 20, 5);
    let protected = Rect::new(0, 2, 20, 1);
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let f = fx(level, nc);
            let mut b = stage();
            f.logo(&mut b, rect, Duration::from_secs(2), &[]);
            assert_eq!(b, stage());
            f.breath(&mut b, rect, Duration::from_secs(20), &[]);
            assert_eq!(b, stage());
            f.resize(&mut b, rect, Duration::from_millis(120), &[]);
            assert_eq!(b, stage());
            for ms in [0, 60, 120, 180, 250] {
                let mut b = stage();
                f.logo(&mut b, rect, Duration::from_millis(ms), &[protected]);
                f.power_down(&mut b, rect, Duration::from_millis(ms), &[protected]);
                for x in 0..20 {
                    assert_eq!(b[(x, 2)], stage()[(x, 2)]);
                }
            }
            let mut a = stage();
            let mut b = stage();
            f.logo(&mut a, rect, Duration::from_millis(200), &[]);
            f.logo(&mut b, rect, Duration::from_millis(200), &[]);
            assert_eq!(a, b);
            let mut a = stage();
            let mut b = stage();
            f.power_down(&mut a, rect, Duration::from_secs(1), &[]);
            fx(Level::Off, nc).power_down(&mut b, rect, Duration::ZERO, &[]);
            assert_eq!(a, b);
        }
    }
}
#[test]
fn clipping_and_failed_resolution() {
    let mut b = stage();
    fx(Level::Full, true).roster(
        &mut b,
        Rect::new(0, 0, 20, 1),
        "失败",
        true,
        Color::Blue,
        Duration::from_secs(1),
    );
    assert_eq!(b[(0, 0)].symbol(), "×");
    fx(Level::Full, false).resize(&mut b, Rect::new(19, 4, 100, 100), Duration::ZERO, &[]);
}
