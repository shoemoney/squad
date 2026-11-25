use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};
use squad_core::banner::BannerMode;
use squad_tui::fx::{
    banner::{BannerCache, ColourTier, Entrance, Level, Palette},
    pick::{Candidate, Kind, Pick, State},
};
use std::{fmt::Write, time::Duration};
fn candidates() -> [Candidate<'static>; 3] {
    [
        Candidate {
            nick: "grok",
            marker: Rect::new(1, 12, 1, 1),
            accent: Rect::new(3, 12, 12, 1),
            colour: Color::Blue,
        },
        Candidate {
            nick: "gemini",
            marker: Rect::new(1, 13, 1, 1),
            accent: Rect::new(3, 13, 12, 1),
            colour: Color::Cyan,
        },
        Candidate {
            nick: "failed",
            marker: Rect::new(1, 14, 1, 1),
            accent: Rect::new(3, 14, 12, 1),
            colour: Color::Red,
        },
    ]
}
fn base(width: u16, height: u16) -> Buffer {
    let mut b = Buffer::empty(Rect::new(0, 0, width, height));
    for (row, text) in [
        (12, "   @grok eligible"),
        (13, "   @gemini eligible"),
        (14, "   @failed did not answer"),
        (16, "ordinary chat remains usable"),
    ] {
        b.set_string(0, row, text, Style::default());
    }
    b
}
fn pending(kind: Kind, restored: bool) -> Pick {
    Pick::new(kind, ["grok".to_owned(), "gemini".to_owned()], restored)
}
#[test]
fn both_pick_banners_hints_and_candidate_snapshots() {
    let mut cache = BannerCache::default();
    for kind in [Kind::Debate, Kind::Grill] {
        for (w, h) in [(120, 40), (80, 24), (40, 24)] {
            let mut frames = String::new();
            for level in [Level::Full, Level::Lite, Level::Off] {
                for tier in [
                    ColourTier::TrueColor,
                    ColourTier::Indexed256,
                    ColourTier::Ansi16,
                    ColourTier::NoColor,
                ] {
                    let pick = pending(kind, false);
                    let entrance = Entrance::new(
                        &mut cache,
                        kind.banner(),
                        BannerMode::On,
                        w,
                        level,
                        Palette {
                            tier,
                            ..Palette::default()
                        },
                    );
                    let mut b = base(w, h);
                    entrance.render(
                        &mut b,
                        Rect::new(0, 1, w, entrance.art.height() as u16),
                        Duration::from_millis(600),
                        &[],
                    );
                    // Hint wrapping belongs to the widget; the effect doesn't touch it.
                    let hint_y = 2 + entrance.art.height() as u16;
                    let chars: Vec<_> = kind.hint().chars().collect();
                    for (row, line) in chars.chunks(usize::from(w)).enumerate() {
                        b.set_string(
                            0,
                            hint_y + row as u16,
                            line.iter().collect::<String>(),
                            Style::default(),
                        );
                    }
                    pick.render(
                        &mut b,
                        &candidates(),
                        Duration::from_millis(600),
                        level,
                        tier,
                        &[],
                    );
                    writeln!(frames, "{level:?}/{tier:?}").unwrap();
                    for y in 0..18 {
                        let row: String = (0..w).map(|x| b[(x, y)].symbol()).collect();
                        writeln!(frames, "|{}|", row.trim_end()).unwrap();
                    }
                    for (x, y) in [(1, 12), (3, 12), (1, 13), (3, 13), (1, 14)] {
                        writeln!(frames, "({x},{y}) {:?}", b[(x, y)]).unwrap();
                    }
                }
            }
            insta::assert_snapshot!(format!("fx_pick_{kind:?}_{w}"), frames);
        }
    }
}
#[test]
fn eligible_only_rejected_persists_and_all_terminal_states_clear() {
    for kind in [Kind::Debate, Kind::Grill] {
        let mut pick = pending(kind, false);
        let before = base(80, 24);
        let mut b = before.clone();
        pick.render(
            &mut b,
            &candidates(),
            Duration::ZERO,
            Level::Full,
            ColourTier::TrueColor,
            &[],
        );
        assert_eq!(b[(1, 12)].symbol(), "▸");
        assert_eq!(b[(1, 13)].symbol(), "▸");
        assert_eq!(b[(1, 14)], before[(1, 14)]);
        for x in 0..80 {
            assert_eq!(b[(x, 16)], before[(x, 16)]);
        }
        pick.rejected();
        let mut rejected = before.clone();
        pick.render(
            &mut rejected,
            &candidates(),
            Duration::ZERO,
            Level::Full,
            ColourTier::TrueColor,
            &[],
        );
        assert_eq!(b, rejected);
        for state in [State::Accepted, State::Cancelled, State::Failed] {
            pick.update(state);
            let mut cleared = before.clone();
            pick.render(
                &mut cleared,
                &candidates(),
                Duration::ZERO,
                Level::Full,
                ColourTier::TrueColor,
                &[],
            );
            assert_eq!(cleared, before);
            assert_eq!(pick.next_wake(Level::Full, ColourTier::TrueColor), None);
        }
    }
}
#[test]
fn restored_pending_has_static_markers_no_entrance_and_no_timer() {
    for kind in [Kind::Debate, Kind::Grill] {
        for tier in [ColourTier::TrueColor, ColourTier::NoColor] {
            for level in [Level::Full, Level::Lite, Level::Off] {
                let pick = pending(kind, true);
                assert!(!pick.entrance_live());
                assert_eq!(pick.next_wake(level, tier), None);
                let mut a = base(80, 24);
                let mut b = a.clone();
                pick.render(&mut a, &candidates(), Duration::ZERO, level, tier, &[]);
                pick.render(
                    &mut b,
                    &candidates(),
                    Duration::from_millis(800),
                    level,
                    tier,
                    &[],
                );
                assert_eq!(a, b);
                assert_eq!(a[(1, 12)].symbol(), "▸");
                let static_pick = pending(kind, false);
                let mut off = base(80, 24);
                static_pick.render(
                    &mut off,
                    &candidates(),
                    Duration::ZERO,
                    Level::Off,
                    tier,
                    &[],
                );
                assert_eq!(a, off);
            }
        }
    }
}
#[test]
fn protected_rects_and_static_levels_have_no_pulse() {
    let pick = pending(Kind::Debate, false);
    for (level, tier) in [
        (Level::Lite, ColourTier::TrueColor),
        (Level::Off, ColourTier::TrueColor),
        (Level::Full, ColourTier::NoColor),
    ] {
        let mut a = base(80, 24);
        let mut b = a.clone();
        pick.render(
            &mut a,
            &candidates(),
            Duration::ZERO,
            level,
            tier,
            &[Rect::new(0, 13, 80, 1)],
        );
        pick.render(
            &mut b,
            &candidates(),
            Duration::from_millis(1000),
            level,
            tier,
            &[Rect::new(0, 13, 80, 1)],
        );
        assert_eq!(a, b);
        let before = base(80, 24);
        for x in 0..80 {
            assert_eq!(a[(x, 13)], before[(x, 13)]);
        }
        assert_eq!(pick.next_wake(level, tier), None);
    }
    assert_eq!(
        pick.next_wake(Level::Full, ColourTier::TrueColor),
        Some(Duration::from_millis(100))
    );
}
