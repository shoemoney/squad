use squad_protocol::*;
use squad_tui::widgets::decide::*;
#[test]
fn enabled_mode_is_discoverable_and_f7_toggles_its_table() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use squad_tui::{app::App, fx::TerminalCaps, session::ContestView};
    let ctx = parser::ParseContext {
        roster: ["a", "b", "c"].map(|s| s.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    assert!(
        squad_tui::typeahead::MODES
            .iter()
            .any(|(mode, _)| *mode == "$decide")
    );
    assert!(
        parser::help_examples()
            .iter()
            .any(|s| s.contains("$decide"))
    );
    let examples = squad_tui::typeahead::examples(&ctx);
    let example = examples.iter().find(|s| s.contains("$decide")).unwrap();
    parse(example, &Default::default(), &ctx).unwrap();
    assert!(
        (0..examples.len())
            .any(|i| squad_tui::typeahead::tip(&ctx, i as u64 * 8).contains("$decide"))
    );

    let c = ContestId::new();
    let state = DecideState {
        contest_id: c,
        state_version: 1,
        state: DecideStatus::Complete,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut contest = ContestView {
        state: Some(ContestState::Decide(state.clone())),
        ..Default::default()
    };
    contest
        .decide
        .apply(EventId::new(), &EnvelopePayload::DecideState(state));
    let mut app = App::new(TerminalCaps::resolve(|_| None, true, (80, 24).into()).unwrap());
    app.session.contests.insert(c, contest);
    for expected in [true, false] {
        app.dirty = false;
        app.handle(
            Event::Key(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE)),
            std::time::Duration::ZERO,
        );
        assert_eq!(app.session.contests[&c].decide.leave_one_out, expected);
        assert!(app.dirty);
    }
}
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
