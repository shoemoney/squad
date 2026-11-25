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
use squad_tui::effects::survey::{Level, Motion, SurveyEffect, Votes};
fn votes(a: u32, d: u32, u: u32, m: u32) -> Votes {
    Votes {
        agree: a,
        disagree: d,
        unsure: u,
        missing: m,
    }
}
fn static_report() -> Buffer {
    report(
        &[
            "Consensus · C1 5–1",
            "Contested · C2 confidence 4 · reason: evidence",
            "Lone insights · C3",
            "Lone dissents · C4",
            "Rejected outliers · C5 likely error",
            "Switched: grok · settling question: which evidence?",
        ],
        75,
    )
}
fn draw(e: &SurveyEffect, t: u64, narrow: bool) -> Buffer {
    let mut b = static_report();
    e.render_graphic(&mut b, Rect::new(0, 7, 70, 4), ms(t), narrow, [GOLD; 3]);
    b
}
#[test]
fn storyboard_all_bands_and_revote() {
    let mut shots = String::new();
    for (name, previous) in [("initial", None), ("re-vote", Some(votes(1, 4, 1, 0)))] {
        let e = SurveyEffect::new(
            Motion::new(Level::Full, false, false),
            1,
            previous,
            votes(3, 1, 1, 1),
        );
        for t in [0, 100, 200, 400, 416] {
            let b = draw(&e, t, false);
            for y in 0..6 {
                for x in 0..75 {
                    assert_eq!(b[(x, y)], static_report()[(x, y)]);
                }
            }
            shots.push_str(&format!("{name} {t}\n{}\n", glyphs(&b)));
        }
    }
    insta::assert_snapshot!(shots);
}
#[test]
fn stored_proportions_missing_votes_and_supersession() {
    let mut e = SurveyEffect::new(
        Motion::new(Level::Full, false, false),
        1,
        Some(votes(1, 4, 1, 0)),
        votes(3, 1, 1, 1),
    );
    assert_eq!(e.boundaries(ms(0), false), [166666, 833333]);
    assert_eq!(e.boundaries(ms(400), false), [600000, 800000]);
    let midway = e.boundaries(ms(200), false);
    assert!(!e.retarget(1, votes(0, 0, 0, 6), ms(200)));
    assert!(e.retarget(2, votes(0, 0, 0, 6), ms(200)));
    assert_eq!(e.boundaries(ms(0), false), midway);
    assert!(glyphs(&draw(&e, 400, false)).contains("UNSURE 0  MISSING 6"));
    let all_unsure = SurveyEffect::new(
        Motion::new(Level::Full, false, false),
        1,
        Some(votes(0, 0, 6, 0)),
        votes(3, 2, 1, 0),
    );
    let b = draw(&all_unsure, 0, false);
    assert!(b[(0, 8)].symbol() == "?");
}
#[test]
fn lifecycle_levels_partial_votes_and_narrow() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let mut e =
                SurveyEffect::new(Motion::new(level, nc, false), 1, None, votes(2, 1, 1, 2));
            let off = SurveyEffect::new(
                Motion::new(Level::Off, nc, false),
                1,
                None,
                votes(2, 1, 1, 2),
            );
            assert_eq!(draw(&e, 416, false), draw(&off, 0, false));
            assert_eq!(draw(&e, 0, true), draw(&off, 0, true));
            let narrow = glyphs(&draw(&e, 0, true));
            for label in ["AGREE 2", "DISAGREE 1", "UNSURE 1", "MISSING 2"] {
                assert!(narrow.contains(label));
            }
            for _event in ["stop", "collapse", "resize", "failure"] {
                e.motion.settle();
                assert!(e.is_done(ms(0)));
                assert_eq!(draw(&e, 0, false), draw(&off, 0, false));
                assert!(!e.motion.begin(1));
            }
            let replay =
                SurveyEffect::new(Motion::new(level, nc, true), 1, None, votes(2, 1, 1, 2));
            assert_eq!(draw(&replay, 0, false), draw(&off, 0, false));
        }
    }
}

#[test]
fn an_empty_previous_tally_is_neutral_not_unsure() {
    let mut e = SurveyEffect::new(
        Motion::new(Level::Full, false, false),
        1,
        Some(votes(0, 0, 0, 6)),
        votes(2, 1, 1, 2),
    );
    let b = draw(&e, 0, false);
    assert_eq!(b[(0, 8)].symbol(), "·");
    assert!(e.retarget(2, votes(0, 0, 0, 6), ms(400)));
    assert!(e.retarget(3, votes(2, 1, 1, 2), ms(400)));
    let b = draw(&e, 0, false);
    assert_eq!(b[(0, 8)].symbol(), "·");
}
