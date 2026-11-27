mod support;
use serde_json::json;
use squad_cli::output::Output;
use squad_protocol::*;
use support::*;

#[test]
fn all_mode_preflights_keep_script_estimates_warnings_and_reused_inputs() {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let mut output = Output {
        json: true,
        stdout: &mut stdout,
        stderr: &mut stderr,
    };
    for kind in [
        ContestKind::Tournament,
        ContestKind::Grill,
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        output
            .preflight(ID.parse().unwrap(), &preflight(kind))
            .unwrap();
    }
    insta::assert_snapshot!(String::from_utf8(stdout).unwrap());
    assert!(stderr.is_empty());
}

#[test]
fn late_round_scores_unscored_markers_and_pick_metadata_keep_shared_shapes() {
    let fixtures = vec![
        event(
            1,
            "debate.round_score",
            json!({"debateId":ID,"caseId":ID,"round":1,"scores":{"alpha":{"score":9,"reason":"Clear"},"beta":{"score":7,"reason":"Slow"}},"tally":{"alpha":1,"beta":0}}),
        ),
        event(
            2,
            "debate.round_score",
            json!({"debateId":ID,"caseId":ID,"round":3,"scores":null,"tally":{"alpha":1,"beta":0},"reason":"unscored"}),
        ),
        event(
            3,
            "contest.pick_requested",
            json!({"contestId":ID,"kind":"tournament","questionMessageId":ID,"eligible":["alpha","beta"],"requiredPicks":2,"banner":"Choose two entrants"}),
        ),
        event(
            4,
            "contest.report",
            json!({"contestId":ID,"stepId":ID,"artifactEventIds":[ID],"skippedWorkIds":[ID],"data":{"skippedMembers":["beta"],"completionReason":"partial","bands":["settled","leans","split","unknown","unasked"],"unexpectedResult":"Return to the root"}}),
        ),
    ];
    for json in [true, false] {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut output = Output {
            json,
            stdout: &mut stdout,
            stderr: &mut stderr,
        };
        for fixture in &fixtures {
            output.event(fixture).unwrap();
        }
        let text = String::from_utf8(stdout).unwrap();
        if json {
            for (line, fixture) in text.lines().zip(&fixtures) {
                let value: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(value["event"], serde_json::to_value(fixture).unwrap());
            }
            insta::assert_snapshot!("shared_json", text);
        } else {
            assert!(text.contains("unscored"));
            assert!(text.contains("skippedMembers"));
            insta::assert_snapshot!("shared_human", text);
        }
    }
}

#[test]
fn ack_does_not_fabricate_a_cursor_or_message_identity() {
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    Output {
        json: true,
        stdout: &mut stdout,
        stderr: &mut stderr,
    }
    .ack(ID.parse().unwrap(), vec![], None)
    .unwrap();
    insta::assert_snapshot!(String::from_utf8(stdout).unwrap(), @r#"{"eventIds":[],"requestId":"01900000-0000-7000-8000-000000000001","type":"ack"}
"#);
}

#[test]
fn human_scores_explicitly_label_late_rounds() {
    let score = event(
        1,
        "debate.round_score",
        json!({"debateId":ID,"caseId":ID,"round":1,"scores":null,"tally":{"alpha":0,"beta":0},"reason":"unscored"}),
    );
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    Output {
        json: false,
        stdout: &mut stdout,
        stderr: &mut stderr,
    }
    .event_with_round(&score, Some(3))
    .unwrap();
    assert!(
        String::from_utf8(stdout)
            .unwrap()
            .starts_with("Round 1 unscored (late):")
    );
}

#[test]
fn topic_mismatch_explains_shared_question_in_human_and_json_errors() {
    for json in [false, true] {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        squad_cli::output::Output {
            json,
            stdout: &mut stdout,
            stderr: &mut stderr,
        }
        .error(
            None,
            squad_protocol::ErrorCode::DEBATE_TOPIC_MISMATCH,
            "Debaters' solutions must answer the same user message.",
            false,
        )
        .unwrap();
        let text = String::from_utf8(if json { stdout } else { stderr }).unwrap();
        assert!(text.contains("Send one shared question to both debaters"));
        assert!(text.contains("wait for both replies"));
    }
}

// ---- M3.P3.L5.T5 / T7: shared-schema state snapshots ----

fn seeded(
    state_rows: &[(&str, serde_json::Value, bool)],
    day: &str,
    max: i64,
) -> serde_json::Value {
    let temp = tempfile::tempdir().unwrap();
    let db = database(temp.path(), vec![]);
    db.execute(
        "INSERT OR REPLACE INTO vars(name,value) VALUES ('ROUNDS',10),('GRILL_ROUNDS',2),('CENSUS_PASSES',2)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO scores(nick,wins,losses,trophies) VALUES ('alpha',3,1,2),('beta',1,3,0)",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO spend_daily VALUES ('2026-11-24',4500000,1500000,2)",
        [],
    )
    .unwrap();
    for (kind, state, active) in state_rows {
        db.execute(
            "INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,?,1,?,?,?)",
            rusqlite::params![
                state["contestId"].as_str().or_else(|| state["debateId"].as_str()).unwrap(),
                kind,
                state["stateVersion"].as_i64().unwrap_or(1),
                state.to_string(),
                active
            ],
        )
        .unwrap();
    }
    let roster = ["alpha".parse().unwrap(), "beta".parse().unwrap()];
    let state = squad_core::state::snapshot(&db, &roster, &Default::default(), day, max).unwrap();
    let mut value = serde_json::to_value(state).unwrap();
    // Presence timestamps are wall-clock observations; everything else is committed state.
    for participant in value["participants"].as_array_mut().unwrap() {
        participant["observedAt"] = json!("<observed>");
    }
    value
}

#[test]
fn daily_spend_and_trophies_snapshot_uses_the_shared_camel_case_schema() {
    let value = seeded(&[], "2026-11-24", 20_000_000);
    insta::assert_snapshot!(
        "state_daily_spend_and_trophies",
        serde_json::to_string_pretty(&value).unwrap()
    );
    // The unknown-cost call count stays visible and is never folded into zero.
    assert_eq!(value["dailySpend"]["unknownCalls"], 2);
    assert_eq!(value["dailySpend"]["hasUnreconciledCharges"], true);
    // A different local day starts from its own ledger row.
    let next = seeded(&[], "2026-11-25", 35_000_000);
    assert_eq!(next["dailySpend"]["spentUsd"], 0.0);
    assert_eq!(next["dailySpend"]["limitUsd"], 35.0);
}

#[test]
fn active_contest_and_pending_pick_snapshots_cover_every_contest_family() {
    let u = |n: u32| format!("01900000-0000-7000-8000-{n:012x}");
    let cases = [
        (
            "tournament",
            json!({"contestId":u(1),"state":"ANSWERING","stateVersion":2,"round":1,"replay":0,"survivors":["alpha","beta"],"eliminated":[],"terminalReason":null}),
        ),
        (
            "grill",
            json!({"contestId":u(2),"state":"QUESTIONING","stateVersion":2,"round":1,"totalRounds":3,"leader":"alpha","grillerOrder":["beta"],"turn":1,"planVersion":1,"questionsAsked":1,"passed":0,"terminalReason":null}),
        ),
        (
            "census",
            json!({"contestId":u(3),"stateVersion":3,"status":"HUNTING","pass":2,"planVersion":1,"stepId":u(30),"artifactEventIds":[u(31)],"unfinishedWorkIds":[],"terminalReason":null}),
        ),
        (
            "crosscheck",
            json!({"contestId":u(4),"planId":u(40),"planVersion":1,"gridVersion":2,"stateVersion":4,"status":"REVIEWING","stepId":u(41),"terminalReason":null}),
        ),
        (
            "survey",
            json!({"contestId":u(5),"state":"VOTING","stateVersion":3,"stepId":u(50),"artifactEventIds":[u(51)],"terminalReason":null}),
        ),
        (
            "probe",
            json!({"contestId":u(6),"state":"PROPOSING","stateVersion":2,"stepId":u(60),"artifactEventIds":[],"skippedWorkIds":[],"skippedMembers":["beta"],"terminalReason":null}),
        ),
        (
            "grill",
            json!({"contestId":u(7),"kind":"grill","state":"AWAITING_PICK","stateVersion":2,"questionMessageId":u(70),"addressed":["alpha","beta"],"answers":[{"nick":"alpha","messageId":u(71),"text":"frozen answer"}],"requiredPicks":1}),
        ),
    ];
    for (index, (kind, state)) in cases.into_iter().enumerate() {
        let value = seeded(&[(kind, state, true)], "2026-11-24", 20_000_000);
        assert!(!value["activeContest"].is_null(), "{kind}");
        let name = if index == 6 {
            "state_pending_pick".to_owned()
        } else {
            format!("state_active_{kind}")
        };
        insta::assert_snapshot!(
            name,
            serde_json::to_string_pretty(&value["activeContest"]).unwrap()
        );
    }
}

#[test]
fn state_json_never_contains_snake_case_wire_names_or_hidden_context() {
    let value = seeded(&[], "2026-11-24", 20_000_000);
    let text = value.to_string();
    for forbidden in [
        "daily_spend",
        "active_contest",
        "pending_preflights",
        "label_map",
        "labelMap",
        "prompt",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    for required in [
        "dailySpend",
        "activeContest",
        "pendingPreflights",
        "scoreboard",
        "trophies",
        "vars",
    ] {
        assert!(value.get(required).is_some(), "{required}");
    }
}

#[test]
fn saved_prd_prints_path_and_emits_export_ready() {
    let saved = event(
        1,
        "contest.artifact",
        json!({
            "contestId":ID,"stepId":ID,"artifactKind":"bass_ackwards_prd.saved",
            "sourceRefs":[],"data":{},"kind":"bass_ackwards_prd","round":0,
            "actor":"system","audience":{"kind":"human"},
            "body":{"filename":"prd-demo.md","path":"/room/exports/prd-demo.md"}
        }),
    );
    for json in [true, false] {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        Output {
            json,
            stdout: &mut stdout,
            stderr: &mut stderr,
        }
        .event(&saved)
        .unwrap();
        let text = String::from_utf8(stdout).unwrap();
        if json {
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap(),
                serde_json::json!({"type":"export.ready","scope":"bass_ackwards_prd","filename":"prd-demo.md","path":"/room/exports/prd-demo.md"})
            );
        } else {
            assert_eq!(text, "PRD saved to /room/exports/prd-demo.md\n");
        }
        assert!(stderr.is_empty());
    }
}
