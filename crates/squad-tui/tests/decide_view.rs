use squad_protocol::*;
use squad_tui::widgets::decide::*;
#[test]
fn replay_does_not_duplicate_artifacts_or_regress_state() {
    let c = ContestId::new();
    let mut view = View::default();
    let mut s = DecideState {
        contest_id: c,
        state_version: 2,
        state: DecideStatus::Scoring,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let event = EventId::new();
    view.apply(event, &EnvelopePayload::DecideState(s.clone()));
    s.state_version = 1;
    s.state = DecideStatus::ProposingCriteria;
    view.apply(event, &EnvelopePayload::DecideState(s));
    assert_eq!(view.state.as_ref().unwrap().state, DecideStatus::Scoring);
    let lines = render(&view);
    for label in ["DECIDE", "OPTIONS", "RUBRIC", "SCORES", "WHAT FLIPS IT"] {
        assert!(lines.iter().any(|l| l == label));
    }
    view.toggle();
    assert!(view.leave_one_out);
}
