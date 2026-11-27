use squad_core::export::{ExportSnapshot, negotiate::render};
use squad_protocol::*;
#[test]
fn missing_contest_is_not_an_empty_report() {
    assert!(matches!(
        render(&ExportSnapshot::default(), ContestId::new()),
        Err(squad_core::export::ExportError::ContestNotFound)
    ));
}
#[test]
fn partial_stopped_export_keeps_committed_transcripts() {
    let c = ContestId::new();
    let step = StepId::new();
    let mut e = CommittedEnvelope {
        base: EnvelopeBase {
            version: 1,
            id: EventId::new(),
            room_id: MainRoom::Main,
            correlation_id: EventId::new(),
            causation_id: None,
            sender: Actor::System,
            recipients: vec![Recipient::Me],
            created_at: chrono::Utc::now(),
        },
        commit: CommitFields {
            seq: Cursor::new(1).unwrap(),
            accepted_at: chrono::Utc::now(),
            commit_index: 0,
            commit_size: 1,
        },
        content: EnvelopePayload::NegotiateState(NegotiateState {
            contest_id: c,
            state_version: 1,
            state: NegotiateStatus::Stopped,
            step_id: Some(step),
            round: 1,
            artifact_event_ids: vec![],
            terminal_reason: Some("stopped".parse().unwrap()),
        }),
    };
    e.content = EnvelopePayload::ContestArtifact(ContestArtifact {
        kind: Some(ContestKind::Negotiate),
        round: Some(1),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(
            serde_json::json!({"table":2,"round":1,"actor":"b","counterpart":true,"body":{"message":"Walk away","response":"walk_away","counterOffer":null,"violatedInterest":"price"}}),
        ),
        contest_id: c,
        step_id: step,
        artifact_kind: "negotiate.turn".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
    });
    let text = render(
        &ExportSnapshot {
            events: vec![e],
            ..Default::default()
        },
        c,
    )
    .unwrap();
    assert!(text.contains("Walk away"));
    assert!(text.contains("round 1"));
    assert!(text.contains("\"table\": 2"));
    assert!(!text.contains("COMPLETE"));
}
