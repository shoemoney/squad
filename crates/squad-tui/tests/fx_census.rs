use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use std::time::Duration;
const GOLD: Color = Color::Rgb(255, 210, 63);
fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}
fn report(lines: &[&str], width: u16) -> Buffer {
    let mut b = Buffer::empty(Rect::new(0, 0, width, lines.len() as u16 + 5));
    b.set_style(b.area, Style::default().fg(Color::Rgb(150, 150, 150)));
    for (y, line) in lines.iter().enumerate() {
        b.set_stringn(
            0,
            y as u16,
            line,
            width as usize,
            Style::default().fg(Color::Rgb(150, 150, 150)),
        );
    }
    b
}
fn glyphs(b: &Buffer) -> String {
    (b.area.y..b.area.bottom())
        .map(|y| {
            (b.area.x..b.area.right())
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
use squad_tui::fx::census::{CensusEffect, Estimate, Level, Motion};
fn range(a: u64, b: u64) -> Option<Estimate> {
    Some(Estimate { low: a, high: b })
}
fn static_report() -> Buffer {
    report(
        &[
            "Found 9; estimated 3 to 7 still unfound.",
            "Models share blind spots; true figure at least this.",
            "G1 singleton · plan §2 citation · partial member failure",
            "Discovery pass1 9 · pass2 2",
            "Send singletons to $grill.",
        ],
        72,
    )
}
fn draw(e: &CensusEffect, t: u64, narrow: bool) -> Buffer {
    let mut b = static_report();
    e.render_graphic(&mut b, Rect::new(0, 6, 60, 1), ms(t), narrow, GOLD);
    b
}
#[test]
fn storyboard() {
    let mut shots = String::new();
    for (name, prev, now) in [
        ("first", None, range(3, 7)),
        ("falling", range(12, 20), range(3, 7)),
        ("rising", range(3, 7), range(12, 20)),
        ("unavailable", range(3, 7), None),
    ] {
        let e = CensusEffect::new(Motion::new(Level::Full, false, false), 1, prev, now);
        for t in [0, 100, 200, 400, 416] {
            let b = draw(&e, t, false);
            assert!(!glyphs(&b).contains("Census is quiet"));
            for y in 0..5 {
                for x in 0..72 {
                    assert_eq!(b[(x, y)], static_report()[(x, y)]);
                }
            }
            shots.push_str(&format!("{name} {t}\n{}\n", glyphs(&b)));
        }
    }
    insta::assert_snapshot!(shots);
}
#[test]
fn endpoints_no_zero_or_forced_decline_and_retarget() {
    let mut e = CensusEffect::new(
        Motion::new(Level::Full, false, false),
        1,
        range(3, 7),
        range(12, 20),
    );
    assert_eq!(e.value(ms(0), false), range(3, 7));
    assert_eq!(e.value(ms(200), false), range(9, 16));
    assert!(!e.retarget(1, range(0, 0), ms(200)));
    assert_eq!(e.value(ms(200), false), range(9, 16));
    assert!(e.retarget(2, range(20, 30), ms(200)));
    assert_eq!(e.value(ms(0), false), range(9, 16));
    assert!(e.retarget(3, None, ms(100)));
    assert_eq!(e.value(ms(0), false), None);
    assert!(e.retarget(4, range(u64::MAX - 1, u64::MAX), ms(0)));
    assert_eq!(e.value(ms(0), false), range(u64::MAX - 1, u64::MAX));
}
#[test]
fn lifecycle_levels_and_narrow() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let mut e =
                CensusEffect::new(Motion::new(level, nc, false), 1, range(12, 20), range(3, 7));
            let off = CensusEffect::new(
                Motion::new(Level::Off, nc, false),
                1,
                range(12, 20),
                range(3, 7),
            );
            assert_eq!(draw(&e, 416, false), draw(&off, 0, false));
            assert_eq!(draw(&e, 0, true), draw(&off, 0, true));
            for _event in ["stop", "collapse", "resize", "partial failure"] {
                e.motion.settle();
                assert!(e.is_done(ms(0)));
                assert_eq!(draw(&e, 0, false), draw(&off, 0, false));
                assert!(!e.motion.begin(1));
            }
            let replay =
                CensusEffect::new(Motion::new(level, nc, true), 1, range(12, 20), range(3, 7));
            assert_eq!(draw(&replay, 0, false), draw(&off, 0, false));
        }
    }
}
