use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::grill::*,
};
fn fixture() -> GrillView {
    let mut v = GrillView::new("grok", vec!["claude".into(), "gemini".into()]);
    v.state = "COMPLETE".into();
    v.reason = Some("survived".into());
    v.focus = "Stress-test the rollout plan.".into();
    v.total_rounds = 3;
    v.current_round = 2;
    v.current_turn = "gemini".into();
    v.questions_asked = 1;
    v.passed = 2;
    v.turns = vec![
        Turn {
            round: 1,
            griller: "claude".into(),
            angle: "rollback".into(),
            status: "complete".into(),
            content: TurnContent::Questions(vec![Exchange {
                question: "What if verification fails?".into(),
                answer: "Revert the release.".into(),
                plan_changes: "Add a rollback checkpoint.".into(),
            }]),
        },
        Turn {
            round: 2,
            griller: "gemini".into(),
            angle: "operability".into(),
            status: "complete".into(),
            content: TurnContent::Pass("No further questions.".into()),
        },
    ];
    v.plans = vec![
        PlanVersion {
            version: 1,
            text: "Original plan".into(),
            expanded: None,
        },
        PlanVersion {
            version: 2,
            text: "Stage, verify, publish. Revert on failure.".into(),
            expanded: None,
        },
    ];
    v.final_plan = Some("Stage, verify, publish. Revert on failure.".into());
    v.appendix_expanded = true;
    v
}
fn screen(v: &GrillView, w: u16, h: u16) -> (String, GrillRects, Buffer) {
    let area = Rect::new(0, 0, w, h);
    let mut b = Buffer::empty(area);
    let r = render(v, area, &mut b, &Palette::new(ColorTier::NoColor));
    let t = (0..h)
        .map(|y| {
            (0..w)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (t, r, b)
}
#[test]
fn snapshots() {
    for (w, h) in [(80, 24), (120, 40), (40, 24)] {
        let (t, _, b) = screen(&fixture(), w, h);
        insta::assert_snapshot!(format!("grill_{w}x{h}"), t);
        assert!(
            b.content
                .iter()
                .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
        );
    }
}
#[test]
fn versions_appendix_exact_pass_and_survived() {
    let (t, r, _) = screen(&fixture(), 120, 40);
    assert!(t.contains("▸ PLAN v1"));
    assert!(!t.contains("Original plan"));
    assert!(t.contains("▾ PLAN v2 · latest"));
    assert!(t.contains("PASS · No further questions."));
    assert_eq!(t.matches("Q: What if verification fails?").count(), 2);
    assert!(!r.survived.is_empty());
    assert_eq!(r.turns.len(), 2);
    assert_eq!(r.plan_badges.len(), 2);
    assert!(!t.contains("CHAMPION"));
}
#[test]
fn leader_failure_keeps_last_good_plan_and_skipped_turn() {
    let mut v = fixture();
    v.state = "FAILED".into();
    v.reason = Some("Leader failed after retry".into());
    v.final_plan = None;
    v.turns[1].content = TurnContent::Skipped("griller failed after retry".into());
    let (t, r, _) = screen(&v, 120, 40);
    assert!(t.contains("SKIPPED · griller failed after retry"));
    assert!(t.contains("Stage, verify, publish"));
    assert!(r.survived.is_empty());
    assert!(r.final_plan.is_empty());
    insta::assert_snapshot!("grill_leader_failure", t);
    v.state = "REBUILDING".into();
    v.reason = None;
    v.scroll = 12;
    let (t, _, _) = screen(&v, 40, 24);
    assert!(t.contains("PLAN v2"));
}

#[test]
fn generic_state_updates_preserve_plan_and_reject_other_identity_or_mode() {
    use squad_protocol::{ContestId, ContestKind, GenericContestState, Text};
    let mut v = fixture();
    let mut s = GenericContestState {
        contest_id: ContestId::new(),
        kind: ContestKind::Grill,
        state_version: 1,
        status: Text::new("REBUILDING").unwrap(),
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    assert!(v.apply_state(&s));
    assert_eq!(v.plans.len(), 2);
    s.contest_id = ContestId::new();
    assert!(!v.apply_state(&s));
    s.contest_id = v.id.unwrap();
    s.kind = ContestKind::Tournament;
    assert!(!v.apply_state(&s));
}
