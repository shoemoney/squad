use ratatui::style::{Color, Modifier};
use squad_tui::fx::{palette::*, *};
#[test]
fn canonical_members_and_default_body_in_every_tier() {
    for tier in [
        ColorTier::TrueColor,
        ColorTier::Indexed256,
        ColorTier::Ansi16,
        ColorTier::NoColor,
    ] {
        let palette = Palette::new(tier);
        assert_eq!(palette.body(), Color::Reset);
        assert_eq!(palette.member("me", 0), Color::Reset);
        for (slot, nick) in MEMBERS.iter().enumerate() {
            let expected = match tier {
                ColorTier::TrueColor => SIGNATURES[slot],
                ColorTier::Indexed256 => cube256(SIGNATURES[slot]),
                ColorTier::Ansi16 => ANSI_MEMBERS[slot],
                ColorTier::NoColor => Color::Reset,
            };
            assert_eq!(palette.member(nick, 11), expected);
        }
    }
}
#[test]
fn tier_palette_snapshot() {
    let snapshot = [
        ColorTier::TrueColor,
        ColorTier::Indexed256,
        ColorTier::Ansi16,
        ColorTier::NoColor,
    ]
    .into_iter()
    .map(|tier| {
        let p = Palette::new(tier);
        format!(
            "{tier:?}: {:?}; chrome={:?}; body={:?}",
            MEMBERS.map(|nick| p.member(nick, 0)),
            p.chrome(),
            p.body()
        )
    })
    .collect::<Vec<_>>()
    .join("\n");
    insta::assert_snapshot!(snapshot, @r"
TrueColor: [Rgb(224, 130, 90), Rgb(167, 139, 250), Rgb(25, 195, 154), Rgb(91, 155, 255), Rgb(230, 233, 239), Rgb(255, 79, 163)]; chrome=Rgb(255, 210, 63); body=Reset
Indexed256: [Indexed(173), Indexed(141), Indexed(36), Indexed(69), Indexed(189), Indexed(205)]; chrome=Indexed(221); body=Reset
Ansi16: [Yellow, Magenta, Green, Cyan, Blue, LightMagenta]; chrome=Yellow; body=Reset
NoColor: [Reset, Reset, Reset, Reset, Reset, Reset]; chrome=Reset; body=Reset
");
}
#[test]
fn rgb_round_trip_and_short_hue_arc() {
    for color in SIGNATURES {
        if let Color::Rgb(r, g, b) = color {
            let Color::Rgb(r2, g2, b2) = Oklch::from_rgb(r, g, b).color() else {
                panic!()
            };
            for (a, b) in [(r, r2), (g, g2), (b, b2)] {
                assert!((i16::from(a) - i16::from(b)).abs() <= 1);
            }
        }
    }
    let mix = Oklch {
        l: 0.74,
        c: 0.15,
        h: 350.,
    }
    .mix(
        Oklch {
            l: 0.74,
            c: 0.15,
            h: 10.,
        },
        0.5,
    );
    assert!(mix.h < 0.001 || mix.h > 359.999);
    assert_eq!(mix.l, 0.74);
    for t in [0., 0.25, 0.5, 0.75, 1.] {
        assert_eq!(interpolate(SIGNATURES[0], SIGNATURES[0], t), SIGNATURES[0]);
    }
}
#[test]
fn extra_slots_are_stable_distinct_and_perceptually_balanced() {
    let p = Palette::new(ColorTier::TrueColor);
    let again = Palette::new(ColorTier::TrueColor);
    let mut colors = Vec::new();
    for slot in 6..12 {
        let c = p.member("extra", slot);
        assert_eq!(c, again.member("extra", slot));
        assert!(!colors.contains(&c));
        colors.push(c);
        if let Color::Rgb(r, g, b) = c {
            let lch = Oklch::from_rgb(r, g, b);
            assert!((lch.l - 0.74).abs() < 0.003);
            assert!(lch.c <= 0.155);
        }
    }
    let p = Palette::new(ColorTier::Ansi16);
    let ansi: Vec<_> = (0..12).map(|slot| p.member("extra", slot)).collect();
    for (i, c) in ansi.iter().enumerate() {
        assert!(!ansi[..i].contains(c));
    }
}
#[test]
fn cube_mapping_and_reduced_gradients() {
    assert_eq!(cube256(Color::Rgb(0, 0, 0)), Color::Indexed(16));
    assert_eq!(cube256(Color::Rgb(255, 255, 255)), Color::Indexed(231));
    let p = Palette::new(ColorTier::Ansi16);
    let start = p.gradient(Color::Yellow, Color::Cyan, 0.25);
    let end = p.gradient(Color::Yellow, Color::Cyan, 0.75);
    assert_eq!(start.fg, Some(Color::Yellow));
    assert!(start.add_modifier.contains(Modifier::DIM));
    assert_eq!(end.fg, Some(Color::Cyan));
    assert!(end.add_modifier.contains(Modifier::BOLD));
    let nc = Palette::new(ColorTier::NoColor).gradient(Color::Yellow, Color::Cyan, 0.75);
    assert_eq!(nc.fg, None);
}
