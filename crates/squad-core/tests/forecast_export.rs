use squad_core::{
    export::{ExportSnapshot, forecast::render},
    orchestrator::forecast::aggregate,
};
use squad_protocol::*;
fn event(id: ContestId, kind: &str, body: serde_json::Value, seq: i64) -> CommittedEnvelope {
    let e = squad_core::router::envelope(
        EventId::new(),
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestArtifact(ContestArtifact {
            contest_id: id,
            step_id: StepId::new(),
            artifact_kind: Text::new(kind).unwrap(),
            kind: Some(ContestKind::Forecast),
            round: Some(0),
            actor: Some(Actor::System),
            audience: Some(ArtifactAudience::Human),
            body: Some(body.clone()),
            data: body,
            work_id: None,
            source_refs: vec![],
        }),
    );
    CommittedEnvelope {
        base: e.base,
        content: e.content,
        commit: CommitFields {
            seq: Cursor::new(seq).unwrap(),
            accepted_at: chrono::Utc::now(),
            commit_index: 0,
            commit_size: 1,
        },
    }
}
#[test]
fn failed_crux_export_preserves_app_number_and_unrounded_inputs() {
    let id = ContestId::new();
    let aggregate = aggregate(&[10, 20, 80, 90]).unwrap();
    let snapshot = ExportSnapshot {
        events: vec![event(
            id,
            "forecast.aggregate",
            serde_json::to_value(&aggregate).unwrap(),
            1,
        )],
        ..Default::default()
    };
    let out = render(&snapshot, id).unwrap();
    assert!(out.contains("Partial run"));
    assert!(out.contains("The room is split."));
    assert!(out.contains("Pooled probability: **50.0%**"));
    assert!(out.contains("sortedLogOdds"));
    assert!(out.contains("trimmedLogOdds"));
    assert!(out.contains("\"k\": 0"));
    assert!(!out.contains("promptProvenance"));
    assert!(render(&snapshot, ContestId::new()).is_err());
}
#[test]
fn export_keeps_delivered_bundle_and_timestamp() {
    let id = ContestId::new();
    let b = forecast::ForecastBundle {
        recipient: "a".parse().unwrap(),
        entries: vec![forecast::ForecastBundleEntry {
            label: "F1".into(),
            reference_class: "[number withheld]".into(),
            factors: vec![],
            rationale: "Original delivered projection".into(),
        }],
    };
    let out = render(
        &ExportSnapshot {
            events: vec![event(
                id,
                "forecast.bundle",
                serde_json::to_value(b).unwrap(),
                1,
            )],
            ..Default::default()
        },
        id,
    )
    .unwrap();
    assert!(out.contains("Original delivered projection"));
    assert!(out.contains("number withheld"));
    assert!(out.contains("forecast.bundle"));
}
