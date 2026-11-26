use serde_json::json;
use squad_core::{
    banner::{self, Banner},
    export::{self, ExportSnapshot},
};
use squad_protocol::*;
use squad_store::{Store, events};

#[test]
fn four_mode_committed_reports_replay_export_and_banners_without_private_material() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let id = ContestId::new();
    let step = StepId::new();
    let plan = PlanId::new();
    let generation = json!({"messageId":MessageId::new(),"workId":WorkId::new(),"triggerEventId":EventId::new(),"attempt":1,"model":"fixture/judge","finishReason":"stop","usage":null});
    let typed: Vec<EnvelopePayload> = vec![
        EnvelopePayload::CensusReport(squad_core::orchestrator::census::report(id, 1, 1, vec![], vec![], vec![], &[])),
        EnvelopePayload::CrosscheckReport(serde_json::from_value(json!({"contestId":id,"planId":plan,"planVersion":1,"gridVersion":1,"ledger":[],"omitted":[],"residualRisks":[],"failedInventories":[],"failedReviewers":[],"failedAuditors":["member0"],"auditsComplete":false,"auditClean":false,"boundedStop":false})).unwrap()),
        EnvelopePayload::SurveyReport(serde_json::from_value(json!({"contestId":id,"stepId":step,"completed":generation,"map":[],"report":{"consensus":[],"contested":[],"loneInsights":[],"loneDissents":[],"rejectedOutliers":[],"limitations":"SURVEY_REPORT_CANARY"}})).unwrap()),
        EnvelopePayload::ProbeReport(serde_json::from_value(json!({"contestId":id,"stepId":step,"completed":generation,"data":{"decisionTree":{"root":"leaf","nodes":{"leaf":{"kind":"leaf","remaining":[],"nextAction":"PROBE_REPORT_CANARY","reassess":true}}},"predictionMatrix":{},"discardedChecks":[],"inseparableGroups":[],"limitations":["No usable evidence"]}})).unwrap()),
    ];
    for (mut payload, mode) in typed
        .into_iter()
        .zip(["census", "crosscheck", "survey", "probe"])
    {
        let id = ContestId::new();
        match &mut payload {
            EnvelopePayload::CensusReport(r) => r.contest_id = id,
            EnvelopePayload::CrosscheckReport(r) => r.contest_id = id,
            EnvelopePayload::SurveyReport(r) => r.contest_id = id,
            EnvelopePayload::ProbeReport(r) => r.contest_id = id,
            _ => unreachable!(),
        }
        let request = EventId::new();
        store
            .commit(request, "fixture".into(), mode.into(), move |tx, group| {
                tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,?,1,1,'{}',0)", [id.to_string(), mode.to_owned()])?;
                tx.execute("INSERT INTO contest_private_maps VALUES (?,'labels','{\"secret\":\"PRIVATE_MAP_CANARY\"}')", [id.to_string()])?;
                for payload in [
                    EnvelopePayload::ContestArtifact(ContestArtifact {
                        contest_id: id,
                        step_id: step,
                        artifact_kind: Text::new(mode).unwrap(),
                        work_id: None,
                        source_refs: vec![],
                        data: json!({"privateMap":"OPAQUE_ARTIFACT_CANARY"}),
                    }),
                    payload,
                ] {
                    let sender = if matches!(
                        &payload,
                        EnvelopePayload::SurveyReport(_) | EnvelopePayload::ProbeReport(_)
                    ) {
                        Actor::Judge
                    } else {
                        Actor::System
                    };
                    group.push(
                        tx,
                        squad_core::router::envelope(request, sender, vec![Recipient::Me], payload),
                    )?;
                }
                Ok(group.accepted(request))
            })
            .unwrap();
    }
    let committed = store.call(|db| events::page(db, None, 100)).unwrap();
    let expected = export::render(
        &ExportSnapshot {
            events: committed.clone(),
            ..Default::default()
        },
        ExportScope::Room,
        None,
    )
    .unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    let replayed = reopened
        .call(|db| {
            let mut feed = events::FeedReplay::default();
            let mut all = vec![];
            loop {
                let group = feed.catch_up(db, 1).unwrap();
                if group.is_empty() {
                    break;
                }
                assert_eq!(
                    group.len(),
                    2,
                    "small pages retain each artifact/report commit group"
                );
                all.extend(group);
            }
            Ok(all)
        })
        .unwrap();
    assert_eq!(replayed, committed);
    assert!(
        !serde_json::to_string(&replayed)
            .unwrap()
            .contains("PRIVATE_MAP_CANARY")
    );
    let actual = export::render(
        &ExportSnapshot {
            events: replayed.clone(),
            ..Default::default()
        },
        ExportScope::Room,
        None,
    )
    .unwrap();
    assert_eq!(actual, expected);
    for marker in [
        "SURVEY_REPORT_CANARY",
        "PROBE_REPORT_CANARY",
        "CENSUS",
        "Coverage ledger",
    ] {
        assert!(actual.contains(marker), "{marker}: {actual}");
    }
    assert!(!actual.contains("OPAQUE_ARTIFACT_CANARY"));
    assert!(!actual.contains("provisional"));
    let boundaries = |events: &[CommittedEnvelope]| {
        events
            .iter()
            .filter_map(|e| match &e.content {
                EnvelopePayload::CensusReport(_) => Some(Banner::Census),
                EnvelopePayload::CrosscheckReport(_) => Some(Banner::Crosscheck),
                EnvelopePayload::SurveyReport(_) => Some(Banner::Survey),
                EnvelopePayload::ProbeReport(_) => Some(Banner::Probe),
                _ => None,
            })
            .map(|b| banner::render_default(&b.text()))
            .collect::<Vec<_>>()
    };
    assert_eq!(boundaries(&replayed), boundaries(&committed));
    assert_eq!(boundaries(&replayed).len(), 4);
}
