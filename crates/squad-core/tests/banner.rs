use squad_core::banner::{Banner, BannerMode, BannerStyle, render, render_default};

fn boundaries() -> Vec<Banner> {
    vec![
        Banner::Opening,
        Banner::Round(3),
        Banner::FinalRound,
        Banner::Verdict,
        Banner::Tournament,
        Banner::TournamentRound {
            round: 2,
            remaining: 4,
        },
        Banner::Final,
        Banner::Champion,
        Banner::Grill,
        Banner::GrillRound(2),
        Banner::FinalPlan,
        Banner::Survived,
        Banner::ChooseDebate,
        Banner::ChooseGrill,
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ]
}

#[test]
fn default_font_boundaries() {
    let output = boundaries()
        .into_iter()
        .map(|banner| {
            let text = banner.text();
            format!("{text}\n{}", render_default(&text))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    insta::assert_snapshot!(output);
}

#[test]
fn compact_font_boundaries() {
    let output = boundaries()
        .into_iter()
        .map(|banner| {
            let text = banner.text();
            let output = render(&text, 160, BannerMode::Compact);
            assert_eq!(output.style, BannerStyle::Compact, "{text}");
            format!("{text}\n{}", output.lines.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    insta::assert_snapshot!(output);
}

#[test]
fn width_fallbacks_and_disabled_labels() {
    let text = Banner::FinalRound.text();
    let mut output = String::new();
    for mode in [BannerMode::On, BannerMode::Compact, BannerMode::Off] {
        for width in [80, 56, 44, 24, 16, 11, 5, 1, 0] {
            let banner = render(&text, width, mode);
            assert!(banner.width() <= width);
            assert_eq!(banner.label, text);
            assert_eq!(banner.height(), banner.lines.len());
            output.push_str(&format!(
                "{mode:?}, width {width}, {:?}\n{}\n\n",
                banner.style,
                banner.lines.join("\n")
            ));
        }
    }
    insta::assert_snapshot!(output);
}

#[test]
fn font_selection_uses_actual_measured_width() {
    let text = "ROUND 3";
    let default_width = render_default(text).lines().map(str::len).max().unwrap();
    assert_eq!(
        render(text, default_width, BannerMode::On).style,
        BannerStyle::Default
    );
    assert_eq!(
        render(text, default_width - 1, BannerMode::On).style,
        BannerStyle::Compact
    );
    assert_eq!(render(text, 10, BannerMode::On).style, BannerStyle::Divider);
    assert_eq!(render(text, 80, BannerMode::Off).style, BannerStyle::Label);
}

#[test]
fn boundaries_have_authoritative_labels() {
    assert_eq!(Banner::debate_round(0, 1), Banner::Opening);
    assert_eq!(Banner::debate_round(1, 1), Banner::FinalRound);
    assert_eq!(Banner::debate_round(1, 3), Banner::Round(1));
    assert_eq!(Banner::debate_round(3, 3), Banner::FinalRound);
    assert_eq!(
        Banner::TournamentRound {
            round: 3,
            remaining: 2
        }
        .text(),
        "FINAL"
    );
    assert_eq!(
        Banner::TournamentRound {
            round: 2,
            remaining: 4
        }
        .text(),
        "ROUND 2 · 4 REMAIN"
    );
}

#[test]
fn export_is_fixed_while_viewport_and_modes_change() {
    // No process environment is mutated: these tests can safely run alongside
    // config tests. Export has no width, mode, effects, or colour inputs.
    for banner in boundaries() {
        let text = banner.text();
        let expected = render_default(&text);
        assert!(
            expected.lines().count() > 1,
            "{text} must render all glyphs"
        );
        assert!(!expected.contains('\x1b'));
        assert!(expected.lines().all(|line| !line.ends_with(' ')));
        for mode in [BannerMode::On, BannerMode::Compact, BannerMode::Off] {
            for width in [0, 1, 10, 40, 80, 200] {
                let viewport = render(&text, width, mode);
                assert!(viewport.width() <= width);
                assert_eq!(viewport.label, text);
                assert_eq!(render_default(&text), expected);
            }
        }
    }
}

#[test]
fn unsupported_glyphs_and_empty_labels_are_not_lost() {
    assert_eq!(render_default("PLAN 界"), "PLAN 界");
    let narrow = render("PLAN 界", 7, BannerMode::On);
    assert_eq!(narrow.label, "PLAN 界");
    assert!(narrow.width() <= 7);
    assert_eq!(render_default("\x1b\nOPENING\t"), render_default("OPENING"));
    assert!(render("  ", 80, BannerMode::On).lines.is_empty());
    assert_eq!(render_default(""), "");
}

#[test]
fn export_ignores_terminal_and_presentation_environment() {
    let executable = std::env::current_exe().unwrap();
    for (columns, banners, effects, no_color) in [
        ("1", "off", "off", "1"),
        ("24", "compact", "lite", ""),
        ("240", "on", "full", "0"),
    ] {
        let output = std::process::Command::new(&executable)
            .args(["--exact", "default_font_boundaries"])
            .env("COLUMNS", columns)
            .env("SQUAD_BANNERS", banners)
            .env("SQUAD_FX", effects)
            .env("NO_COLOR", no_color)
            .env("INSTA_UPDATE", "no")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
