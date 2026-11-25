#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn live_and_offline_export_share_the_snapshot_while_lock_is_held() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    accepted(
        room.submit(message("@a exported text", EventId::new()))
            .await,
    );
    let live = room.export(ExportScope::Room, None).await.unwrap();
    assert!(live.contains("exported text"));
    let offline = squad_core::room::export_offline(
        &config(root.path()).data_dir,
        "main",
        ExportScope::Room,
        None,
    )
    .unwrap();
    assert_eq!(offline, live);
    assert_eq!(
        room.export(ExportScope::Debate, None)
            .await
            .unwrap_err()
            .code,
        ErrorCode::INVALID_COMMAND
    );
    room.shutdown().await.unwrap();
    assert_eq!(
        squad_core::room::export_offline(
            &config(root.path()).data_dir,
            "main",
            ExportScope::Room,
            None
        )
        .unwrap(),
        live
    );
}

// ---- M3.P3.L3.T19: live and offline exports of the four new modes ----

fn census_fixture(id: ContestId, complete: bool) -> Vec<Fixture> {
    let gap = CensusGapOutcome {
        gap: CensusGap {
            id: Text::new("g1").unwrap(),
            omission: Text::new("No rollback plan").unwrap(),
            consequence: Text::new("A bad release may cause an outage").unwrap(),
            lenses: vec![Text::new("failure modes").unwrap()],
            source_find_ids: vec![Text::new("find1").unwrap()],
        },
        disposition: CensusDisposition::Confirmed,
        votes: 3,
        covered: 1,
        trivial: 0,
        finders: vec![nick("a")],
        finder_count: 1,
        priority: Some(Text::new("Prevent a prolonged outage").unwrap()),
        citations: vec![CensusCitation {
            location: Text::new("Recovery section").unwrap(),
            quote: Text::new("Restore from backup").unwrap(),
        }],
    };
    let state = CensusState {
        contest_id: id,
        state_version: 3,
        status: if complete {
            CensusStatus::Complete
        } else {
            CensusStatus::Stopped
        },
        pass: 1,
        plan_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        unfinished_work_ids: vec![],
        terminal_reason: (!complete).then(|| Text::new("stopped").unwrap()),
    };
    let mut items = vec![(
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::CensusState(state),
    )];
    if complete {
        let report =
            squad_core::orchestrator::census::report(id, 1, 1, vec![gap], vec![], vec![], &[]);
        items.push((
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::CensusReport(report),
        ));
    }
    items
}

fn survey_fixture(id: ContestId, complete: bool) -> Vec<Fixture> {
    use serde_json::json;
    let contest = id.to_string();
    let mut items = vec![system(
        "survey.setup",
        json!({"contestId":contest,"questionMessageId":uuid(70),"question":"Is the proposal feasible?","participants":["a","b"],"answers":[{"nick":"a","messageId":uuid(71),"text":"It fits the capacity budget."},{"nick":"b","messageId":uuid(72),"text":"A load test is needed."}],"collect":false}),
    )];
    items.push(system(
        "survey.state",
        json!({"contestId":contest,"state":if complete {"COMPLETE"} else {"STOPPED"},"stateVersion":9,"stepId":null,"artifactEventIds":[],"terminalReason":if complete {serde_json::Value::Null} else {json!("Stopped")}}),
    ));
    items
}

fn probe_fixture(id: ContestId) -> Vec<Fixture> {
    use serde_json::json;
    let contest = id.to_string();
    vec![
        system(
            "probe.setup",
            json!({"contestId":contest,"brief":{"observation":"Noise only during charging","ruledOut":"Cooling fan","inspectable":"External indicator only"},"participants":["a","b","c","d"],"explainers":["a","b"],"checkers":["c","d"],"sittingOut":null}),
        ),
        system(
            "probe.state",
            json!({"contestId":contest,"state":"STOPPED","stateVersion":3,"stepId":null,"artifactEventIds":[],"skippedWorkIds":[],"skippedMembers":["d"],"terminalReason":"Stopped"}),
        ),
    ]
}

fn crosscheck_fixture(id: ContestId) -> Vec<Fixture> {
    use serde_json::json;
    let contest = id.to_string();
    vec![
        system(
            "crosscheck.setup",
            json!({"contestId":contest,"planId":uuid(81),"planVersion":1,"plan":"Maintain service with the existing staff.","participants":["a","b","c"],"goal":null}),
        ),
        system(
            "crosscheck.state",
            json!({"contestId":contest,"planId":uuid(81),"planVersion":1,"gridVersion":0,"stateVersion":2,"status":"STOPPED","stepId":null,"terminalReason":"stopped"}),
        ),
    ]
}

#[tokio::test]
async fn four_mode_exports_are_identical_live_and_offline_with_partials_and_no_provider_calls() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, server) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let census: ContestId = uuid(900).parse().unwrap();
    let partial_census: ContestId = uuid(901).parse().unwrap();
    let survey: ContestId = uuid(902).parse().unwrap();
    let partial_survey: ContestId = uuid(903).parse().unwrap();
    let probe: ContestId = uuid(904).parse().unwrap();
    let crosscheck: ContestId = uuid(905).parse().unwrap();
    let mut items = Vec::new();
    items.extend(census_fixture(census, true));
    items.extend(census_fixture(partial_census, false));
    items.extend(survey_fixture(survey, true));
    items.extend(survey_fixture(partial_survey, false));
    items.extend(probe_fixture(probe));
    items.extend(crosscheck_fixture(crosscheck));
    // A private artifact that must never appear in any export.
    items.push(system(
        "contest.artifact",
        serde_json::json!({"contestId":probe.to_string(),"stepId":uuid(80),"artifactKind":"private","sourceRefs":[],"data":{"prompt":"SECRET-PROMPT","labelMap":"SECRET-LABEL-MAP"}}),
    ));
    commit_each(&launcher, items);
    let calls = server.received_requests().await.unwrap().len();
    let data_dir = config(root.path()).data_dir;
    let cases = [
        (ExportScope::Census, census),
        (ExportScope::Census, partial_census),
        (ExportScope::Survey, survey),
        (ExportScope::Survey, partial_survey),
        (ExportScope::Probe, probe),
        (ExportScope::Crosscheck, crosscheck),
    ];
    let mut live_exports = Vec::new();
    for (scope, id) in cases.clone() {
        let live = room.export(scope.clone(), Some(id)).await.unwrap();
        let offline =
            squad_core::room::export_offline(&data_dir, "main", scope.clone(), Some(id)).unwrap();
        assert_eq!(live, offline, "{scope:?} {id}");
        assert!(!live.contains("SECRET"), "{scope:?}");
        // Deterministic: a second render is byte-identical.
        assert_eq!(room.export(scope.clone(), Some(id)).await.unwrap(), live);
        // The wrong scope never matches the contest.
        assert!(
            room.export(ExportScope::Tournament, Some(id))
                .await
                .is_err(),
            "{scope:?} exported under the wrong scope"
        );
        live_exports.push(live);
    }
    // A complete run carries its report; the stopped run discloses that it is partial.
    assert!(live_exports[0].contains("No rollback plan"));
    assert!(!live_exports[1].contains("No rollback plan"));
    assert!(live_exports[2].contains("Is the proposal feasible?"));
    assert_ne!(live_exports[2], live_exports[3]);
    // Required identifiers and mismatches are typed errors with the stable code.
    for scope in [
        ExportScope::Census,
        ExportScope::Crosscheck,
        ExportScope::Survey,
        ExportScope::Probe,
    ] {
        assert_eq!(
            room.export(scope, None).await.unwrap_err().code,
            ErrorCode::INVALID_COMMAND
        );
    }
    assert!(
        room.export(ExportScope::Census, Some(ContestId::new()))
            .await
            .is_err()
    );
    // The room export is still readable with those events present, and exporting is read-only.
    let room_export = room.export(ExportScope::Room, None).await.unwrap();
    assert!(!room_export.contains("SECRET"));
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        calls,
        "exports never call the provider"
    );
    room.shutdown().await.unwrap();
    // After shutdown the offline reader still renders the identical text.
    for ((scope, id), live) in cases.into_iter().zip(&live_exports) {
        assert_eq!(
            &squad_core::room::export_offline(&data_dir, "main", scope, Some(id)).unwrap(),
            live
        );
    }
}
