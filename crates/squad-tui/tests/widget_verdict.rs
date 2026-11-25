use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_protocol::*;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::verdict::*,
};
use std::collections::BTreeMap;
fn fixture() -> (Verdict, [MemberNick; 2]) {
    let nicks = [
        MemberNick::new("claude").unwrap(),
        MemberNick::new("gemini").unwrap(),
    ];
    let judge = JudgeVerdict {
        winner: nicks[0].clone(),
        scores: nicks
            .iter()
            .map(|nick| JudgeScore {
                nick: nick.clone(),
                criteria: [
                    CriterionId::Fit,
                    CriterionId::Feasibility,
                    CriterionId::Originality,
                    CriterionId::SurvivedAttacks,
                ]
                .into_iter()
                .map(|criterion_id| CriterionScore {
                    criterion_id,
                    score: 8,
                    reasoning: "Concrete and testable.".into(),
                })
                .collect(),
            })
            .collect(),
        reasoning: "Equal weighted totals; the smaller deployment wins.".into(),
        what_to_steal_from_loser: vec!["Keep the rollback checklist.".into()],
        tally_override_reason: Some("The final proposal resolves the earlier weaknesses.".into()),
    };
    let tally = BTreeMap::from([(nicks[0].clone(), 12), (nicks[1].clone(), 16)]);
    (
        squad_protocol::verdict::assemble_verdict(judge, &nicks, &tally).unwrap(),
        nicks,
    )
}
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
fn criteria_totals_winner_and_respect_viewports() {
    let (verdict, nicks) = fixture();
    for (width, height) in [(80, 24), (120, 40), (32, 40)] {
        let mut view = VerdictView::new(&verdict, &nicks)
            .unwrap()
            .with_judge("judge/actual", Some(&nicks[0]));
        view.show_criterion_reasons = true;
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        let rects = render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
        assert_eq!(rects.score_cells.len(), 8);
        assert_eq!(rects.totals.len(), 2);
        assert!(rects.rows.iter().all(|r| r.intersection(area) == *r));
        assert!(buf.content.iter().all(|c| c.fg == Color::Reset));
        insta::assert_snapshot!(format!("verdict_{width}x{height}"), screen(&buf));
    }
}
#[test]
fn protocol_arithmetic_and_exact_conflict_warning() {
    let (mut verdict, nicks) = fixture();
    let view = VerdictView::new(&verdict, &nicks)
        .unwrap()
        .with_judge("judge/replacement", Some(&nicks[1]));
    assert_eq!(
        view.substitution_warning.as_deref(),
        Some("Judge model changed to judge/replacement because @gemini is debating.")
    );
    assert_eq!(view.verdict.verdict.winner, nicks[0]);
    assert_eq!(view.verdict.weighted_totals[0].total, 8.0);
    assert!(
        view.with_tournament_judge("blind/judge")
            .substitution_warning
            .is_none()
    );
    verdict.weighted_totals[0].total = 9.0;
    assert!(VerdictView::new(&verdict, &nicks).is_err());
    verdict.weighted_totals[0].total = 8.0;
    verdict.verdict.tally_override_reason = None;
    assert!(VerdictView::new(&verdict, &nicks).is_err());
}
#[test]
fn scrolling_exposes_all_details_with_effects_off() {
    let (verdict, nicks) = fixture();
    let mut view = VerdictView::new(&verdict, &nicks).unwrap();
    view.show_criterion_reasons = true;
    let area = Rect::new(0, 0, 24, 8);
    let mut buf = Buffer::empty(area);
    let height = render(&view, area, &mut buf, &Palette::new(ColorTier::TrueColor)).content_height;
    let mut seen = String::new();
    for scroll in (0..height).step_by(8) {
        view.scroll = scroll;
        render(&view, area, &mut buf, &Palette::new(ColorTier::TrueColor));
        seen.push_str(&screen(&buf));
    }
    let seen = seen.replace('│', " ");
    let seen = seen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(seen.contains("Concrete and testable."));
    assert!(seen.contains("Keep the rollback"));
}

#[test]
fn compact_verdict_keeps_result_within_fifteen_rows() {
    let (verdict, nicks) = fixture();
    let view = VerdictView::new(&verdict, &nicks)
        .unwrap()
        .with_judge("judge/actual", None);
    let mut buf = Buffer::empty(Rect::new(0, 0, 62, 30));
    let rects = render(&view, buf.area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(rects.content_height <= 15, "{}", rects.content_height);
    assert!(rects.score_cells.is_empty());
    let output = screen(&buf);
    assert!(output.contains("Winner: @claude"));
    assert!(output.contains("claude 8.00 : 8.00 gemini"));
    assert!(output.contains("Tally 12 : 16 · Judge judge/actual"));
    assert!(output.contains("F7 Expand"));
}
