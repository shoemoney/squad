use squad_protocol::*;
use squad_tui::widgets::rank::{View, render};
#[test]
fn rank_progress_replay_and_state_projection() {
    let mut v = View::default();
    let contest = ContestId::new();
    let state = RankState {
        contest_id: contest,
        state_version: 2,
        state: RankStatus::Comparing,
        step_id: Some(StepId::new()),
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    v.apply(EventId::new(), &EnvelopePayload::RankState(state));
    let id = EventId::new();
    let a = ContestArtifact {
        contest_id: contest,
        step_id: StepId::new(),
        artifact_kind: "rank.compare".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Rank),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::json!({"choices":[{"pairId":"P1","winner":"A","reason":"Impact"}]})),
    };
    v.apply(id, &EnvelopePayload::ContestArtifact(a.clone()));
    v.apply(id, &EnvelopePayload::ContestArtifact(a));
    let text = render(&v).join("\n");
    assert!(text.contains("RANK"));
    assert!(text.contains("COMPARING 1 judgments returned"));
    assert_eq!(v.artifacts.len(), 1);
}
