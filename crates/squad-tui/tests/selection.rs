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
