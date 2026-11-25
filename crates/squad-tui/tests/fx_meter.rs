use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::effects::meter::{Heat, Level, Meter, Palette, Spend};
use std::time::Duration;
fn fx(level: Level, no_color: bool) -> Meter {
    Meter {
        level,
        no_color,
        palette: Palette {
            gold: Color::Yellow,
            orange: Color::LightYellow,
            red: Color::Red,
            pending: Color::Gray,
        },
    }
}
fn spend(actual: u64) -> Spend {
    Spend {
        day: 1,
        actual: actual * 1_000_000,
        pending: 2_000_000,
        max: 20_000_000,
        unknown_calls: 1,
    }
}
fn draw(f: Meter, s: Spend, ms: u64) -> Buffer {
    let mut b = Buffer::empty(Rect::new(0, 0, 80, 2));
    f.render(&mut b, Rect::new(0, 0, 80, 2), s, Duration::from_millis(ms));
    b
}
#[test]
fn storyboard() {
    let mut frames = Vec::new();
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            for actual in [14, 15, 18, 20] {
                frames.push((level, nc, actual, draw(fx(level, nc), spend(actual), 0)));
            }
        }
    }
    insta::assert_debug_snapshot!(frames);
}
#[test]
fn odometer_and_record_storyboard() {
    let mut frames = Vec::new();
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            for ms in [0, 37, 75, 150, 167] {
                let f = fx(level, nc);
                let t = Duration::from_millis(ms);
                let mut b = Buffer::empty(Rect::new(0, 0, 30, 3));
                b.set_string(
                    0,
                    0,
                    f.odometer("$9.99", "$10.04", t),
                    ratatui::style::Style::default(),
                );
                for (row, winner) in [(1, true), (2, false)] {
                    let (text, style) = f.record_tick(9, 10, winner, t);
                    b.set_string(0, row, text, style);
                }
                frames.push((level, nc, ms, b));
            }
        }
    }
    insta::assert_debug_snapshot!(frames);
}
#[test]
fn authoritative_state_transitions() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let f = fx(level, nc);
            assert_eq!(spend(14).heat(), Heat::Normal);
            assert_eq!(spend(15).heat(), Heat::Warm);
            assert_eq!(spend(18).heat(), Heat::Hot);
            assert_eq!(spend(20).heat(), Heat::Max);
            let before = spend(18);
            let committed = Spend {
                actual: 20_000_000,
                pending: 0,
                unknown_calls: 0,
                ..before
            };
            let b = draw(f, committed, 0);
            assert!(!b.content.iter().any(|c| c.symbol() == "▒"));
            assert!(!committed.labels().contains('~'));
            assert!(before.labels().starts_with("~$18.00 / $20.00"));
            assert!(before.labels().contains("pending $2.00"));
            let rollover = Spend {
                day: 2,
                actual: 0,
                pending: 0,
                unknown_calls: 0,
                ..committed
            };
            let raised = Spend {
                max: 40_000_000,
                ..committed
            };
            assert_eq!(rollover.heat(), Heat::Normal);
            assert_eq!(raised.heat(), Heat::Normal);
            assert!(!format!("{:?}", draw(f, rollover, 0)).contains("DAILY MAX"));
            assert_eq!(draw(f, raised, 500), draw(f, raised, 0));
            assert_eq!(draw(f, committed, 500), draw(f, committed, 0));
            assert_eq!(
                f.odometer("$1.00", "$2.00", Duration::from_millis(150)),
                "$2.00"
            );
            assert_eq!(
                f.record_tick(1, 2, true, Duration::from_millis(150)),
                fx(Level::Off, nc).record_tick(1, 2, true, Duration::ZERO)
            );
        }
    }
}
#[test]
fn pending_never_triggers_max_and_sweep_preserves_glyphs() {
    let f = fx(Level::Full, false);
    let pending = Spend {
        pending: 100_000_000,
        ..spend(1)
    };
    assert_eq!(pending.heat(), Heat::Normal);
    let mut b = draw(f, spend(20), 0);
    let original = b.clone();
    f.max_sweep(
        &mut b,
        Rect::new(0, 0, 80, 2),
        spend(19),
        spend(20),
        Duration::from_millis(140),
        &[Rect::new(40, 0, 1, 2)],
    );
    for (a, b) in b.content.iter().zip(&original.content) {
        assert_eq!(a.symbol(), b.symbol());
    }
    assert_eq!(b[(40, 0)], original[(40, 0)]);
    assert!(f.next_wake(spend(18)).is_some());
    assert!(f.next_wake(spend(20)).is_none());
    assert_ne!(draw(f, spend(18), 0), draw(f, spend(18), 1000));
}

#[test]
fn reservation_replacement_storyboard() {
    let mut frames = Vec::new();
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let f = fx(level, nc);
            let previous = Spend {
                actual: 3_000_000,
                pending: 6_000_000,
                ..spend(3)
            };
            let current = Spend {
                actual: 7_000_000,
                pending: 0,
                unknown_calls: 0,
                ..previous
            };
            for ms in [0, 37, 75, 150, 167] {
                let mut b = Buffer::empty(Rect::new(0, 0, 80, 2));
                f.render_transition(
                    &mut b,
                    Rect::new(0, 0, 80, 2),
                    previous,
                    current,
                    Duration::from_millis(ms),
                );
                let exact = draw(f, current, ms);
                for x in 0..80 {
                    assert_eq!(b[(x, 0)], exact[(x, 0)]);
                }
                if ms >= 150 || level != Level::Full || nc {
                    assert_eq!(b, exact);
                }
                frames.push((level, nc, ms, b));
            }
            for changed in [
                Spend {
                    day: 2,
                    actual: 0,
                    pending: 0,
                    ..current
                },
                Spend {
                    max: 40_000_000,
                    ..current
                },
            ] {
                assert_eq!(f.transition(previous, changed, Duration::ZERO), changed);
            }
        }
    }
    insta::assert_debug_snapshot!(frames);
}
#[test]
fn exact_thresholds_and_extreme_values() {
    for (amount, heat) in [
        (14_999_999, Heat::Normal),
        (15_000_000, Heat::Warm),
        (17_999_999, Heat::Warm),
        (18_000_000, Heat::Hot),
        (19_999_999, Heat::Hot),
        (20_000_000, Heat::Max),
    ] {
        assert_eq!(
            Spend {
                actual: amount,
                ..spend(0)
            }
            .heat(),
            heat
        );
    }
    let max = Spend {
        actual: u64::MAX,
        pending: u64::MAX,
        max: u64::MAX,
        ..spend(0)
    };
    assert_eq!(max.heat(), Heat::Max);
    draw(fx(Level::Full, false), max, 0);
    let f = fx(Level::Full, false);
    let mut b = Buffer::empty(Rect::new(0, 0, 4, 1));
    f.render(&mut b, Rect::new(90, 90, 0, 0), max, Duration::ZERO);
}

#[test]
fn compact_and_status_line_keep_labels_and_bar() {
    let mut frames = Vec::new();
    for size in [(24, 2), (80, 1), (80, 2)] {
        for amount in [3, 18, 20] {
            for nc in [false, true] {
                let mut b = Buffer::empty(Rect::new(0, 0, size.0, size.1));
                fx(Level::Full, nc).render(
                    &mut b,
                    Rect::new(0, 0, size.0, size.1),
                    spend(amount),
                    Duration::ZERO,
                );
                let line: String = (0..size.0).map(|x| b[(x, 0)].symbol()).collect();
                assert!(line.contains(&format!("~${amount}.00 / $20.00")));
                assert!(
                    b.content
                        .iter()
                        .any(|c| c.symbol() == "▰" || c.symbol() == "▱"),
                    "{size:?} {amount} {nc} {b:?}"
                );
                if nc {
                    assert!(
                        b.content
                            .iter()
                            .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
                    );
                }
                frames.push((size, amount, nc, b));
            }
        }
    }
    insta::assert_debug_snapshot!(frames);
}
