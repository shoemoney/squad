use squad_core::{
    export::{self, ExportSnapshot},
    orchestrator::census,
};
use squad_protocol::*;
fn text(s: &str) -> Text {
    Text::new(s).unwrap()
}
#[test]
fn committed_census_report_and_scope() {
    let id: ContestId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    let gap = CensusGapOutcome {
        gap: CensusGap {
            id: text("g1"),
            omission: text("No rollback plan"),
            consequence: text("A bad release may cause an outage"),
            lenses: vec![text("failure modes")],
            source_find_ids: vec![text("find1")],
        },
        disposition: CensusDisposition::Confirmed,
        votes: 3,
        covered: 1,
        trivial: 0,
        finders: vec!["member0".parse().unwrap()],
        finder_count: 1,
        priority: Some(text("Prevent a prolonged outage")),
        citations: vec![CensusCitation {
            location: text("Recovery section"),
            quote: text("Restore from backup"),
        }],
    };
    let report = census::report(id, 1, 1, vec![gap], vec![], vec![], &[]);
    report.validate().unwrap();
    let state = CensusState {
        contest_id: id,
        state_version: 3,
        status: CensusStatus::Complete,
        pass: 1,
        plan_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        unfinished_work_ids: vec![],
        terminal_reason: None,
    };
    let dir = tempfile::tempdir().unwrap();
    let store = squad_store::Store::open(dir.path().join("room.db")).unwrap();
    let request = EventId::new();
    let outcome = store
        .commit(
            request,
            "fixture".into(),
            "census-export".into(),
            move |tx, group| {
                for payload in [
                    EnvelopePayload::CensusState(state),
                    EnvelopePayload::CensusReport(report),
                ] {
                    group.push(
                        tx,
                        squad_core::router::envelope(
                            request,
                            Actor::System,
                            vec![Recipient::Me],
                            payload,
                        ),
                    )?;
                }
                Ok(group.accepted(request))
            },
        )
        .unwrap();
    let snapshot = ExportSnapshot {
        events: outcome.events,
        ..Default::default()
    };
    let output = export::render(&snapshot, ExportScope::Census, Some(id)).unwrap();
    assert_eq!(
        export::render(&snapshot, ExportScope::Debate, Some(id)),
        Err(export::ExportError::ScopeMismatch)
    );
    insta::assert_snapshot!(output);
}
