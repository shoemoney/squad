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
#[test]
fn committed_matrix_marks_contested_cells_and_toggle_selects_leave_out() {
    let c = ContestId::new();
    let s = DecideState {
        contest_id: c,
        state_version: 8,
        state: DecideStatus::Explaining,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut view = View::default();
    view.apply(EventId::new(), &EnvelopePayload::DecideState(s));
    let body = serde_json::json!({"cells":[{"optionId":"a","criterionId":"cost","median":8.0,"contested":true},{"optionId":"b","criterionId":"cost","median":5.0,"contested":false}],"ranking":[{"optionId":"a","total":8.0},{"optionId":"b","total":5.0}],"margin":3.0,"winner":"a","leaveOneOut":[{"criterionId":"cost","ranking":[{"optionId":"b","total":9.0},{"optionId":"a","total":3.0}]}],"flips":[],"robust":false,"memberSupport":3});
    let artifact = ContestArtifact {
        contest_id: c,
        step_id: StepId::new(),
        kind: Some(ContestKind::Decide),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        artifact_kind: "decide.matrix".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: body,
    };
    let event = EventId::new();
    let payload = EnvelopePayload::ContestArtifact(artifact);
    view.apply(event, &payload);
    view.apply(event, &payload);
    assert_eq!(view.artifacts.len(), 1);
    let rendered = render(&view).join("\n");
    assert!(rendered.contains("Winner: a"));
    assert!(rendered.contains("8.00*"));
    view.toggle();
    let toggled = render(&view).join("\n");
    assert!(toggled.contains("b (9.0000) > a (3.0000)"));
    assert!(!toggled.contains("Winner: a"));
}
