use squad_protocol::*;
use squad_tui::widgets::forge::*;
#[test]
fn committed_view_collapses_drafts_and_ignores_stale_state() {
    let c = ContestId::new();
    let s = ForgeState {
        contest_id: c,
        state_version: 2,
        state: ForgeStatus::Ranking,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut view = View::default();
    view.apply(EventId::new(), &EnvelopePayload::ForgeState(s.clone()));
    let mut stale = s;
    stale.state_version = 1;
    stale.state = ForgeStatus::Drafting;
    view.apply(EventId::new(), &EnvelopePayload::ForgeState(stale));
    assert_eq!(view.state.as_ref().unwrap().state, ForgeStatus::Ranking);
    let a = ContestArtifact {
        contest_id: c,
        step_id: StepId::new(),
        artifact_kind: "forge.draft".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Forge),
        round: Some(0),
        actor: Some(Actor::Member("a".parse().unwrap())),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::json!({"body":"private draft text"})),
    };
    let event = EventId::new();
    view.apply(event, &EnvelopePayload::ContestArtifact(a.clone()));
    view.apply(event, &EnvelopePayload::ContestArtifact(a));
    let lines = render(&view).join("\n");
    for d in [
        "FORGE",
        "DRAFTS",
        "RANKING",
        "FUSION",
        "VERDICT",
        "collapsed",
    ] {
        assert!(lines.contains(d));
    }
    assert!(!lines.contains("private draft text"));
    assert_eq!(view.artifacts.len(), 1);
}

#[test]
fn enable_switch_controls_help_completion_and_parser_checked_tips() {
    let enabled = forge::ENABLED;
    assert_eq!(
        squad_tui::typeahead::MODES
            .iter()
            .any(|(name, _)| *name == "$forge"),
        enabled
    );
    assert_eq!(
        squad_tui::actions::help_lines()
            .join("\n")
            .contains("$forge"),
        enabled
    );
    let context = parser::ParseContext {
        roster: ["a", "b", "c"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let examples = squad_tui::typeahead::examples(&context);
    assert_eq!(examples.iter().any(|text| text.contains("$forge")), enabled);
    for text in examples.iter().filter(|text| text.contains("$forge")) {
        assert!(matches!(
            parse(text, &parser::SubmissionMeta::default(), &context).unwrap(),
            ParsedInput::Forge { .. }
        ));
    }
}
