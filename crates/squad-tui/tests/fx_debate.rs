use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use squad_tui::fx::debate::*;
use std::time::Duration;
fn presentation(level: Level, no_color: bool) -> Presentation {
    Presentation { level, no_color }
}
fn versus() -> VersusCard<'static> {
    VersusCard {
        a: Contender {
            name: "claude",
            wins: 4,
            losses: 2,
            color: Color::Yellow,
        },
        b: Contender {
            name: "gemini",
            wins: 2,
            losses: 3,
            color: Color::Cyan,
        },
        topic: "Can a terminal be a broadcast stage?",
        seed: 17,
    }
}
fn graphic(width: u16, t: Duration, p: Presentation) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, 13));
    buf.set_string(0, 12, "protected message body", Style::default());
    versus().draw(Rect::new(0, 0, width, 7), &mut buf, t, p);
    ScoreSlam {
        scores: Scores { a: 8, b: 7 },
        colors: [Color::Yellow, Color::Cyan],
    }
    .draw(Rect::new(0, 7, 30, 2), &mut buf, t, p);
    TugBar {
        previous: Scores { a: 4, b: 5 },
        current: Scores { a: 12, b: 11 },
        colors: [Color::Yellow, Color::Cyan],
    }
    .draw(Rect::new(0, 10, 31, 1), &mut buf, t, p);
    LeadFlip {
        previous: Scores { a: 4, b: 5 },
        current: Scores { a: 12, b: 11 },
        color: Color::Yellow,
    }
    .draw(
        Rect::new(0, 9, 40, 1),
        Rect::new(0, 10, 31, 1),
        &mut buf,
        t,
        p,
        None,
    );
    buf
}
#[test]
fn debate_storyboards_and_static_settlement() {
    let mut frames = Vec::new();
    for width in [60, 100] {
        for level in [Level::Full, Level::Lite, Level::Off] {
            for nc in [false, true] {
                let p = presentation(level, nc);
                for ms in [
                    0, 100, 120, 130, 200, 260, 280, 370, 400, 416, 520, 536, 740, 800, 816, 1480,
                    1496,
                ] {
                    let buf = graphic(width, Duration::from_millis(ms), p);
                    assert_eq!(buf[(0, 12)].symbol(), "p");
                    if ms >= 1480 {
                        assert_eq!(
                            buf,
                            graphic(width, Duration::ZERO, presentation(Level::Off, nc))
                        );
                    }
                    frames.push((format!("{width} {level:?} nc={nc} {ms}"), buf));
                }
            }
        }
    }
    insta::assert_debug_snapshot!(frames);
}
#[test]
fn scores_tug_and_lead_use_authoritative_values() {
    let s = ScoreSlam {
        scores: Scores { a: 8, b: 7 },
        colors: [Color::Yellow, Color::Cyan],
    };
    assert_eq!(s.sample(Duration::ZERO, Level::Full), Scores { a: 0, b: 0 });
    assert_eq!(s.sample(SCORE_DURATION, Level::Full), s.scores);
    assert_eq!(s.sample(Duration::ZERO, Level::Off), s.scores);
    assert!(s.is_done(SCORE_DURATION));
    let bar = TugBar {
        previous: Scores { a: 0, b: 0 },
        current: Scores {
            a: u32::MAX,
            b: u32::MAX,
        },
        colors: s.colors,
    };
    assert_eq!(bar.split(TUG_DURATION, Level::Full, 30), 15);
    assert!(bar.is_done(TUG_DURATION));
    let mut flip = LeadFlip {
        previous: Scores { a: 1, b: 2 },
        current: Scores { a: 3, b: 2 },
        color: Color::Yellow,
    };
    assert!(flip.triggered());
    assert!(flip.is_done(LEAD_DURATION));
    assert_eq!(
        flip.row_highlight(Duration::ZERO, presentation(Level::Full, false)),
        Some(Color::White)
    );
    flip.current = Scores { a: 2, b: 2 };
    assert!(!flip.triggered());
    flip.previous = Scores { a: 0, b: 0 };
    flip.current = Scores { a: 2, b: 1 };
    assert!(!flip.triggered());
    assert!(versus().is_done(VERSUS_DURATION));
}
#[test]
fn clips_to_nonzero_rect_and_tiny_buffers() {
    for size in [0, 1, 2, 3] {
        let mut b = Buffer::empty(Rect::new(10, 20, size, size));
        let area = b.area;
        versus().draw(
            area,
            &mut b,
            Duration::ZERO,
            presentation(Level::Full, false),
        );
        ScoreSlam {
            scores: Scores { a: 10, b: 0 },
            colors: [Color::Yellow, Color::Cyan],
        }
        .draw(
            area,
            &mut b,
            Duration::ZERO,
            presentation(Level::Full, false),
        );
    }
}
