use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::fx::verdict::{Contender, Criterion, Level, Phase, Tick, Verdict, VerdictReveal};
use std::time::Duration;

fn data() -> Verdict {
    Verdict {
        contenders: [
            Contender {
                nick: "claude".into(),
                hue: Color::Rgb(224, 130, 90),
                art: vec![
                    "  ___ _      _   _   _ ___  ___".into(),
                    " / __| |    /_\\ | | | |   \\| __|".into(),
                    " \\___|____/_/ \\_\\___/|___/|___|".into(),
                ],
                compact_art: vec![
                    "▛▀▘ ▌ ▄▀▄ ▌ ▌ ▛▀▄ ▛▀▘".into(),
                    "▙▄▖ ▙▄ ▌ ▌ ▙▄▌ ▙▄▀ ▙▄▖".into(),
                ],
            },
            Contender {
                nick: "gemini".into(),
                hue: Color::Rgb(91, 155, 255),
                art: vec!["GEMINI".into()],
                compact_art: vec!["GEMINI".into()],
            },
        ],
        criteria: std::array::from_fn(|i| Criterion {
            name: ["fit", "feasibility", "originality", "survivedAttacks"][i].into(),
            scores: [[8, 7], [7, 9], [9, 6], [8, 8]][i],
        }),
        totals: [82, 79],
        scale: 10,
        winner: 0,
        round_tally: [1, 1],
        override_reason: Some("Better fit despite equal scored rounds.".into()),
        reasoning: "The focused plan survives the strongest objections.".into(),
        steal: "The staged rollout and rollback criteria.".into(),
    }
}
fn render(fx: &VerdictReveal, t: u64, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    fx.render(&mut buf, area, area, &[], Duration::from_millis(t), &data());
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
fn storyboard_snapshots() {
    for (name, level, nc, width, height) in [
        ("full", Level::Full, false, 120, 40),
        ("lite", Level::Lite, false, 120, 40),
        ("narrow", Level::Full, false, 80, 24),
        ("no_color", Level::Full, true, 80, 24),
    ] {
        let fx = VerdictReveal::new(level, nc, true, 42);
        for ms in [
            0, 600, 900, 1250, 1600, 1950, 2300, 2800, 3100, 4200, 5600, 6500, 6516,
        ] {
            insta::assert_snapshot!(
                format!("verdict_{name}_{ms}"),
                text(&render(&fx, ms, width, height))
            );
        }
    }
}
#[test]
fn phases_and_idle_deadline() {
    let fx = VerdictReveal::new(Level::Full, false, true, 42);
    for (ms, phase) in [
        (0, Phase::Accent),
        (600, Phase::Blackout),
        (900, Phase::Criteria),
        (2300, Phase::Suspense),
        (2800, Phase::FinishTotals),
        (3100, Phase::Winner),
        (4200, Phase::Respect),
        (5600, Phase::Settle),
        (6500, Phase::Done),
    ] {
        assert_eq!(fx.phase(Duration::from_millis(ms)), phase);
    }
    assert_eq!(fx.duration(), Duration::from_millis(6500));
    assert!(fx.is_done(fx.duration()));
}
#[test]
fn finished_equals_static_including_styles() {
    for level in [Level::Full, Level::Lite, Level::Off] {
        for nc in [false, true] {
            for (w, h) in [(120, 40), (80, 24), (40, 20)] {
                let fx = VerdictReveal::new(level, nc, true, 42);
                let off = VerdictReveal::new(Level::Off, nc, true, 99);
                for ms in [6500, 6516, 10000] {
                    assert_eq!(render(&fx, ms, w, h), render(&off, 0, w, h));
                }
            }
        }
    }
}
#[test]
fn equal_round_tally_still_has_a_required_winner_and_override_reason() {
    let out = text(&render(
        &VerdictReveal::new(Level::Off, false, true, 42),
        0,
        120,
        40,
    ));
    assert!(out.contains("WINNER · claude"));
    assert!(out.contains("Round tally  1 : 1"));
    assert!(out.contains("Better fit despite equal scored rounds."));
    assert!(!out.contains("TIE"));
}
#[test]
fn typed_and_escape_keys_survive_skip() {
    for code in [KeyCode::Char('a'), KeyCode::Esc, KeyCode::Enter] {
        let mut fx = VerdictReveal::new(Level::Full, false, true, 42);
        let key = KeyEvent::new(code, KeyModifiers::CONTROL);
        assert_eq!(fx.skip_on_key(key), key);
        assert_eq!(
            render(&fx, 0, 120, 40),
            render(&VerdictReveal::new(Level::Off, false, true, 42), 0, 120, 40)
        );
        assert_eq!(
            fx.celebration.take_tick(Duration::ZERO),
            Some(Tick::DebateRecord)
        );
        assert_eq!(fx.celebration.take_tick(Duration::ZERO), None);
    }
}
#[test]
fn protected_glyphs_and_outside_target_are_untouched() {
    let area = Rect::new(0, 0, 120, 40);
    let protected = Rect::new(4, 4, 20, 4);
    let card = Rect::new(2, 2, 100, 32);
    let fx = VerdictReveal::new(Level::Full, false, true, 42);
    for ms in [0, 600, 900, 2300, 3100, 3500, 4200, 5600, 6500] {
        let mut buf = Buffer::filled(area, ratatui::buffer::Cell::new(" "));
        for y in protected.y..protected.bottom() {
            for x in protected.x..protected.right() {
                buf[(x, y)].set_symbol("X");
            }
        }
        buf[(119, 39)].set_symbol("Z");
        fx.render(
            &mut buf,
            card,
            card,
            &[protected],
            Duration::from_millis(ms),
            &data(),
        );
        for y in protected.y..protected.bottom() {
            for x in protected.x..protected.right() {
                assert_eq!(buf[(x, y)].symbol(), "X");
            }
        }
        assert_eq!(buf[(119, 39)].symbol(), "Z");
    }
}
#[test]
fn seeded_confetti_is_deterministic_and_seed_matters() {
    let a = VerdictReveal::new(Level::Full, false, true, 42);
    let b = VerdictReveal::new(Level::Full, false, true, 42);
    let c = VerdictReveal::new(Level::Full, false, true, 99);
    assert_eq!(render(&a, 3650, 120, 40), render(&b, 3650, 120, 40));
    assert_ne!(render(&a, 3650, 120, 40), render(&c, 3650, 120, 40));
}
#[test]
fn replay_and_resize_settle_without_a_second_record_tick() {
    let mut replay = VerdictReveal::new(Level::Full, false, false, 42);
    assert!(replay.is_done(Duration::ZERO));
    assert_eq!(replay.celebration.take_tick(Duration::ZERO), None);
    let mut live = VerdictReveal::new(Level::Full, false, true, 42);
    assert_eq!(
        live.celebration.take_tick(Duration::from_millis(5599)),
        None
    );
    assert_eq!(
        live.celebration.take_tick(Duration::from_millis(5600)),
        Some(Tick::DebateRecord)
    );
    live.celebration.settle();
    assert_eq!(
        live.celebration.take_tick(Duration::from_millis(6500)),
        None
    );
    assert_eq!(render(&live, 0, 80, 24), render(&replay, 0, 80, 24));
}
#[test]
fn no_color_uses_text_and_modifiers_only() {
    for ms in [0, 600, 2300, 3100, 3500, 4200, 6500] {
        let buf = render(
            &VerdictReveal::new(Level::Full, true, true, 42),
            ms,
            120,
            40,
        );
        assert!(
            buf.content
                .iter()
                .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
        );
    }
}

#[test]
fn equal_totals_preserve_the_judges_chosen_second_contender() {
    let mut verdict = data();
    verdict.winner = 1;
    verdict.totals = [80, 80];
    let area = Rect::new(0, 0, 120, 40);
    let mut buf = Buffer::empty(area);
    VerdictReveal::new(Level::Off, false, true, 42).render(
        &mut buf,
        area,
        area,
        &[],
        Duration::ZERO,
        &verdict,
    );
    assert!(text(&buf).contains("WINNER · gemini"));
    assert!(text(&buf).contains("respect · what to steal from claude"));
}

#[test]
fn blackout_changes_only_style_then_restores_the_background() {
    let stage = Rect::new(0, 0, 120, 40);
    let card = Rect::new(10, 2, 100, 32);
    let fx = VerdictReveal::new(Level::Full, false, true, 42);
    for (ms, expected) in [
        (0, Color::Rgb(200, 160, 100)),
        (600, Color::Rgb(70, 56, 35)),
        (6000, Color::Rgb(200, 160, 100)),
        (6500, Color::Rgb(200, 160, 100)),
    ] {
        let mut buf = Buffer::empty(stage);
        buf[(1, 1)]
            .set_symbol("X")
            .set_fg(Color::Rgb(200, 160, 100));
        fx.render(
            &mut buf,
            stage,
            card,
            &[Rect::new(1, 1, 1, 1)],
            Duration::from_millis(ms),
            &data(),
        );
        assert_eq!(buf[(1, 1)].symbol(), "X");
        assert_eq!(buf[(1, 1)].fg, expected);
    }
}

#[test]
fn ansi_hues_keep_the_sixteen_color_fallback() {
    let mut verdict = data();
    verdict.contenders[0].hue = Color::Yellow;
    verdict.contenders[1].hue = Color::Cyan;
    let area = Rect::new(0, 0, 120, 40);
    for ms in [0, 900, 3100, 3650, 4200, 6500] {
        let mut buf = Buffer::empty(area);
        VerdictReveal::new(Level::Full, false, true, 42).render(
            &mut buf,
            area,
            area,
            &[],
            Duration::from_millis(ms),
            &verdict,
        );
        assert!(buf.content.iter().all(|c| !matches!(c.fg, Color::Rgb(..))));
    }
}
