use squad_protocol::*;
use squad_tui::widgets::readback::*;
#[test]
fn human_step_banner_round_and_replay_are_stable() {
    let s = ReadbackState {
        contest_id: ContestId::new(),
        state_version: 3,
        state: ReadbackStatus::AwaitingHuman,
        step_id: Some(StepId::new()),
        round: 2,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut view = View::default();
    let id = EventId::new();
    view.apply(id, &EnvelopePayload::ReadbackState(s.clone()));
    view.apply(id, &EnvelopePayload::ReadbackState(s.clone()));
    let out = render(&view).join("\n");
    assert!(out.contains("READBACK"));
    assert!(out.contains("ROUND 2"));
    assert!(out.contains("READ IT BACK"));
    assert!(out.contains("1b 2a ... or /answer 3: your reading"));
    let mut old = s;
    old.state_version = 2;
    old.round = 1;
    view.apply(EventId::new(), &EnvelopePayload::ReadbackState(old));
    assert_eq!(render(&view).join("\n"), out);
}
