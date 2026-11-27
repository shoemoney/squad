use squad_protocol::*;
use squad_tui::widgets::debugging::*;
#[test]
fn committed_state_is_idempotent_and_pending_shape_is_visible() {
    let id = ContestId::new();
    let state = DebuggingState {
        contest_id: id,
        state_version: 3,
        state: DebuggingStatus::AwaitingHuman,
        step_id: Some(StepId::new()),
        round: 1,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut v = View::default();
    let event = EventId::new();
    v.apply(event, &EnvelopePayload::DebuggingState(state.clone()));
    v.apply(event, &EnvelopePayload::DebuggingState(state.clone()));
    let mut stale = state;
    stale.state_version = 2;
    stale.state = DebuggingStatus::Selecting;
    v.apply(EventId::new(), &EnvelopePayload::DebuggingState(stale));
    assert_eq!(v.state.as_ref().unwrap().state_version, 3);
    let lines = render(&v).join("\n");
    assert!(lines.contains("DEBUG"));
    assert!(lines.contains("RUN THE TEST"));
    assert!(lines.contains("/answer O1 <what you saw>"));
}

#[test]
fn discovery_uses_the_enabled_switch() {
    assert!(squad_tui::typeahead::enabled("$debug"));
    assert_eq!(
        squad_tui::typeahead::describe("$debug", &[]),
        "Narrow a failure with tests you run"
    );
    assert!(
        squad_protocol::parser::help_examples()
            .iter()
            .any(|s| s.contains("$debug"))
    );
}
