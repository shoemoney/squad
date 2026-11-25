use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
};
use squad_tui::effects::dispatch::{Dispatch, DispatchEffect, Level, Target};
use std::time::Duration;

fn base() -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
    buf.set_string(3, 1, "human quote 界", Style::default());
    buf.set_string(3, 4, "Other body stays readable", Style::default());
    buf.set_string(31, 2, "claude", Style::default());
    buf.set_string(31, 5, "gemini", Style::default());
    buf.set_string(1, 9, "━━━━━━━━━━━━━━━━━━━━━━━━━━", Style::default());
    buf
}
fn effect(kind: Dispatch<'_>) -> DispatchEffect<'_> {
    DispatchEffect {
        area: Rect::new(0, 0, 40, 10),
        kind,
        gold: Color::Yellow,
        level: Level::Full,
        no_color: false,
        terminal: Rect::new(0, 0, 120, 40),
        ansi16: false,
    }
}
#[test]
fn storyboard() {
    let targets = [
        Target {
            row: Rect::new(30, 2, 8, 1),
            hue: Color::Red,
        },
        Target {
            row: Rect::new(30, 5, 8, 1),
            hue: Color::Blue,
        },
    ];
    let others = [Rect::new(1, 4, 27, 1)];
    for (name, kind, duration) in [
        (
            "launch",
            Dispatch::Launch {
                gutter: Rect::new(1, 0, 1, 9),
                border: Rect::new(1, 9, 26, 1),
            },
            180,
        ),
        (
            "spokes",
            Dispatch::Spokes {
                origin: Position::new(1, 1),
                targets: &targets,
            },
            320,
        ),
        (
            "lasso",
            Dispatch::Quote {
                rail: Rect::new(1, 0, 1, 3),
                others: &others,
            },
            120,
        ),
    ] {
        let fx = effect(kind);
        for ms in [0, duration / 4, duration / 2, duration, duration + 17] {
            let mut buf = base();
            fx.apply(
                Duration::from_millis(ms),
                &mut buf,
                &[Rect::new(3, 1, 14, 1), Rect::new(3, 4, 25, 1)],
            );
            insta::assert_snapshot!(format!("{name}_{ms}"), format!("{buf:#?}"));
        }
    }
}
#[test]
fn fallback_settlement_and_glyph_safety() {
    let targets = [Target {
        row: Rect::new(30, 2, 8, 1),
        hue: Color::Red,
    }];
    let others = [Rect::new(1, 4, 27, 1)];
    for kind in [
        Dispatch::Launch {
            gutter: Rect::new(1, 0, 1, 9),
            border: Rect::new(1, 9, 26, 1),
        },
        Dispatch::Spokes {
            origin: Position::new(1, 1),
            targets: &targets,
        },
        Dispatch::Quote {
            rail: Rect::new(1, 0, 1, 3),
            others: &others,
        },
    ] {
        for level in [Level::Full, Level::Lite, Level::Off] {
            for no_color in [false, true] {
                let fx = DispatchEffect {
                    level,
                    no_color,
                    ..effect(kind)
                };
                let original = base();
                for ms in (0..400).step_by(17) {
                    let mut buf = original.clone();
                    fx.apply(
                        Duration::from_millis(ms),
                        &mut buf,
                        &[Rect::new(3, 1, 26, 5)],
                    );
                    for y in 1..6 {
                        for x in 3..29 {
                            assert_eq!(buf[(x, y)].symbol(), original[(x, y)].symbol());
                        }
                    }
                }
                let mut end = original.clone();
                fx.apply(fx.duration(), &mut end, &[]);
                let mut off = original;
                DispatchEffect {
                    level: Level::Off,
                    ..fx
                }
                .apply(Duration::ZERO, &mut off, &[]);
                if matches!(kind, Dispatch::Quote { .. }) && level == Level::Lite && !no_color {
                    // Lite deliberately omits background dimming. Chrome and all
                    // glyphs still match the final Off selection exactly.
                    for y in 0..10 {
                        for x in 0..40 {
                            assert_eq!(end[(x, y)].symbol(), off[(x, y)].symbol());
                        }
                    }
                } else {
                    assert_eq!(end, off);
                }
            }
        }
    }
    for terminal in [Rect::new(0, 0, 99, 40), Rect::new(0, 0, 120, 29)] {
        let fx = DispatchEffect {
            terminal,
            ..effect(Dispatch::Spokes {
                origin: Position::new(1, 1),
                targets: &targets,
            })
        };
        let mut buf = base();
        fx.apply(Duration::from_millis(80), &mut buf, &[]);
        assert_eq!(buf, base());
        fx.apply(Duration::from_millis(180), &mut buf, &[]);
        assert_ne!(buf, base());
        assert_eq!(buf[(31, 2)].symbol(), "c");
    }
}

#[test]
fn fallback_snapshots_and_selection_persists() {
    let targets = [Target {
        row: Rect::new(30, 2, 8, 1),
        hue: Color::Rgb(224, 130, 90),
    }];
    for (name, level, no_color) in [
        ("narrow", Level::Full, false),
        ("lite", Level::Lite, false),
        ("no_color", Level::Full, true),
    ] {
        let fx = DispatchEffect {
            level,
            no_color,
            terminal: Rect::new(0, 0, 80, 24),
            ..effect(Dispatch::Spokes {
                origin: Position::new(1, 1),
                targets: &targets,
            })
        };
        let mut buf = base();
        fx.apply(Duration::from_millis(200), &mut buf, &[]);
        insta::assert_snapshot!(name, format!("{buf:#?}"));
    }
    let others = [Rect::new(1, 4, 27, 1)];
    let fx = effect(Dispatch::Quote {
        rail: Rect::new(1, 0, 1, 3),
        others: &others,
    });
    let mut buf = base();
    fx.apply(Duration::from_secs(30), &mut buf, &[]);
    assert!(buf[(3, 4)].modifier.contains(ratatui::style::Modifier::DIM));
    assert_eq!(buf[(1, 0)].symbol(), "┃");
}
