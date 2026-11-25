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
use squad_tui::effects::probe::{Branch, Level, Motion, ProbeEffect};
fn tree() -> Buffer {
    report(
        &[
            "Root: measure voltage",
            "├── high: A or B remain inseparable",
            "│   └── next check: inspect cable",
            "└── Unexpected result: revisit assumptions",
            "Matrix: A check1 cannot-predict · B check1 high",
            "Explainers: a,b · Checkers: c,d · Sit-out: e",
            "Skipped check: no permission · partial failure",
        ],
        70,
    )
}
fn branches() -> Vec<Branch> {
    vec![
        Branch {
            connectors: vec![(0, 1), (1, 1), (2, 1), (0, 2), (4, 2), (5, 2), (6, 2)],
        },
        Branch {
            connectors: vec![(0, 3), (1, 3), (2, 3)],
        },
    ]
}
#[test]
fn storyboard_root_first_unexpected_and_inseparable() {
    let e = ProbeEffect::new(Motion::new(Level::Full, false, false), 1, &branches(), GOLD);
    let mut shots = String::new();
    for t in [0, 225, 450, 900, 916] {
        let mut b = tree();
        e.apply(&mut b, ms(t), Rect::new(0, 0, 70, 12));
        assert_eq!(glyphs(&b), glyphs(&tree()));
        shots.push_str(&format!("{t}\n{}\n", frame(&b)));
    }
    insta::assert_snapshot!(shots);
}
#[test]
fn order_no_node_accents_duplicate_and_rejected_output() {
    let mut e = ProbeEffect::new(Motion::new(Level::Full, false, false), 1, &branches(), GOLD);
    assert!(!e.replace(1, Some(&[])));
    let mut b = tree();
    e.apply(&mut b, ms(0), Rect::new(0, 0, 70, 12));
    assert_eq!(b[(0, 1)].fg, GOLD);
    assert_ne!(b[(0, 3)].fg, GOLD);
    // Even an erroneous connector coordinate cannot alter the static node style.
    assert!(e.replace(
        2,
        Some(&[Branch {
            connectors: vec![(0, 0), (0, 3)]
        }])
    ));
    let mut b = tree();
    e.apply(&mut b, ms(0), Rect::new(0, 0, 70, 12));
    assert_eq!(b[(0, 0)], tree()[(0, 0)]);
    // Rejection cannot replace the last accepted report or consume its key.
    assert!(!e.replace(3, None));
    let mut b = tree();
    e.apply(&mut b, ms(450), Rect::new(0, 0, 70, 12));
    assert_ne!(b[(0, 3)].fg, tree()[(0, 3)].fg);
    assert_eq!(glyphs(&b), glyphs(&tree()));
    assert!(e.replace(3, Some(&branches())));
}
#[test]
fn lifecycle_levels_narrow_wrap_and_partial_tree() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let mut e = ProbeEffect::new(Motion::new(level, nc, false), 1, &branches(), GOLD);
            let mut b = tree();
            e.apply(&mut b, ms(916), Rect::new(0, 0, 70, 12));
            assert_eq!(b, tree());
            let mut narrow = report(
                &[
                    "Root: voltage",
                    "├── high: A or B",
                    "│   inseparable",
                    "└── Unexpected result",
                    "    revisit assumptions",
                ],
                25,
            );
            let original = glyphs(&narrow);
            e.apply(&mut narrow, ms(450), Rect::new(0, 0, 25, 10));
            assert_eq!(glyphs(&narrow), original);
            for _event in ["stop", "collapse", "resize", "failure"] {
                e.motion.settle();
                let mut b = tree();
                e.apply(&mut b, ms(0), Rect::new(0, 0, 8, 2));
                assert_eq!(b, tree());
                assert!(e.is_done(ms(0)));
                assert!(!e.replace(1, Some(&branches())));
            }
            let replay = ProbeEffect::new(Motion::new(level, nc, true), 1, &branches(), GOLD);
            let mut b = tree();
            replay.apply(&mut b, ms(0), Rect::new(0, 0, 70, 12));
            assert_eq!(b, tree());
        }
    }
}
