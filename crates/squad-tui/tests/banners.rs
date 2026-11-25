use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_core::banner::{Banner, BannerMode};
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::banner::{BannerCache, BannerView, render_art},
};
use unicode_width::UnicodeWidthStr;

const WIDTHS: [u16; 3] = [120, 80, 40];
const MODES: [(BannerMode, &str); 3] = [
    (BannerMode::On, "on"),
    (BannerMode::Compact, "compact"),
    (BannerMode::Off, "off"),
];
fn every_banner() -> Vec<Banner> {
    vec![
        Banner::Opening,
        Banner::Round(2),
        Banner::FinalRound,
        Banner::Verdict,
        Banner::Tournament,
        Banner::TournamentRound {
            round: 1,
            remaining: 8,
        },
        Banner::TournamentRound {
            round: 3,
            remaining: 2,
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
fn new_banners() -> [Banner; 4] {
    [
        Banner::Census,
        Banner::Crosscheck,
        Banner::Survey,
        Banner::Probe,
    ]
}
fn paint(banners: &[Banner], mode: BannerMode, width: u16, palette: &Palette) -> String {
    let mut cache = BannerCache::default();
    let mut out = String::new();
    for kind in banners {
        let art = cache.get(&kind.text(), mode, width);
        let height = art.height().max(1) as u16;
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        let rects = render_art(&art, buf.area, &mut buf, palette);
        assert_eq!(rects.height, art.height());
        out.push_str(&format!(
            "── {kind:?} · {:?} · {} rows\n",
            art.style,
            art.height()
        ));
        for y in 0..height {
            let line = (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>();
            assert!(line.width() <= usize::from(width));
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out
}

#[test]
fn every_banner_snapshots_at_120_80_and_40_columns_in_every_mode_without_colour() {
    let palette = Palette::new(ColorTier::NoColor);
    for width in WIDTHS {
        for (mode, name) in MODES {
            insta::assert_snapshot!(
                format!("banners_all_{name}_{width}"),
                paint(&every_banner(), mode, width, &palette)
            );
        }
    }
}

#[test]
fn four_new_banners_and_step_dividers_snapshot_with_default_and_compact_fonts() {
    let palette = Palette::new(ColorTier::NoColor);
    for width in WIDTHS {
        for (mode, name) in MODES {
            let text = paint(&new_banners(), mode, width, &palette);
            for banner in new_banners() {
                assert!(
                    text.to_uppercase().contains(&banner.text()) || mode != BannerMode::Off,
                    "{banner:?} label kept in {name} at {width}"
                );
            }
            insta::assert_snapshot!(format!("banners_new_{name}_{width}"), text);
        }
        // Step dividers: the narrow one-line fallbacks used between rounds.
        let steps = [
            Banner::Round(1),
            Banner::FinalRound,
            Banner::TournamentRound {
                round: 2,
                remaining: 4,
            },
            Banner::GrillRound(3),
        ];
        insta::assert_snapshot!(
            format!("banners_steps_{width}"),
            paint(&steps, BannerMode::Compact, width, &palette)
        );
    }
}

#[test]
fn forty_columns_never_overflow_and_fall_back_to_one_line_labels() {
    let mut cache = BannerCache::default();
    for kind in every_banner() {
        for (mode, _) in MODES {
            let art = cache.get(&kind.text(), mode, 40);
            assert!(
                art.lines.iter().all(|l| l.width() <= 40),
                "{kind:?} {mode:?}"
            );
            assert!(art.height() >= 1);
        }
        let off = cache.get(&kind.text(), BannerMode::Off, 40);
        assert_eq!(off.height(), 1, "Off keeps just the label for {kind:?}");
    }
}

#[test]
fn no_color_has_no_foreground_and_colour_tiers_do() {
    let kind = Banner::Census;
    for tier in [ColorTier::NoColor, ColorTier::Ansi16, ColorTier::TrueColor] {
        let mut cache = BannerCache::default();
        let art = cache.get(&kind.text(), BannerMode::On, 80);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, art.height() as u16));
        render_art(&art, buf.area, &mut buf, &Palette::new(tier));
        let coloured = buf
            .content
            .iter()
            .any(|c| c.symbol() != " " && c.fg != Color::Reset);
        assert_eq!(coloured, tier != ColorTier::NoColor, "{tier:?}");
    }
}

#[test]
fn replay_hits_the_cache_and_resize_reflows_heights() {
    let mut cache = BannerCache::default();
    let first = cache.get("CENSUS", BannerMode::On, 120);
    let again = cache.get("CENSUS", BannerMode::On, 120);
    assert!(
        std::sync::Arc::ptr_eq(&first, &again),
        "replay reuses the render"
    );
    assert_eq!(cache.len(), 1);
    let view = BannerView::new(Banner::Census, BannerMode::On);
    let wide = view.measure(120, &mut cache);
    let narrow = view.measure(40, &mut cache);
    assert!(narrow <= wide + 6 && narrow >= 1);
    assert_eq!(view.measure(120, &mut cache), wide, "stable after reflow");
    assert_eq!(cache.len(), 2);
}
