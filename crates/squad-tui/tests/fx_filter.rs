use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::fx::filter::{ColorTier, Filter};
use std::time::Duration;
#[test]
fn pure_filters_and_no_color() {
    for tier in [
        ColorTier::TrueColor,
        ColorTier::Indexed256,
        ColorTier::Ansi16,
        ColorTier::NoColor,
    ] {
        let f = Filter {
            width: 24,
            height: 7,
            tier,
        };
        for t in [0, 300, 600, 750, 900, 1900, 1916] {
            for y in 0..7 {
                for x in 0..24 {
                    let t = Duration::from_millis(t);
                    assert_eq!(f.gay(x, y, t), f.gay(x, y, t));
                    if tier == ColorTier::NoColor {
                        assert_eq!(f.gay(x, y, t), Color::Reset);
                        assert_eq!(f.metal(x, y, t), Color::Reset);
                        assert_eq!(f.duotone(x, y, t, Color::Red, Color::Yellow), Color::Reset);
                    }
                    if tier == ColorTier::Ansi16 {
                        assert!(!matches!(
                            f.gay(x, y, t),
                            Color::Rgb(..) | Color::Indexed(_)
                        ));
                        assert!(!matches!(
                            f.metal(x, y, t),
                            Color::Rgb(..) | Color::Indexed(_)
                        ));
                        assert!(!matches!(
                            f.duotone(x, y, t, Color::Rgb(255, 0, 0), Color::Yellow),
                            Color::Rgb(..) | Color::Indexed(_)
                        ));
                    }
                }
            }
        }
    }
    let f = Filter {
        width: 24,
        height: 7,
        tier: ColorTier::TrueColor,
    };
    assert_eq!(
        f.gay(0, 0, Duration::ZERO),
        f.gay(0, 0, Duration::from_secs(3))
    );
    assert_ne!(
        f.gay(0, 0, Duration::ZERO),
        f.gay(0, 0, Duration::from_millis(500))
    );
    assert_eq!(f.metal(8, 3, Duration::ZERO), Color::Rgb(255, 255, 255));
    assert_eq!(f.metal(0, 0, Duration::ZERO), Color::Rgb(232, 241, 255));
    assert_eq!(f.metal(0, 6, Duration::ZERO), Color::Rgb(39, 64, 107));
    assert_eq!(
        f.metal(12, 0, Duration::from_millis(750)),
        Color::Rgb(255, 255, 255)
    );
    assert_ne!(
        f.metal(12, 0, Duration::from_millis(916)),
        Color::Rgb(255, 255, 255)
    );
    for tier in [
        ColorTier::TrueColor,
        ColorTier::Indexed256,
        ColorTier::Ansi16,
        ColorTier::NoColor,
    ] {
        let f = Filter {
            width: 0,
            height: 0,
            tier,
        };
        f.gay(0, 0, Duration::ZERO);
        f.metal(0, 0, Duration::ZERO);
        f.duotone(0, 0, Duration::ZERO, Color::Red, Color::Yellow);
    }
}
#[test]
fn filter_storyboards() {
    let mut frames = Vec::new();
    for tier in [
        ColorTier::TrueColor,
        ColorTier::Indexed256,
        ColorTier::Ansi16,
        ColorTier::NoColor,
    ] {
        for ms in [0, 475, 750, 950, 1900, 1916] {
            let f = Filter {
                width: 12,
                height: 5,
                tier,
            };
            let mut buf = Buffer::empty(Rect::new(0, 0, 36, 5));
            for y in 0..5 {
                for x in 0..12 {
                    let t = Duration::from_millis(ms);
                    for (offset, color) in [
                        (0, f.gay(x, y, t)),
                        (12, f.metal(x, y, t)),
                        (
                            24,
                            f.duotone(x, y, t, Color::Rgb(224, 130, 90), Color::Rgb(91, 155, 255)),
                        ),
                    ] {
                        buf[(x + offset, y)].set_char('█').set_fg(color);
                    }
                }
            }
            frames.push((format!("{tier:?} {ms}"), buf));
        }
    }
    insta::assert_debug_snapshot!(frames);
}
