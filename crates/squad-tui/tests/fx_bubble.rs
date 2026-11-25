use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Color,
};
use squad_tui::fx::bubble::{Bubble, Level, Moment};
use std::time::Duration;

fn fixture(moment: Moment, level: Level, no_color: bool) -> (Buffer, Bubble, [Rect; 1]) {
    let area = Rect::new(0, 0, 28, 7);
    let mut buf = Buffer::empty(area);
    buf.set_string(3, 0, "claude", ratatui::style::Style::default());
    buf.set_string(
        3,
        1,
        "Readable 界 e\u{301}",
        ratatui::style::Style::default(),
    );
    buf.set_string(3, 2, "streaming text", ratatui::style::Style::default());
    buf.set_string(
        3,
        5,
        "412 tok · $0.0031 · 3.2s",
        ratatui::style::Style::default(),
    );
    let fx = Bubble {
        area,
        rail: Rect::new(1, 0, 1, 6),
        header: Rect::new(3, 0, 6, 1),
        footer: Rect::new(3, 5, 23, 1),
        cursor: Some(Position::new(17, 2)),
        tail: [
            Some(Position::new(16, 2)),
            Some(Position::new(15, 2)),
            Some(Position::new(14, 2)),
            Some(Position::new(13, 2)),
        ],
        status: Rect::new(21, 0, 5, 1),
        hue: Color::Rgb(224, 130, 90),
        text: Color::Rgb(230, 233, 239),
        error: Color::Red,
        level,
        no_color,
        moment,
    };
    (buf, fx, [Rect::new(3, 1, 14, 3)])
}

fn snapshot(buf: &Buffer) -> String {
    format!("{buf:#?}")
}

#[test]
fn storyboard() {
    for (name, moment, duration) in [
        ("arrival", Moment::Arrival, 280),
        ("comet", Moment::Streaming, 400),
        ("seal", Moment::Complete, 150),
        ("fracture", Moment::Failure, 240),
    ] {
        for ms in [0, duration / 4, duration / 2, duration, duration + 17] {
            let (mut buf, fx, protected) = fixture(moment, Level::Full, false);
            fx.apply(Duration::from_millis(ms), &mut buf, &protected);
            insta::assert_snapshot!(format!("{name}_{ms}"), snapshot(&buf));
        }
    }
}

#[test]
fn static_settlement_and_protected_glyphs() {
    for moment in [
        Moment::Arrival,
        Moment::Streaming,
        Moment::Complete,
        Moment::Failure,
    ] {
        for level in [Level::Full, Level::Lite, Level::Off] {
            for no_color in [false, true] {
                let (base, fx, protected) = fixture(moment, level, no_color);
                for ms in (0..=400).step_by(13) {
                    let mut buf = base.clone();
                    fx.apply(Duration::from_millis(ms), &mut buf, &protected);
                    for y in 1..4 {
                        for x in 3..17 {
                            assert_eq!(buf[(x, y)].symbol(), base[(x, y)].symbol());
                        }
                    }
                }
                if let Some(duration) = fx.duration() {
                    let mut end = base.clone();
                    fx.apply(duration, &mut end, &protected);
                    let mut off = base.clone();
                    Bubble {
                        level: Level::Off,
                        ..fx
                    }
                    .apply(Duration::ZERO, &mut off, &protected);
                    assert_eq!(end, off);
                    assert!(fx.is_done(duration));
                } else {
                    assert!(!fx.is_done(Duration::MAX));
                }
            }
        }
    }
}

#[test]
fn tiny_clipped_rects_and_occupied_cursor_are_safe() {
    let (mut buf, mut fx, protected) = fixture(Moment::Streaming, Level::Full, false);
    fx.cursor = Some(Position::new(3, 1));
    fx.apply(Duration::ZERO, &mut buf, &protected);
    assert_eq!(buf[(3, 1)].symbol(), "R");
    fx.area = Rect::new(99, 99, 0, 0);
    fx.apply(Duration::MAX, &mut buf, &protected);
}

#[test]
fn seal_cursor_has_four_frames_and_error_is_explicit_without_color() {
    for (ms, glyph) in [(0, "█"), (30, "▓"), (60, "▒"), (90, "░"), (120, " ")] {
        let (mut buf, fx, protected) = fixture(Moment::Complete, Level::Full, false);
        fx.apply(Duration::from_millis(ms), &mut buf, &protected);
        assert_eq!(buf[(17, 2)].symbol(), glyph);
    }
    let (mut buf, fx, protected) = fixture(Moment::Failure, Level::Full, true);
    fx.apply(Duration::ZERO, &mut buf, &protected);
    let status = (21..26).map(|x| buf[(x, 0)].symbol()).collect::<String>();
    assert_eq!(status, "[ERR]");
    assert_eq!(buf[(1, 0)].fg, Color::Reset);
    assert!(
        buf[(21, 0)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
}

#[test]
fn comet_tracks_wrapped_tail_without_touching_glyphs() {
    let (mut buf, mut fx, protected) = fixture(Moment::Streaming, Level::Full, false);
    fx.cursor = Some(Position::new(3, 3));
    fx.tail = [
        Some(Position::new(16, 2)),
        Some(Position::new(15, 2)),
        Some(Position::new(14, 2)),
        Some(Position::new(13, 2)),
    ];
    let original = buf.clone();
    fx.apply(Duration::from_secs(2), &mut buf, &protected);
    assert_ne!(buf[(16, 2)].fg, original[(16, 2)].fg);
    for x in 13..17 {
        assert_eq!(buf[(x, 2)].symbol(), original[(x, 2)].symbol());
    }
    // The supplied protected body includes the insertion cell; it remains blank.
    assert_eq!(buf[(3, 3)].symbol(), " ");
}
