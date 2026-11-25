use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_core::banner::{Banner, BannerMode};
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::banner::*,
};
fn screen(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn core_font_fallbacks_and_heights() {
    let mut cache = BannerCache::default();
    for (width, height) in [(80, 24), (120, 40), (24, 12)] {
        let mut buf = Buffer::empty(Rect::new(0, 0, width, height));
        let mut y = 0;
        for (kind, mode) in [
            (Banner::Opening, BannerMode::On),
            (Banner::FinalRound, BannerMode::Compact),
            (Banner::Verdict, BannerMode::Off),
        ] {
            let view = BannerView::new(kind, mode);
            let measured = view.measure(width, &mut cache);
            let art = cache.get(&kind.text(), mode, width);
            let result = render_art(
                &art,
                Rect::new(0, y, width, height - y),
                &mut buf,
                &Palette::new(ColorTier::NoColor),
            );
            assert_eq!(measured, result.height);
            assert!(art.lines.iter().all(|s| !s.contains('\x1b')));
            y += (measured as u16).min(height - y);
        }
        assert!(buf.content.iter().all(|c| c.fg == Color::Reset));
        insta::assert_snapshot!(format!("banners_{width}x{height}"), screen(&buf));
    }
    let n = cache.len();
    cache.get("OPENING", BannerMode::On, 80);
    assert_eq!(cache.len(), n);
    cache.get("OPENING", BannerMode::On, 3);
    assert_eq!(cache.len(), n + 1);
}
#[test]
fn cached_and_uncached_draws_match_and_clip() {
    let view = BannerView::new(Banner::Round(3), BannerMode::On);
    for width in 0..125 {
        let mut a = Buffer::empty(Rect::new(0, 0, 130, 10));
        let mut b = a.clone();
        let area = Rect::new(2, 1, width, 8);
        let palette = Palette::new(ColorTier::Ansi16);
        let r = render(&view, area, &mut a, &palette);
        let mut cache = BannerCache::default();
        let art = cache.get("ROUND 3", BannerMode::On, width);
        assert_eq!(r, render_art(&art, area, &mut b, &palette));
        assert_eq!(a, b);
        assert_eq!(a[(0, 0)].symbol(), " ");
    }
}
