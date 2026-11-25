use ratatui::{buffer::Buffer, layout::Rect};
use squad_protocol::*;
use squad_tui::{
    fx::{ColorTier, Palette},
    widgets::case::*,
};
fn opened() -> CaseOpened {
    CaseOpened {
        case_id: "01900000-0000-7000-8000-000000000001".parse().unwrap(),
        judge_invocation_id: "01900000-0000-7000-8000-000000000002".parse().unwrap(),
        work_id: None,
        judge_model: Text::new("judge/actual").unwrap(),
        material: CaseMaterial::Standalone {
            messages: vec![ResolvedQuote {
                event_id: "01900000-0000-7000-8000-000000000003".parse().unwrap(),
                author: Actor::Member(MemberNick::new("claude").unwrap()),
                text: "Use a reversible migration.".into(),
            }],
            question: Text::new("Which proposal is safer?").unwrap(),
        },
        debate_id: None,
        contest_id: None,
    }
}
fn ruling(opened: &CaseOpened) -> CaseRuling {
    CaseRuling {
        completed: CompletedGeneration {
            generation: GenerationIdentity {
                message_id: MessageId::new(),
                work_id: WorkId::new(),
                trigger_event_id: EventId::new(),
                attempt: 1,
            },
            model: opened.judge_model.clone(),
            finish_reason: FinishReason::Stop,
            usage: None,
        },
        case_id: opened.case_id,
        judge_invocation_id: opened.judge_invocation_id,
        in_reply_to_event_id: EventId::new(),
        content: Text::new(
            "Prefer the reversible migration. Verify the rollback before proceeding.",
        )
        .unwrap(),
    }
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
fn standalone_ruling_and_followup_viewports() {
    let opened = opened();
    let mut view = CaseView::from(&opened);
    let mut accepted = ruling(&opened);
    accepted.in_reply_to_event_id = "01900000-0000-7000-8000-000000000004".parse().unwrap();
    assert!(view.add_ruling(&accepted));
    view.original_verdict = Some(accepted.in_reply_to_event_id);
    let followup = CaseFollowup {
        case_id: opened.case_id,
        user_event_id: "01900000-0000-7000-8000-000000000005".parse().unwrap(),
        judge_invocation_id: EventId::new(),
        work_id: WorkId::new(),
    };
    assert!(view.add_followup(&followup, "What should the rollback verify?"));
    for (width, height) in [(80, 24), (120, 40), (32, 32)] {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        let rects = render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
        assert_eq!(rects.rulings.len(), 1);
        assert_eq!(rects.followups.len(), 1);
        let text = screen(&buf);
        assert!(!text.contains("Winner:"));
        assert!(!text.contains("/10"));
        insta::assert_snapshot!(format!("case_{width}x{height}"), text);
    }
}
#[test]
fn case_identity_deduplication_and_failure() {
    let opened = opened();
    let mut view = CaseView::from(&opened);
    let mut accepted = ruling(&opened);
    assert!(view.add_ruling(&accepted));
    assert!(view.add_ruling(&accepted));
    assert_eq!(view.entries.len(), 1);
    accepted.case_id = CaseId::new();
    assert!(!view.add_ruling(&accepted));
    let followup = CaseFollowup {
        case_id: opened.case_id,
        user_event_id: EventId::new(),
        judge_invocation_id: EventId::new(),
        work_id: WorkId::new(),
    };
    assert!(view.add_followup(&followup, "Why?"));
    assert!(view.add_followup(&followup, "Why?"));
    assert_eq!(view.entries.len(), 2);
    view.failure = Some("Exact core validation error".into());
    let area = Rect::new(0, 0, 80, 30);
    let mut buf = Buffer::empty(area);
    render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(screen(&buf).contains("Exact core validation error"));
    view.collapsed = true;
    let rects = render(&view, area, &mut buf, &Palette::new(ColorTier::NoColor));
    assert!(rects.rulings.is_empty());
}
