use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::BudgetStateEvent;
use squad_tui::{
    fx::{ColorTier, Palette, meter::Heat},
    widgets::budget::{DailySpendView, render},
};
fn event() -> BudgetStateEvent {
    BudgetStateEvent {
        spent_usd: 3.42,
        reserved_usd: 1.5,
        limit_usd: 20.,
        estimated_usd: 0.,
        unresolved_attempts: 1,
        day: "2026-11-24".into(),
        unknown_calls: 1,
    }
}
fn screen(w: u16, h: u16) -> String {
    let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
    let view = DailySpendView::from(&event());
    let rects = render(&view, buf.area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(rects.labels.intersection(rects.meter).is_empty());
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn budget_snapshots() {
    insta::assert_snapshot!("budget_80x24", screen(80, 24));
    insta::assert_snapshot!("budget_120x40", screen(120, 40));
    insta::assert_snapshot!("budget_narrow", screen(32, 3));
    insta::assert_snapshot!("budget_status", screen(80, 1));
}
#[test]
fn snapshots_replace_reservations_and_recompute_heat() {
    let mut e = event();
    let view = DailySpendView::from(&e);
    assert_eq!(view.remaining(), Some(15_080_000));
    assert!(view.labels().starts_with("~$3.42"));
    assert_eq!(view, DailySpendView::from(&e));
    e.spent_usd = 18.;
    e.reserved_usd = 0.;
    e.unknown_calls = 0;
    assert_eq!(DailySpendView::from(&e).heat(), Some(Heat::Hot));
    e.limit_usd = 40.;
    assert_eq!(DailySpendView::from(&e).heat(), Some(Heat::Normal));
    e.day = "2026-11-25".into();
    e.spent_usd = 0.;
    assert_eq!(DailySpendView::from(&e).actual, Some(0));
    e.spent_usd = f64::NAN;
    assert!(DailySpendView::from(&e).labels().contains("unavailable"));
}

#[test]
fn heat_uses_actual_and_refusal_retains_exact_contract_and_inflight_notice() {
    let mut e = event();
    e.unknown_calls = 0;
    e.limit_usd = 20.;
    e.spent_usd = 14.99;
    e.reserved_usd = 10.;
    assert_eq!(DailySpendView::from(&e).heat(), Some(Heat::Normal));
    for (actual, heat) in [(15., Heat::Warm), (18., Heat::Hot), (20., Heat::Max)] {
        e.spent_usd = actual;
        assert_eq!(DailySpendView::from(&e).heat(), Some(heat));
    }
    let view = DailySpendView::from(&e);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 5));
    let rects = render(&view, buf.area, &mut buf, &Palette::new(ColorTier::NoColor));
    let text: String = (rects.refusal.y..rects.refusal.bottom())
        .flat_map(|y| (0..80).map(move |x| (x, y)))
        .map(|p| buf[p].symbol())
        .collect();
    assert!(text.contains("DAILY_MAX_REACHED"));
    assert!(text.contains("Daily max of $20.00 reached."));
    assert!(text.contains("SQUAD_DAILY_MAX_USD"));
    assert!(text.contains("In-flight calls continue."));
    let previous = view.spend().unwrap();
    e.day = "2026-11-25".into();
    assert_ne!(previous.day, DailySpendView::from(&e).spend().unwrap().day);
}
