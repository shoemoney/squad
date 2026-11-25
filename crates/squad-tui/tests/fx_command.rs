use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
};
use squad_tui::effects::command::{Command, CommandEffect, Level};
use std::time::Duration;
fn base() -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, 32, 8));
    buf.set_string(
        3,
        1,
        "Message 界 e\u{301} preserved",
        Style::default().fg(Color::Rgb(91, 155, 255)),
    );
    buf.set_string(3, 3, "Second stream", Style::default());
    buf.set_string(1, 7, "input stays", Style::default());
    buf
}
fn effect(kind: Command<'_>) -> CommandEffect<'_> {
    CommandEffect {
        area: Rect::new(0, 0, 32, 6),
        kind,
        gold: Color::Yellow,
        level: Level::Full,
        no_color: false,
    }
}
#[test]
fn storyboard_and_hard_cut() {
    let cursors = [Position::new(26, 1), Position::new(16, 3)];
    let rails = [Rect::new(1, 0, 1, 2), Rect::new(1, 3, 1, 2)];
    let cleared = Buffer::empty(base().area);
    for (name, kind, duration) in [
        (
            "brake",
            Command::Stop {
                cursors: &cursors,
                rails: &rails,
                line: Rect::new(0, 5, 32, 1),
            },
            180,
        ),
        (
            "wipe",
            Command::Clear {
                cleared: &cleared,
                synchronized: true,
            },
            280,
        ),
    ] {
        let fx = effect(kind);
        for ms in [0, duration / 4, duration / 2, duration, duration + 17] {
            let mut buf = base();
            fx.apply(
                Duration::from_millis(ms),
                &mut buf,
                &[Rect::new(3, 1, 23, 1), Rect::new(3, 3, 13, 1)],
            );
            insta::assert_snapshot!(format!("{name}_{ms}"), format!("{buf:#?}"));
        }
    }
}
#[test]
fn all_protected_glyphs_survive_until_cut_and_off_matches() {
    let cleared = Buffer::empty(base().area);
    let cursors = [Position::new(26, 1), Position::new(16, 3)];
    let rails = [Rect::new(1, 0, 1, 5)];
    for kind in [
        Command::Stop {
            cursors: &cursors,
            rails: &rails,
            line: Rect::new(0, 5, 32, 1),
        },
        Command::Clear {
            cleared: &cleared,
            synchronized: true,
        },
    ] {
        for level in [Level::Full, Level::Lite, Level::Off] {
            for no_color in [false, true] {
                let fx = CommandEffect {
                    level,
                    no_color,
                    ..effect(kind)
                };
                for ms in 0..fx.duration().as_millis() as u64 {
                    let original = base();
                    let mut buf = original.clone();
                    fx.apply(
                        Duration::from_millis(ms),
                        &mut buf,
                        &[Rect::new(3, 1, 23, 1), Rect::new(3, 3, 13, 1)],
                    );
                    for (y, end) in [(1, 26), (3, 16)] {
                        for x in 3..end {
                            assert_eq!(buf[(x, y)].symbol(), original[(x, y)].symbol());
                        }
                    }
                    assert_eq!(buf[(1, 7)], original[(1, 7)]);
                }
                let mut end = base();
                fx.apply(fx.duration(), &mut end, &[]);
                let mut off = base();
                CommandEffect {
                    level: Level::Off,
                    ..fx
                }
                .apply(Duration::ZERO, &mut off, &[]);
                assert_eq!(end, off);
                assert!(fx.is_done(fx.duration()));
            }
        }
    }
}
#[test]
fn unsynchronized_wipe_cuts_immediately_to_authoritative_render() {
    let mut cleared = Buffer::empty(base().area);
    cleared.set_string(2, 2, "room cleared", Style::default());
    let fx = effect(Command::Clear {
        cleared: &cleared,
        synchronized: false,
    });
    let mut buf = base();
    fx.apply(Duration::ZERO, &mut buf, &[fx.area]);
    for y in 0..6 {
        for x in 0..32 {
            assert_eq!(buf[(x, y)], cleared[(x, y)]);
        }
    }
    assert!(fx.is_done(Duration::ZERO));
}
