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
use squad_tui::fx::crosscheck::{CellAccent, CrosscheckEffect, Level, Motion};
fn cells(n: usize) -> Vec<CellAccent> {
    (0..n)
        .rev()
        .map(|i| CellAccent {
            grid_index: i,
            rect: Rect::new((i % 10 * 7) as u16, (i / 10) as u16, 6, 1),
        })
        .collect()
}
fn grid(n: usize) -> Buffer {
    let mut b = report(&[], 72);
    b.resize(Rect::new(0, 0, 72, 24));
    b.set_style(b.area, Style::default().fg(Color::Rgb(150, 150, 150)));
    for i in 0..n {
        b.set_string(
            (i % 10 * 7) as u16,
            (i / 10) as u16,
            format!(
                "{:03}{}",
                i,
                ["OPEN", "PEND", "SKIP", "CHAL", "RISK", "CITE", "EXCL"][i % 7]
            ),
            Style::default().fg(Color::Rgb(150, 150, 150)),
        );
    }
    b
}
#[test]
fn storyboard_single_and_200_cell_audit() {
    let mut shots = String::new();
    for n in [1, 200] {
        let e = CrosscheckEffect::new(Motion::new(Level::Full, false, false), 1, cells(n), GOLD);
        let duration = e.duration().as_millis() as u64;
        for t in [0, duration / 4, duration / 2, duration, duration + 16] {
            let mut b = grid(n);
            e.apply(&mut b, ms(t), Rect::new(0, 0, 72, 24), None);
            assert_eq!(glyphs(&b), glyphs(&grid(n)));
            shots.push_str(&format!("n={n} t={t}\n{}\n", frame(&b)));
        }
    }
    insta::assert_snapshot!(shots);
}
#[test]
fn duplicates_audit_supersession_and_grid_order() {
    let mut e = CrosscheckEffect::new(Motion::new(Level::Full, false, false), 1, cells(200), GOLD);
    assert!(!e.replace(1, vec![]));
    let mut b = grid(200);
    e.apply(&mut b, ms(0), b_area(), None);
    assert_eq!(b[(0, 0)].fg, GOLD);
    assert_ne!(b[(7, 0)].fg, GOLD);
    assert!(e.replace(
        2,
        vec![CellAccent {
            grid_index: 3,
            rect: Rect::new(21, 0, 6, 1)
        }]
    ));
    let mut b = grid(200);
    b.set_string(21, 0, "003CHAL", Style::default());
    let original = glyphs(&b);
    e.apply(&mut b, ms(0), b_area(), None);
    assert_eq!(glyphs(&b), original);
    assert!(glyphs(&b).contains("CHAL"));
    assert!(!glyphs(&b).contains("COMPLETE"));
}
fn b_area() -> Rect {
    Rect::new(0, 0, 72, 24)
}
#[test]
fn lifecycle_levels_narrow_and_partial_results() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let mut e = CrosscheckEffect::new(Motion::new(level, nc, false), 1, cells(200), GOLD);
            let mut b = grid(200);
            e.apply(&mut b, ms(916), b_area(), None);
            assert_eq!(b, grid(200));
            let mut narrow = report(
                &[
                    "OPEN 30 · PENDING 20 · SKIPPED 2",
                    "coverage summary · audit unresolved",
                ],
                38,
            );
            let before = glyphs(&narrow);
            e.apply(
                &mut narrow,
                ms(0),
                Rect::new(0, 0, 38, 7),
                Some(Rect::new(0, 1, 38, 1)),
            );
            assert_eq!(glyphs(&narrow), before);
            for _event in ["stop", "collapse", "resize", "failure"] {
                e.motion.settle();
                let mut b = grid(200);
                e.apply(&mut b, ms(0), Rect::new(0, 0, 8, 2), None);
                assert_eq!(b, grid(200));
                assert!(e.is_done(ms(0)));
                assert!(!e.replace(1, cells(200)));
            }
            let replay = CrosscheckEffect::new(Motion::new(level, nc, true), 1, cells(200), GOLD);
            let mut b = grid(200);
            replay.apply(&mut b, ms(0), b_area(), None);
            assert_eq!(b, grid(200));
        }
    }
}
