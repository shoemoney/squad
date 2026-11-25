use ratatui::{buffer::Buffer, layout::Rect, style::Color};
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::pick::*,
};
fn fixture(kind: PickKind) -> PickView {
    let mut v = PickView::new(
        kind,
        "How should we ship safely?",
        vec![
            Candidate {
                nick: "grok".into(),
                eligible: true,
                status: "answered".into(),
                answer: "Verify before publishing.".into(),
            },
            Candidate {
                nick: "gemini".into(),
                eligible: true,
                status: "answered".into(),
                answer: "Keep a tested rollback.".into(),
            },
            Candidate {
                nick: "claude".into(),
                eligible: false,
                status: "stopped".into(),
                answer: String::new(),
            },
        ],
    );
    v.status = PickStatus::Pending;
    v.restored = true;
    v.feedback = Some(v.wrong_pick_feedback());
    v
}
fn screen(v: &PickView, w: u16, h: u16) -> (String, PickRects, Buffer) {
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
    for (kind, name) in [(PickKind::Debate, "debate"), (PickKind::Grill, "grill")] {
        for (w, h) in [(80, 24), (120, 40), (40, 24)] {
            let (t, _, b) = screen(&fixture(kind), w, h);
            insta::assert_snapshot!(format!("pick_{name}_{w}x{h}"), t);
            assert!(
                b.content
                    .iter()
                    .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
            );
        }
    }
}
#[test]
fn exact_feedback_and_pending_survive_rejection() {
    let v = fixture(PickKind::Debate);
    assert_eq!(
        v.wrong_pick_feedback(),
        "Pick exactly two of: @grok, @gemini."
    );
    assert_eq!(
        fixture(PickKind::Grill).wrong_pick_feedback(),
        "Pick exactly one of: @grok, @gemini."
    );
    assert_eq!(
        PickView::ineligible_feedback("claude"),
        "@claude did not answer, so it cannot be picked."
    );
    assert_eq!(
        PickView::insufficient_feedback(1, 3),
        "Not enough answers to start. 1 of 3 members answered."
    );
    assert_eq!(
        v.placeholder(),
        Some("Type two names, e.g. @grok @gemini · Esc to cancel")
    );
    let (t, r, _) = screen(&v, 80, 24);
    assert!(t.contains("pick pending · restored"));
    assert_eq!(r.candidates.len(), 2);
    assert_eq!(r.answers.len(), 2);
}
#[test]
fn acknowledged_cancellation_retains_answers() {
    let mut v = fixture(PickKind::Grill);
    v.status = PickStatus::Cancelled;
    v.feedback = None;
    let (t, r, _) = screen(&v, 80, 24);
    assert!(t.contains("STOPPED · pick_cancelled"));
    assert!(t.contains("Verify before publishing."));
    assert!(t.contains("Keep a tested rollback."));
    assert!(r.candidates.is_empty());
    assert_eq!(r.answers.len(), 2);
    assert_eq!(v.placeholder(), None);
    insta::assert_snapshot!("pick_cancelled", t);
}
#[test]
fn collection_failure_keeps_answers() {
    let mut v = fixture(PickKind::Debate);
    v.status = PickStatus::Collecting;
    let (t, r, _) = screen(&v, 80, 24);
    assert!(t.contains("COLLECTING_ANSWERS"));
    insta::assert_snapshot!("pick_collecting", t);
    assert!(r.candidates.is_empty());
    v.status = PickStatus::Failed;
    v.feedback = Some(PickView::insufficient_feedback(1, 3));
    let (t, _, _) = screen(&v, 80, 24);
    assert!(t.contains("Not enough answers to start. 1 of 3 members answered."));
    insta::assert_snapshot!("pick_failed", t);
}

#[test]
fn restore_from_protocol_preserves_order_answers_and_authoritative_eligibility() {
    use squad_protocol::{
        ContestAnswer, ContestId, ContestKind, ContestSelectionState, ContestSelectionStatus,
        MemberNick, MessageId,
    };
    let nick = |n: &str| MemberNick::new(n).unwrap();
    let mut state = ContestSelectionState {
        contest_id: ContestId::new(),
        kind: ContestKind::Grill,
        state: ContestSelectionStatus::AwaitingPick,
        state_version: 7,
        question_message_id: MessageId::new(),
        addressed: vec![nick("grok"), nick("gemini")],
        // Completion order differs from roster order, and only gemini is eligible.
        answers: vec![
            ContestAnswer {
                nick: nick("gemini"),
                message_id: MessageId::new(),
                text: "Rollback".into(),
            },
            ContestAnswer {
                nick: nick("grok"),
                message_id: MessageId::new(),
                text: "Verify".into(),
            },
        ],
        required_picks: 1,
    };
    let eligible = vec!["gemini".into()];
    let v = PickView::from_selection(&state, &eligible, "Ship?").unwrap();
    assert!(v.restored);
    assert_eq!(v.status, PickStatus::Pending);
    assert_eq!(v.candidates[0].nick, "grok");
    assert!(!v.candidates[0].eligible);
    assert!(v.candidates[1].eligible);
    assert_eq!(v.candidates[0].answer, "Verify");
    state.state = ContestSelectionStatus::Stopped;
    let v = PickView::from_selection(&state, &eligible, "Ship?").unwrap();
    assert_eq!(v.status, PickStatus::Cancelled);
    assert_eq!(v.candidates[1].answer, "Rollback");
    state.kind = ContestKind::Tournament;
    assert!(PickView::from_selection(&state, &eligible, "Ship?").is_none());
}
