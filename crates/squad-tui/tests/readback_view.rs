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
#[test]
fn rewrite_pairs_are_human_visible_and_replay_deduplicates() {
    let contest = ContestId::new();
    let state = ReadbackState {
        contest_id: contest,
        state_version: 1,
        state: ReadbackStatus::Rewriting,
        step_id: Some(StepId::new()),
        round: 1,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let mut view = View::default();
    view.apply(
        EventId::new(),
        &EnvelopePayload::ReadbackState(state.clone()),
    );
    let mut artifact = ContestArtifact {
        kind: Some(ContestKind::Readback),
        round: Some(1),
        actor: Some(Actor::Judge),
        audience: Some(ArtifactAudience::Human),
        body: Some(
            serde_json::json!({"spec":"New spec","changes":[{"entryId":"one","before":"Old passage here","after":"New passage here"}]}),
        ),
        contest_id: contest,
        step_id: state.step_id.unwrap(),
        artifact_kind: "readback.rewrite".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
    };
    let id = EventId::new();
    view.apply(id, &EnvelopePayload::ContestArtifact(artifact.clone()));
    view.apply(id, &EnvelopePayload::ContestArtifact(artifact.clone()));
    let lines = render(&view);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.as_str() == "Before: Old passage here")
            .count(),
        1
    );
    assert!(lines.contains(&"After: New passage here".into()));
    artifact.audience = Some(ArtifactAudience::Case);
    view.apply(EventId::new(), &EnvelopePayload::ContestArtifact(artifact));
    assert_eq!(view.artifacts.len(), 1);
}
