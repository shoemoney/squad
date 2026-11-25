use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::fx::{
    tournament::{ChampionReveal, RankingReveal, RankingRow, TournamentFx},
    verdict::{Contender, Level, Tick},
};
use std::time::Duration;
fn rows() -> Vec<RankingRow> {
    [
        ("muse", 81, "Novel and feasible.", false),
        (
            "claude",
            81,
            "Strong fit; accepted second on the judge's tiebreak.",
            false,
        ),
        (
            "gemini",
            73,
            "Good evidence; the attack remains unanswered.",
            true,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (nick, score, critique, eliminated))| RankingRow {
        nick: nick.into(),
        score,
        hue: [Color::Magenta, Color::Yellow, Color::Cyan][i],
        critique: critique.into(),
        eliminated,
    })
    .collect()
}
fn winner() -> Contender {
    Contender {
        nick: "muse".into(),
        hue: Color::Magenta,
        art: vec!["M U S E".into()],
        compact_art: vec!["MUSE".into()],
    }
}
fn render(fx: &RankingReveal, ms: u64, area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    fx.render(&mut buf, area, &[], Duration::from_millis(ms), &rows());
    buf
}
fn text(buf: &Buffer) -> String {
    (buf.area.y..buf.area.bottom())
        .map(|y| {
            (buf.area.x..buf.area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn ranking_snapshots_all_levels() {
    for (name, level) in [
        ("full", Level::Full),
        ("lite", Level::Lite),
        ("off", Level::Off),
    ] {
        for nc in [false, true] {
            let fx = RankingReveal::new(level, nc, true);
            for ms in [0, 250, 500, 700, 900, 916] {
                insta::assert_snapshot!(
                    format!("tournament_{name}_{nc}_{ms}"),
                    text(&render(&fx, ms, Rect::new(0, 0, 90, 12)))
                );
            }
        }
    }
}
#[test]
fn ranking_reveals_bottom_up_with_fixed_rows_and_accepted_ties() {
    let fx = RankingReveal::new(Level::Full, false, true);
    let buf = render(&fx, 200, Rect::new(0, 0, 90, 12));
    assert!(text(&buf).contains("1. muse · score 81"));
    assert!(text(&buf).contains("2. claude · score 81"));
    assert!(text(&buf).contains("3. gemini · score 73 · ELIMINATED"));
    assert_eq!(buf[(3, 2)].symbol(), " "); // best row has not begun its count-up
    assert_eq!(buf[(3, 8)].symbol(), "━"); // last place begins first
    for ms in [0, 100, 250, 500, 900] {
        let buf = render(&fx, ms, Rect::new(0, 0, 90, 12));
        for (y, name) in [(0, "muse"), (3, "claude"), (6, "gemini")] {
            let line = (0..90).map(|x| buf[(x, y)].symbol()).collect::<String>();
            assert!(line.contains(name));
        }
        for row in rows() {
            assert!(text(&buf).contains(&row.critique));
        }
    }
}
#[test]
fn rejected_replay_and_duplicate_cues_have_no_entrances() {
    let mut fx = TournamentFx::default();
    assert!(fx.ranking("r1", false, true, Level::Full, false).is_none());
    assert!(fx.ranking("r1", true, false, Level::Full, false).is_none());
    assert!(fx.ranking("r1", true, true, Level::Full, false).is_some());
    assert!(fx.ranking("r1", true, true, Level::Full, false).is_none());
    assert!(
        fx.elimination("e1", false, true, Level::Full, false)
            .is_none()
    );
    assert!(
        fx.elimination("e1", true, false, Level::Full, false)
            .is_none()
    );
    assert!(
        fx.elimination("e1", true, true, Level::Full, false)
            .is_some()
    );
    assert!(
        fx.elimination("e1", true, true, Level::Full, false)
            .is_none()
    );
    assert!(fx.champion("c1", false, Level::Full, false, 42).is_none());
    assert!(fx.champion("c1", true, Level::Full, false, 42).is_some());
    assert!(fx.champion("c1", true, Level::Full, false, 42).is_none());
}
#[test]
fn eliminated_marker_survives_settle_resize_replay_and_dropped_cue() {
    let mut fx = RankingReveal::new(Level::Full, false, true);
    fx.settle();
    for area in [Rect::new(0, 0, 90, 12), Rect::new(0, 0, 65, 12)] {
        let replay = RankingReveal::new(Level::Full, false, false);
        assert_eq!(render(&fx, 0, area), render(&replay, 0, area));
        assert!(text(&render(&fx, 0, area)).contains("ELIMINATED"));
    }
}
#[test]
fn elimination_stamp_is_once_and_leaves_static_marker() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        let mut manager = TournamentFx::default();
        let stamp = manager.elimination("e", true, true, level, false).unwrap();
        let static_fx = RankingReveal::new(Level::Off, false, false);
        for ms in [0, 75, 150, 166] {
            let mut buf = render(&static_fx, 0, Rect::new(0, 0, 90, 12));
            stamp.render(&mut buf, Rect::new(1, 6, 44, 1), Duration::from_millis(ms));
            insta::assert_snapshot!(format!("tournament_stamp_{level:?}_{ms}"), text(&buf));
            assert!(text(&buf).contains("ELIMINATED"));
            if ms >= 150 {
                assert_eq!(buf, render(&static_fx, 0, Rect::new(0, 0, 90, 12)));
            }
        }
        assert!(stamp.is_done(Duration::from_millis(150)));
    }
}
#[test]
fn champion_snapshots_and_one_trophy_tick_without_debate_tick() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let mut fx = ChampionReveal::new(level, nc, true, 42);
            for ms in [0, 3100, 3650, 5600, 6500] {
                let area = Rect::new(0, 0, 80, 24);
                let mut buf = Buffer::empty(area);
                fx.render(&mut buf, area, &[], Duration::from_millis(ms), &winner());
                insta::assert_snapshot!(
                    format!("tournament_champion_{level:?}_{nc}_{ms}"),
                    text(&buf)
                );
            }
            assert_eq!(
                fx.celebration.take_tick(Duration::from_millis(6500)),
                Some(Tick::Trophy)
            );
            assert_eq!(fx.celebration.take_tick(Duration::from_millis(6516)), None);
        }
    }
}
#[test]
fn champion_skip_preserves_key_and_end_state() {
    let mut fx = ChampionReveal::new(Level::Full, false, true, 42);
    assert_eq!(fx.skip_on_key('x'), 'x');
    let replay = ChampionReveal::new(Level::Off, false, false, 99);
    let area = Rect::new(0, 0, 80, 24);
    let mut a = Buffer::empty(area);
    let mut b = a.clone();
    fx.render(&mut a, area, &[], Duration::ZERO, &winner());
    replay.render(&mut b, area, &[], Duration::ZERO, &winner());
    assert_eq!(a, b);
}
#[test]
fn end_state_equals_off_and_protected_glyphs_survive() {
    let area = Rect::new(0, 0, 90, 12);
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            let fx = RankingReveal::new(level, nc, true);
            assert_eq!(
                render(&fx, 916, area),
                render(&RankingReveal::new(Level::Off, nc, true), 0, area)
            );
            let mut buf = Buffer::empty(area);
            buf[(3, 0)].set_symbol("X");
            fx.render(
                &mut buf,
                area,
                &[Rect::new(3, 0, 1, 1)],
                Duration::from_millis(300),
                &rows(),
            );
            assert_eq!(buf[(3, 0)].symbol(), "X");
            let c = ChampionReveal::new(level, nc, true, 42);
            let mut a = Buffer::empty(area);
            let mut b = a.clone();
            c.render(&mut a, area, &[], Duration::from_millis(6516), &winner());
            ChampionReveal::new(Level::Off, nc, true, 99).render(
                &mut b,
                area,
                &[],
                Duration::ZERO,
                &winner(),
            );
            assert_eq!(a, b);
        }
    }
}
