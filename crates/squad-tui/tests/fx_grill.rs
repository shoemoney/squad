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
fn frame(b: &Buffer) -> String {
    let mut s = glyphs(b);
    s.push_str("\naccents:");
    for y in b.area.y..b.area.bottom() {
        for x in b.area.x..b.area.right() {
            let c = &b[(x, y)];
            if c.symbol() != " " && (c.fg != Color::Rgb(150, 150, 150) || !c.modifier.is_empty()) {
                s.push_str(&format!(" ({x},{y}):{:?}/{:?}", c.fg, c.modifier));
            }
        }
    }
    s
}
use squad_tui::effects::grill::{
    CompletionReason as Reason, GrillEffect, Level, Motion, Treatment,
};
fn effect(level: Level, nc: bool, replay: bool, treatment: Treatment) -> GrillEffect {
    GrillEffect::new(Motion::new(level, nc, replay), 1, treatment, GOLD)
}
fn treatments() -> [Treatment; 3] {
    [
        Treatment::Turn {
            previous: Rect::new(0, 0, 1, 1),
            active: Rect::new(0, 1, 1, 1),
            previous_color: Color::Rgb(224, 130, 90),
            active_color: Color::Rgb(25, 195, 154),
        },
        Treatment::PlanVersion {
            badge: Rect::new(0, 3, 7, 1),
        },
        Treatment::Complete {
            reason: Reason::Survived,
            survived_label: Rect::new(11, 4, 8, 1),
        },
    ]
}
fn static_report() -> Buffer {
    report(
        &[
            "| claude grills grok",
            "| gemini PASS",
            "Q: evidence? A: citation",
            "PLAN v2 last good plan",
            "FINAL PLAN SURVIVED",
            "questions asked 14 · passed 2",
        ],
        70,
    )
}
#[test]
fn storyboard() {
    let mut shots = String::new();
    for (name, treatment) in ["handoff", "version", "survived"]
        .into_iter()
        .zip(treatments())
    {
        let e = effect(Level::Full, false, false, treatment);
        let duration = e.duration().as_millis() as u64;
        for t in [0, duration / 4, duration / 2, duration, duration + 16] {
            let mut b = static_report();
            e.apply(&mut b, ms(t));
            assert_eq!(glyphs(&b), glyphs(&static_report()));
            shots.push_str(&format!("{name} t={t}\n{}\n", frame(&b)));
        }
    }
    insta::assert_snapshot!(shots);
}
#[test]
fn static_fallbacks_and_settlement() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            for treatment in treatments() {
                let mut e = effect(level, nc, false, treatment);
                for t in [0, 30, 90, 150, 400, 416] {
                    let mut b = static_report();
                    e.apply(&mut b, ms(t));
                    assert_eq!(glyphs(&b), glyphs(&static_report()));
                }
                for _event in ["stop", "collapse", "resize", "leader failure"] {
                    e.motion.settle();
                    let mut b = static_report();
                    e.apply(&mut b, ms(0));
                    assert_eq!(b, static_report());
                    assert!(e.is_done(ms(0)));
                    assert!(!e.motion.begin(1));
                }
                let replay = effect(level, nc, true, treatment);
                let mut b = static_report();
                replay.apply(&mut b, ms(0));
                assert_eq!(b, static_report());
            }
        }
    }
}
#[test]
fn completion_reason_is_authoritative_and_failure_keeps_plan() {
    for reason in [Reason::Normal, Reason::Stopped, Reason::LeaderFailed] {
        let e = effect(
            Level::Full,
            false,
            false,
            Treatment::Complete {
                reason,
                survived_label: Rect::new(0, 0, 60, 5),
            },
        );
        let mut b = report(
            &[
                "FAILED · PLAN v2 last good plan",
                "Q: evidence? A: citation",
                "PASS · skipped griller",
            ],
            35,
        );
        let original = b.clone();
        e.apply(&mut b, ms(0));
        assert_eq!(b, original);
        assert!(e.is_done(ms(0)));
    }
}
