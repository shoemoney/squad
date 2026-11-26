use squad_protocol::MessageId;
use squad_tui::selection::{MessageAction as A, SelectableMessage, SelectionState};
#[test]
fn durable_selection_survives_navigation_and_obeys_distinct_eligibility() {
    let messages = vec![
        SelectableMessage {
            id: MessageId::new(),
            durable: true,
            actions: vec![A::Quote, A::CopyMessage],
        },
        SelectableMessage {
            id: MessageId::new(),
            durable: false,
            actions: vec![A::Regenerate],
        },
    ];
    let mut state = SelectionState::default();
    state.navigate(&messages, 1);
    assert_eq!(state.focused, Some(messages[0].id));
    assert!(state.toggle(&messages[0]));
    state.navigate(&messages, 1);
    assert!(!state.toggle(&messages[1]));
    assert_eq!(state.eligible(&messages, A::Quote), vec![messages[0].id]);
    assert!(state.eligible(&messages, A::Regenerate).is_empty());
    state.navigate(&messages, -1);
    assert!(state.is_selected(messages[0].id));
    state.remove(&[messages[0].id]);
    assert!(state.selected().next().is_none());
    for action in [
        A::Quote,
        A::Regenerate,
        A::DebateSolution,
        A::GrillPlan,
        A::CopyMessage,
        A::CopyCode,
    ] {
        assert!(!action.label().is_empty());
    }
}

mod sources {
    use squad_protocol::MessageId;
    use squad_tui::selection::*;
    fn msg(actions: Vec<MessageAction>) -> (SelectableMessage, MessagePreview) {
        let id = MessageId::new();
        (
            SelectableMessage {
                id,
                durable: true,
                actions,
            },
            MessagePreview {
                id,
                event_id: squad_protocol::EventId::new(),
                author: String::new(),
                text: "plan  text\nmore".into(),
            },
        )
    }
    fn pick(state: &mut SelectionState, m: &SelectableMessage) {
        state.toggle(m);
    }
    #[test]
    fn debate_solutions_need_two_distinct_authors_and_use_reply_to_only() {
        let (a, mut pa) = msg(vec![MessageAction::DebateSolution]);
        let (b, mut pb) = msg(vec![MessageAction::DebateSolution]);
        let (c, mut pc) = msg(vec![MessageAction::Quote]);
        pa.author = "claude".into();
        pb.author = "claude".into();
        pc.author = "grok".into();
        let all = vec![a.clone(), b.clone(), c.clone()];
        let previews = vec![pa.clone(), pb.clone(), pc];
        let mut st = SelectionState::default();
        pick(&mut st, &a);
        let one = DebateSolutionSelection::resolve(&st, &all, &previews, "topic");
        assert_eq!(one.gap, Some(SolutionGap::NeedTwo { selected: 1 }));
        assert!(one.meta().is_none());
        pick(&mut st, &b);
        let same = DebateSolutionSelection::resolve(&st, &all, &previews, "topic");
        assert_eq!(same.gap, Some(SolutionGap::SameAuthor));
        let mut previews = previews;
        previews[1].author = "grok".into();
        let ok = DebateSolutionSelection::resolve(&st, &all, &previews, "a very\n long topic");
        assert!(ok.ready());
        let meta = ok.meta().unwrap();
        assert_eq!(
            meta.reply_to,
            vec![previews[0].event_id, previews[1].event_id]
        );
        assert_ne!(meta.reply_to[0].to_string(), a.id.to_string());
        assert!(meta.quote.is_empty());
        assert!(ok.preview_lines()[1].contains("plan text more"));
        assert!(ok.preview_lines()[0].contains("a very long topic"));
    }
    #[test]
    fn grill_plan_source_is_exactly_one_selected_or_quoted_message() {
        let (a, pa) = msg(vec![MessageAction::GrillPlan]);
        let (b, pb) = msg(vec![MessageAction::GrillPlan]);
        let (c, pc) = msg(vec![MessageAction::Quote]);
        let all = vec![a.clone(), b.clone(), c.clone()];
        let previews = vec![pa, pb, pc];
        let st = SelectionState::default();
        let none = GrillPlanSelection::resolve(&st, &all, &[], &previews);
        assert_eq!(none.gap, Some(PlanSourceGap::Missing));
        let quoted = GrillPlanSelection::resolve(&st, &all, &[a.id], &previews);
        assert!(quoted.ready());
        assert_eq!(quoted.meta().unwrap().quote, vec![previews[0].event_id]);
        assert_ne!(
            quoted.meta().unwrap().quote[0].to_string(),
            a.id.to_string()
        );
        let mut st = SelectionState::default();
        pick(&mut st, &a);
        let dup = GrillPlanSelection::resolve(&st, &all, &[a.id], &previews);
        assert!(
            dup.ready(),
            "selected and quoted same message is one source"
        );
        let two = GrillPlanSelection::resolve(&st, &all, &[b.id], &previews);
        assert_eq!(two.gap, Some(PlanSourceGap::Ambiguous(2)));
        let ineligible =
            GrillPlanSelection::resolve(&SelectionState::default(), &all, &[c.id], &previews);
        assert_eq!(ineligible.gap, Some(PlanSourceGap::Missing));
    }
}
