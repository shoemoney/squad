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
