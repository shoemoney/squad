use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use squad_core::banner::{Banner, BannerMode};
use squad_tui::fx::banner::{
    BannerCache, Boundaries, ColourTier, Entrance, Level, Palette, Phase, accent_step,
};
use std::{fmt::Write, sync::Arc, time::Duration};

const TIERS: [ColourTier; 4] = [
    ColourTier::TrueColor,
    ColourTier::Indexed256,
    ColourTier::Ansi16,
    ColourTier::NoColor,
];
const TIMES: [u64; 7] = [0, 120, 280, 600, 900, 1600, 1900];

fn frame(entrance: &Entrance, width: u16, height: u16, ms: u64) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
    buf.set_string(2, 0, "transcript above", ratatui::style::Style::default());
    buf.set_string(
        2,
        12,
        "protected reply stays put",
        ratatui::style::Style::default(),
    );
    entrance.render(
        &mut buf,
        Rect::new(0, 2, width, entrance.art.height() as u16),
        Duration::from_millis(ms),
        &[Rect::new(0, 12, width, 1)],
    );
    buf
}
// Full glyph rows plus colour/modifier runs on nonblank cells. This captures
// the actual buffer's colour tiers without thousands of redundant blank cells.
fn snapshot(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..14.min(buf.area.height) {
        let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        writeln!(out, "{y:02} |{}|", row.trim_end()).unwrap();
        let mut last = None;
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            if cell.symbol() == " " {
                continue;
            }
            let style = (cell.fg, cell.bg, cell.modifier);
            if last != Some(style) {
                write!(out, " {x}:{:?}/{:?}/{:?}", cell.fg, cell.bg, cell.modifier).unwrap();
                last = Some(style);
            }
        }
        out.push('\n');
    }
    out
}
#[test]
fn storyboard_snapshots() {
    let mut cache = BannerCache::default();
    for kind in [Banner::Round(3), Banner::FinalRound, Banner::Opening] {
        for (w, h) in [(120, 40), (80, 24)] {
            for tier in TIERS {
                let entrance = Entrance::new(
                    &mut cache,
                    kind,
                    BannerMode::On,
                    w,
                    Level::Full,
                    Palette {
                        tier,
                        ..Palette::default()
                    },
                );
                let mut frames = String::new();
                for ms in TIMES {
                    writeln!(
                        frames,
                        "t={ms} ms\n{}",
                        snapshot(&frame(&entrance, w, h, ms))
                    )
                    .unwrap();
                }
                insta::assert_snapshot!(
                    format!(
                        "fx_banner_{kind:?}_{w}_{}",
                        match tier {
                            ColourTier::TrueColor => "TrueColor",
                            ColourTier::Indexed256 => "Indexed",
                            ColourTier::Ansi16 => "Ansi",
                            ColourTier::NoColor => "NoColor",
                        }
                    ),
                    frames
                );
            }
        }
    }
}
#[test]
fn four_mode_snapshots() {
    let mut cache = BannerCache::default();
    for kind in [
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ] {
        for (w, h) in [(120, 40), (80, 24)] {
            let mut frames = String::new();
            for level in [Level::Full, Level::Lite, Level::Off] {
                for tier in TIERS {
                    let entrance = Entrance::new(
                        &mut cache,
                        kind,
                        BannerMode::On,
                        w,
                        level,
                        Palette {
                            tier,
                            ..Palette::default()
                        },
                    );
                    for ms in [400, 1900] {
                        let mut buf = frame(&entrance, w, h, ms);
                        buf.set_string(2, 10, "PASS 1 · HUNT", ratatui::style::Style::default());
                        accent_step(
                            &mut buf,
                            Rect::new(2, 10, 20, 1),
                            Duration::from_millis(if ms == 400 { 60 } else { 120 }),
                            level,
                            entrance.palette,
                        );
                        writeln!(frames, "{level:?}/{tier:?} t={ms}\n{}", snapshot(&buf)).unwrap();
                    }
                }
            }
            insta::assert_snapshot!(format!("fx_banner_mode_{kind:?}_{w}"), frames);
        }
    }
}
#[test]
fn cache_keys_and_width_fallbacks() {
    let mut cache = BannerCache::default();
    let a = cache.get("ROUND 3", BannerMode::On, 120);
    assert!(Arc::ptr_eq(&a, &cache.get("ROUND 3", BannerMode::On, 120)));
    cache.get("ROUND 3", BannerMode::Compact, 120);
    cache.get("ROUND 3", BannerMode::On, 80);
    cache.get("OPENING", BannerMode::On, 120);
    assert_eq!(cache.len(), 4);
    for width in [0, 1, 20, 40, 59, 80, 120] {
        let entrance = Entrance::new(
            &mut cache,
            Banner::Round(3),
            BannerMode::On,
            width,
            Level::Full,
            Palette::default(),
        );
        assert!(entrance.art.width() <= usize::from(width));
        if width < 60 {
            assert!(entrance.art.height() <= 1);
        }
    }
}
#[test]
fn all_final_frames_equal_off_and_protect_message_glyphs() {
    let mut cache = BannerCache::default();
    for kind in [
        Banner::Opening,
        Banner::Round(3),
        Banner::FinalRound,
        Banner::Verdict,
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ] {
        for tier in TIERS {
            for width in [40, 80, 120] {
                let off = Entrance::new(
                    &mut cache,
                    kind,
                    BannerMode::On,
                    width,
                    Level::Off,
                    Palette {
                        tier,
                        ..Palette::default()
                    },
                );
                for level in [Level::Full, Level::Lite, Level::Off] {
                    let mut entrance = Entrance::new(
                        &mut cache,
                        kind,
                        BannerMode::On,
                        width,
                        level,
                        Palette {
                            tier,
                            ..Palette::default()
                        },
                    );
                    assert_eq!(frame(&entrance, width, 24, 1900), frame(&off, width, 24, 0));
                    assert_eq!(frame(&entrance, width, 24, 1917), frame(&off, width, 24, 0));
                    for ms in TIMES {
                        let mut buf = Buffer::filled(
                            Rect::new(0, 0, width, 24),
                            ratatui::buffer::Cell::new("x"),
                        );
                        let before = buf.clone();
                        entrance.render(
                            &mut buf,
                            Rect::new(0, 2, width, 10),
                            Duration::from_millis(ms),
                            &[Rect::new(1, 3, width - 2, 2)],
                        );
                        for y in 3..5 {
                            for x in 1..width - 1 {
                                assert_eq!(buf[(x, y)], before[(x, y)]);
                            }
                        }
                        assert_eq!(buf[(0, 1)], before[(0, 1)]);
                    }
                    entrance.settle();
                    assert_eq!(frame(&entrance, width, 24, 120), frame(&off, width, 24, 0));
                    assert!(entrance.is_done(Duration::ZERO));
                }
            }
        }
    }
}
#[test]
fn judging_is_the_only_verdict_boundary_and_replay_never_animates() {
    let mut cache = BannerCache::default();
    let mut boundaries = Boundaries::default();
    assert!(boundaries.entering_judging(
        "contest-a",
        &mut cache,
        120,
        Level::Full,
        Palette::default(),
        true
    ));
    assert!(!boundaries.entering_judging(
        "contest-a",
        &mut cache,
        120,
        Level::Full,
        Palette::default(),
        true
    ));
    assert_eq!(
        boundaries
            .get("contest-a:verdict")
            .unwrap()
            .phase(Duration::ZERO),
        Phase::Dim
    );
    boundaries.verdict_arrived("contest-a");
    assert_eq!(
        boundaries
            .get("contest-a:verdict")
            .unwrap()
            .phase(Duration::ZERO),
        Phase::Static
    );
    boundaries.verdict_arrived("never-judged");
    assert!(boundaries.get("never-judged:verdict").is_none());
    for kind in [
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ] {
        let key = format!("{kind:?}");
        assert!(boundaries.insert(
            &key,
            Entrance::new(
                &mut cache,
                kind,
                BannerMode::On,
                80,
                Level::Full,
                Palette::default()
            ),
            false
        ));
        assert!(!boundaries.insert(
            &key,
            Entrance::new(
                &mut cache,
                kind,
                BannerMode::On,
                80,
                Level::Full,
                Palette::default()
            ),
            true
        ));
        assert!(boundaries.get(&key).unwrap().is_done(Duration::ZERO));
    }
}
#[test]
fn disabled_banners_no_colour_and_step_labels_remain_static() {
    let mut cache = BannerCache::default();
    for kind in [
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ] {
        for tier in TIERS {
            let mut entrance = Entrance::new(
                &mut cache,
                kind,
                BannerMode::Off,
                80,
                Level::Full,
                Palette {
                    tier,
                    ..Palette::default()
                },
            );
            assert!(entrance.is_done(Duration::ZERO));
            assert_eq!(entrance.art.height(), 1);
            assert!(entrance.art.lines[0].contains(&kind.text()));
            entrance.settle();
            assert_eq!(frame(&entrance, 80, 24, 0), frame(&entrance, 80, 24, 1900));
        }
    }
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
    buf.set_string(0, 0, "BLIND VOTE", ratatui::style::Style::default());
    let before = buf.clone();
    accent_step(
        &mut buf,
        Rect::new(0, 0, 80, 1),
        Duration::from_millis(60),
        Level::Full,
        Palette::default(),
    );
    for x in 0..80 {
        assert_eq!(buf[(x, 0)].symbol(), before[(x, 0)].symbol());
    }
    assert!(buf[(0, 0)].modifier.contains(Modifier::BOLD));
    for tier in TIERS {
        let entrance = Entrance::new(
            &mut cache,
            Banner::Opening,
            BannerMode::On,
            80,
            Level::Full,
            Palette {
                tier,
                ..Palette::default()
            },
        );
        let buf = frame(&entrance, 80, 24, 600);
        for cell in &buf.content {
            match tier {
                ColourTier::NoColor => assert_eq!(cell.fg, Color::Reset),
                ColourTier::Ansi16 => {
                    assert!(!matches!(cell.fg, Color::Rgb(..) | Color::Indexed(..)))
                }
                ColourTier::Indexed256 => assert!(!matches!(cell.fg, Color::Rgb(..))),
                _ => {}
            }
        }
    }
}

#[test]
fn live_modes_deduplicate_settle_on_output_and_resize_without_replay() {
    let mut cache = BannerCache::default();
    let mut boundaries = Boundaries::default();
    for kind in [
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ] {
        let key = kind.text();
        assert!(boundaries.insert(
            &key,
            Entrance::new(
                &mut cache,
                kind,
                BannerMode::On,
                120,
                Level::Full,
                Palette::default()
            ),
            true
        ));
        assert!(!boundaries.insert(
            &key,
            Entrance::new(
                &mut cache,
                kind,
                BannerMode::On,
                120,
                Level::Full,
                Palette::default()
            ),
            true
        ));
        assert!(!boundaries.get(&key).unwrap().is_done(Duration::ZERO));
        boundaries.output_started(&key);
        let entry = boundaries.get(&key).unwrap();
        let off = Entrance::new(
            &mut cache,
            kind,
            BannerMode::On,
            120,
            Level::Off,
            Palette::default(),
        );
        assert_eq!(frame(entry, 120, 40, 120), frame(&off, 120, 40, 0));
        let mut resized = Entrance::new(
            &mut cache,
            kind,
            BannerMode::On,
            120,
            Level::Full,
            Palette::default(),
        );
        resized.resize(&mut cache, 80, BannerMode::On);
        let narrow = Entrance::new(
            &mut cache,
            kind,
            BannerMode::On,
            80,
            Level::Off,
            Palette::default(),
        );
        assert!(resized.is_done(Duration::ZERO));
        assert_eq!(frame(&resized, 80, 24, 0), frame(&narrow, 80, 24, 0));
    }
}
