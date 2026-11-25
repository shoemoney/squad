mod support;
use serde_json::json;
use squad_cli::room_api::Progress;
use squad_protocol::*;
use support::*;

#[tokio::test]
async fn partial_failure_preserves_successes_and_exact_event_schema() {
    let room = FakeRoom::default();
    room.progress.set(Progress::Failed);
    room.emitted.borrow_mut().extend([
        reply(2, "A useful answer."),
        event(
            3,
            "error",
            json!({"code":"PROVIDER_TIMEOUT","text":"Timed out","retryable":false}),
        ),
    ]);
    let (code, output, errors) = send(&room, &args("@all hello", true, false), None).await;
    assert_eq!(code, 1);
    assert_eq!(room.shutdowns.get(), 1);
    assert!(errors.is_empty());
    let records: Vec<serde_json::Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["type"], "ack");
    assert_eq!(
        records[1]["event"],
        serde_json::to_value(reply(2, "A useful answer.")).unwrap()
    );
    assert_eq!(records[2]["event"]["kind"], "error");
    insta::assert_snapshot!(normalize(&output));
}

#[tokio::test]
async fn human_labels_completed_replies() {
    let room = FakeRoom::default();
    room.emitted.borrow_mut().push(reply(2, "A useful answer."));
    let (code, output, errors) = send(&room, &args("@alpha hello", false, false), None).await;
    assert_eq!(code, 0);
    assert!(errors.is_empty());
    insta::assert_snapshot!(output, @"
    Accepted (cursor 1).
    alpha: A useful answer.
    ");
}

#[tokio::test]
async fn every_non_debate_preflight_precedes_ack_without_altering_shared_payload() {
    for kind in [
        ContestKind::Tournament,
        ContestKind::Grill,
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        let room = FakeRoom::default();
        let preflight = preflight(kind);
        room.results
            .borrow_mut()
            .push_back(RequestResult::PendingConfirmation {
                request_id: ID.parse().unwrap(),
                preflight: preflight.clone(),
                cursor: Cursor::new(1).unwrap(),
            });
        let mut options = args("@all hello", true, false);
        options.yes = true;
        let (code, output, _) = send(&room, &options, None).await;
        assert_eq!(code, 0);
        let records: Vec<serde_json::Value> = output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            records[0]["type"],
            format!(
                "{}.preflight",
                serde_json::to_value(kind).unwrap().as_str().unwrap()
            )
        );
        assert_eq!(records[1]["type"], "ack");
        assert_eq!(records[0]["estimate"]["modelCalls"], 12);
        assert_eq!(records[0]["settings"]["passes"], 3);
        assert!(matches!(
            room.requests.borrow()[1],
            Request::ContestConfirm { .. }
        ));
        assert!(
            !room
                .requests
                .borrow()
                .iter()
                .any(|request| matches!(request, Request::CrosscheckAccept { .. }))
        );
    }
}

#[tokio::test]
async fn noninteractive_preflight_never_confirms_or_admits() {
    let room = FakeRoom::default();
    room.results
        .borrow_mut()
        .push_back(RequestResult::PendingConfirmation {
            request_id: ID.parse().unwrap(),
            preflight: preflight(ContestKind::Census),
            cursor: Cursor::new(1).unwrap(),
        });
    let (code, output, errors) = send(&room, &args("@all hello", true, false), None).await;
    assert_eq!(code, 1);
    assert!(errors.is_empty());
    // The refused plan is cancelled so the --yes re-run is not rejected as busy.
    assert_eq!(room.requests.borrow().len(), 2);
    assert!(matches!(
        room.requests.borrow()[1],
        Request::ContestCancel { .. }
    ));
    assert_eq!(room.shutdowns.get(), 1);
    assert!(output.contains("Confirmation required"));
    assert!(!output.contains("\"type\":\"ack\""));
    insta::assert_snapshot!(normalize(&output));
}

#[tokio::test]
async fn cancel_has_no_fabricated_receipt_or_cursor() {
    let room = FakeRoom::default();
    room.results
        .borrow_mut()
        .push_back(RequestResult::PendingConfirmation {
            request_id: ID.parse().unwrap(),
            preflight: preflight(ContestKind::Crosscheck),
            cursor: Cursor::new(1).unwrap(),
        });
    let (code, output, _) = send(&room, &args("@all hello", true, false), Some(false)).await;
    assert_eq!(code, 0);
    let last: serde_json::Value = serde_json::from_str(output.lines().last().unwrap()).unwrap();
    assert_eq!(last["type"], "contest.cancelled");
    assert!(last.get("cursor").is_none());
    assert!(matches!(
        room.requests.borrow()[1],
        Request::ContestCancel { .. }
    ));
}

#[tokio::test]
async fn replay_paginates_and_ignores_unrelated_events() {
    let room = FakeRoom::default();
    for seq in 2..=1005 {
        room.emitted.borrow_mut().push(reply(seq, "answer"));
    }
    let mut unrelated = reply(1006, "unrelated");
    unrelated.base.correlation_id = EventId::new();
    room.emitted.borrow_mut().push(unrelated);
    let (code, output, _) = send(&room, &args("@all hello", true, false), None).await;
    assert_eq!(code, 0);
    assert_eq!(output.lines().count(), 1005);
    assert!(!output.contains("unrelated"));
    let cursors: Vec<i64> = output
        .lines()
        .skip(1)
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["cursor"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    assert!(cursors.windows(2).all(|pair| pair[0] < pair[1]));
}

#[tokio::test]
async fn automatic_preflight_is_printed_before_durable_acknowledgement() {
    let room = FakeRoom::default();
    let PendingPreflight::Contest(preflight) = preflight(ContestKind::Probe) else {
        unreachable!()
    };
    room.emitted.borrow_mut().push(event(
        2,
        "contest.preflight",
        serde_json::to_value(preflight).unwrap(),
    ));
    let (code, output, _) = send(&room, &args("@all hello", true, true), None).await;
    assert_eq!(code, 0);
    let records: Vec<serde_json::Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records[0]["type"], "probe.preflight");
    assert_eq!(records[1]["type"], "ack");
}

#[tokio::test]
async fn interruption_returns_130_after_cleanup() {
    let room = FakeRoom::default();
    room.progress.set(Progress::Running);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = squad_cli::send::run_until(
        &room,
        &args("@all hello", true, false),
        &mut squad_cli::output::Output {
            json: true,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
        || None,
        async { Ok(()) },
    )
    .await
    .unwrap();
    assert_eq!(code, 130);
    assert_eq!(room.shutdowns.get(), 1);
}

#[tokio::test]
async fn shutdown_failure_propagates_and_output_failure_still_shuts_down() {
    let room = FakeRoom::default();
    room.fail_shutdown.set(true);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let result = squad_cli::send::run(
        &room,
        &args("@all hello", true, true),
        &mut squad_cli::output::Output {
            json: true,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
        || None,
    )
    .await;
    assert_eq!(result.unwrap_err().message, "Shutdown failed.");
    assert_eq!(room.shutdowns.get(), 1);
    let room = FakeRoom::default();
    let mut stdout = std::io::Cursor::new([0u8; 0]);
    let result = squad_cli::send::run(
        &room,
        &args("@all hello", true, true),
        &mut squad_cli::output::Output {
            json: true,
            stdout: &mut stdout,
            stderr: &mut stderr,
        },
        || None,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(room.shutdowns.get(), 1);
}

#[tokio::test]
async fn sequence_quotes_resolve_only_to_stored_identities() {
    let mut room = FakeRoom::default();
    room.context
        .records
        .push(squad_protocol::parser::RecordSummary {
            event_id: ID.parse().unwrap(),
            message_id: Some(ID.parse().unwrap()),
            seq: Cursor::new(7).unwrap(),
            kind: Text::new("message.final").unwrap(),
            author: Actor::Member("alpha".parse().unwrap()),
            recipients: vec![Recipient::Me],
            content: "Existing answer".into(),
            in_reply_to_message_id: Some(ID.parse().unwrap()),
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: vec![],
            state_version: None,
            superseded: false,
            eligible: true,
        });
    let mut options = args("@beta consider this", true, true);
    options.quote = vec![7];
    assert_eq!(send(&room, &options, None).await.0, 0);
    let requests = room.requests.borrow();
    let Request::UserMessage { input, .. } = &requests[0] else {
        panic!("expected human message")
    };
    assert_eq!(input.quote, Some(vec![ID.parse().unwrap()]));
    assert_eq!(input.text.as_str(), "@beta consider this");
}

#[tokio::test]
async fn accounting_updates_coalesce_by_attempt_and_preserve_latest_cost() {
    for verbose in [false, true] {
        let room = FakeRoom::default();
        for seq in 2..=101 {
            room.emitted.borrow_mut().push(event(
                seq,
                "usage.record",
                json!({
                    "workId": ID, "attempt": 1, "target": "alpha", "contextId": "member:alpha",
                    "model": "p/model", "startedAt": "2026-11-24T00:00:00Z",
                    "endedAt": "2026-11-24T00:00:01Z", "outcome": "completed",
                    "usage": {"inputTokens": 10, "outputTokens": 5, "costUsd": seq as f64 / 1000.0},
                    "responseModel": "p/model", "day": "2026-11-24"
                }),
            ));
        }
        let mut options = args("@alpha hello", true, false);
        options.verbose = verbose;
        let (code, output, _) = send(&room, &options, None).await;
        assert_eq!(code, 0);
        let records: Vec<serde_json::Value> = output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            records
                .iter()
                .filter(|v| v["event"]["kind"] == "usage.record")
                .count(),
            if verbose { 100 } else { 1 }
        );
        assert_eq!(records.last().unwrap()["knownCostUsd"], 0.101);
        assert_eq!(records.last().unwrap()["calls"], 1);
    }
}

// ---- M3.P3.L2.T6 / T8 / T9 / T10 / T11 / T12 ----

fn lines(output: &str) -> Vec<serde_json::Value> {
    output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn late_and_unscored_round_scores_render_with_tallies_in_human_and_json() {
    for json in [false, true] {
        let room = FakeRoom::default();
        room.emitted.borrow_mut().extend([
            event(
                2,
                "debate.state",
                serde_json::json!({"debateId":ID,"state":"ROUND_RUNNING","debaters":["alpha","beta"],"round":3,"totalRounds":3,"judgeModel":"j/m","judgeRequested":true,"stateVersion":2,"terminalReason":null,"caseId":ID,"caseIds":[ID]}),
            ),
            event(
                3,
                "debate.round_score",
                serde_json::json!({"debateId":ID,"caseId":ID,"round":1,"scores":{"alpha":{"score":9,"reason":"Clear"},"beta":{"score":7,"reason":"Slow"}},"tally":{"alpha":1,"beta":0}}),
            ),
            event(
                4,
                "debate.round_score",
                serde_json::json!({"debateId":ID,"caseId":ID,"round":2,"scores":null,"tally":{"alpha":1,"beta":0},"reason":"unscored"}),
            ),
        ]);
        let (code, output, _) = send(&room, &args("@all hello", json, false), None).await;
        assert_eq!(code, 0);
        if json {
            let records = lines(&output);
            let scores: Vec<_> = records
                .iter()
                .filter(|r| r["event"]["kind"] == "debate.round_score")
                .collect();
            assert_eq!(scores.len(), 2);
            // Shared schema: the committed payload is carried intact, tallies and null scores included.
            assert_eq!(scores[0]["event"]["payload"]["tally"]["alpha"], 1);
            assert!(scores[1]["event"]["payload"]["scores"].is_null());
            assert_eq!(scores[1]["event"]["payload"]["reason"], "unscored");
        } else {
            assert!(output.contains("Round 1 score (late):"), "{output}");
            assert!(output.contains("Round 2 unscored (late):"), "{output}");
            assert!(output.contains("\"tally\""));
        }
    }
}

#[tokio::test]
async fn tournament_grill_collecting_and_pick_events_render_through_committed_fixtures() {
    let fixtures = [
        (
            "tournament.state",
            serde_json::json!({"contestId":ID,"state":"ANSWERING","stateVersion":1,"round":1,"replay":0,"survivors":["alpha","beta"],"eliminated":[],"terminalReason":null}),
        ),
        (
            "contest.collecting",
            serde_json::json!({"contestId":ID,"kind":"grill","questionMessageId":ID,"question":"Plan the launch?","addressed":["alpha","beta"]}),
        ),
        (
            "contest.pick_requested",
            serde_json::json!({"contestId":ID,"kind":"grill","questionMessageId":ID,"eligible":["alpha","beta"],"requiredPicks":1,"banner":"Pick the leader"}),
        ),
        (
            "contest.picked",
            serde_json::json!({"contestId":ID,"kind":"grill","nicks":["alpha"],"sourceMessageIds":[ID]}),
        ),
    ];
    for json in [false, true] {
        let room = FakeRoom::default();
        for (index, (kind, payload)) in fixtures.iter().enumerate() {
            room.emitted
                .borrow_mut()
                .push(event(index as u32 + 2, kind, payload.clone()));
        }
        let (code, output, _) = send(&room, &args("@all $grill hello", json, false), None).await;
        assert_eq!(code, 0);
        if json {
            let records = lines(&output);
            assert_eq!(records.len(), 5, "ack plus four committed events");
            for (record, (kind, payload)) in records[1..].iter().zip(&fixtures) {
                assert_eq!(record["type"], "event");
                assert_eq!(record["event"]["kind"], *kind);
                assert_eq!(record["event"]["payload"], *payload);
            }
        } else {
            for needle in [
                "[tournament.state]",
                "[contest.collecting]",
                "Plan the launch?",
                "[contest.pick_requested]",
                "Pick the leader",
                "[contest.picked]",
            ] {
                assert!(output.contains(needle), "{needle} missing: {output}");
            }
        }
    }
}

#[tokio::test]
async fn human_preflights_show_kind_specific_calls_costs_remaining_and_warnings() {
    for kind in [
        ContestKind::Tournament,
        ContestKind::Grill,
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        let room = FakeRoom::default();
        room.results
            .borrow_mut()
            .push_back(RequestResult::PendingConfirmation {
                request_id: ID.parse().unwrap(),
                preflight: preflight(kind),
                cursor: Cursor::new(1).unwrap(),
            });
        let (code, output, _) = send(&room, &args("@all hello", false, false), Some(true)).await;
        assert_eq!(code, 0);
        let name = serde_json::to_value(kind).unwrap();
        let first = output.lines().next().unwrap();
        assert_eq!(
            first,
            format!(
                "{} preflight: 12 model calls; estimate $2.50; reserved $2.50; remaining $10.00",
                name.as_str().unwrap()
            )
        );
        assert!(output.contains("Participants: alpha, beta"));
        assert!(output.contains("Warning: beta sits out the final step"));
        assert!(output.contains("\"reusedInputs\""));
    }
}

#[tokio::test]
async fn preserved_admission_errors_and_daily_cap_text_reach_the_caller_unchanged() {
    let room = FakeRoom::default();
    room.results
        .borrow_mut()
        .push_back(RequestResult::Rejected {
            request_id: ID.parse().unwrap(),
            error: ErrorPayload {
                code: ErrorCode::CONTEST_BUSY,
                text: Text::new("A contest is already running").unwrap(),
                retryable: false,
                related_event_id: None,
                work_id: None,
                debate_id: None,
                case_id: None,
                earliest_cursor: None,
                retry_action: None,
                contest_id: None,
                provider_status: None,
                provider_message: None,
            },
        });
    let (code, output, _) = send(&room, &args("@all $survey q", true, false), None).await;
    assert_eq!(code, 1);
    let record = &lines(&output)[0];
    assert_eq!(record["error"]["code"], "CONTEST_BUSY");
    assert_eq!(record["error"]["message"], "A contest is already running");
}

#[tokio::test]
async fn four_mode_steps_reports_and_skipped_members_keep_unchanged_shared_payloads() {
    let report = serde_json::json!({"contestId":ID,"stepId":ID,"artifactEventIds":[ID],"skippedWorkIds":[ID],"data":{"skippedMembers":["beta"],"unexpectedResult":"Return to the root","bands":["settled","leans","split","unknown","unasked"],"estimate":{"low":2,"high":4,"caveat":"Estimates are approximate"}}});
    let step = serde_json::json!({"contestId":ID,"stepId":ID,"ordinal":1,"scriptVersion":"census-v1","moveName":"fanout","inputs":[ID],"assignments":[{"target":"alpha","contextId":"member:alpha","payloadRefs":[ID]}]});
    for json in [true, false] {
        let mut room = FakeRoom::default();
        room.context.roster = ["alpha", "beta", "gamma", "delta"]
            .map(|n| n.parse().unwrap())
            .into();
        room.emitted.borrow_mut().extend([
            event(2, "contest.step", step.clone()),
            event(3, "contest.report", report.clone()),
        ]);
        let (code, output, _) = send(&room, &args("@all $census hello", json, false), None).await;
        assert_eq!(code, 0);
        if json {
            let records = lines(&output);
            assert_eq!(records[1]["event"]["payload"], step);
            assert_eq!(records[2]["event"]["payload"], report);
        } else {
            assert!(output.contains("[contest.step]") && output.contains("census-v1"));
            assert!(
                output.contains("skippedMembers") && output.contains("Estimates are approximate")
            );
        }
    }
}

#[tokio::test]
async fn yes_confirms_each_mode_but_never_accepts_crosscheck_risks_and_cancel_is_kind_specific() {
    for kind in [
        ContestKind::Crosscheck,
        ContestKind::Census,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        let room = FakeRoom::default();
        room.results
            .borrow_mut()
            .push_back(RequestResult::PendingConfirmation {
                request_id: ID.parse().unwrap(),
                preflight: preflight(kind),
                cursor: Cursor::new(1).unwrap(),
            });
        let mut options = args("@all hello", true, false);
        options.yes = true;
        let (code, _, _) = send(&room, &options, None).await;
        assert_eq!(code, 0, "{kind:?}");
        let requests = room.requests.borrow();
        assert!(matches!(
            &requests[0],
            Request::UserMessage {
                preconfirmed: true,
                ..
            }
        ));
        assert!(
            matches!(requests[1], Request::ContestConfirm { .. }),
            "{kind:?}"
        );
        assert!(
            !requests.iter().any(|r| matches!(
                r,
                Request::CrosscheckAccept { .. } | Request::CensusContinue { .. }
            )),
            "--yes never accepts risk or continues a census plan"
        );
    }
    // A declined contest preflight cancels with ContestCancel; a debate preflight with DebateCancel.
    let room = FakeRoom::default();
    room.results
        .borrow_mut()
        .push_back(RequestResult::PendingConfirmation {
            request_id: ID.parse().unwrap(),
            preflight: PendingPreflight::Debate(DebatePreflight {
                judge_only: false,
                debate_id: ID.parse().unwrap(),
                plan_id: ID.parse().unwrap(),
                expires_at: "2026-11-24T00:10:00Z".parse().unwrap(),
                estimate: DebateEstimate {
                    estimated_usd: 1.,
                    reserved_usd: 1.,
                    remaining_usd: 10.,
                    model_calls: 6,
                },
                judge_model: Some("j/m".into()),
                warning: None,
                confirmation_required: true,
                debaters: ["alpha".parse().unwrap(), "beta".parse().unwrap()],
                rounds: 2,
                judge_requested: true,
                opening_round_required: true,
            }),
            cursor: Cursor::new(1).unwrap(),
        });
    let (code, output, _) = send(
        &room,
        &args("@alpha @beta $debate hi", true, false),
        Some(false),
    )
    .await;
    assert_eq!(code, 0);
    assert!(matches!(
        room.requests.borrow()[1],
        Request::DebateCancel { .. }
    ));
    assert_eq!(lines(&output).last().unwrap()["type"], "debate.cancelled");
}

#[tokio::test]
async fn no_wait_for_each_mode_commits_only_and_stops_after_the_acknowledgement() {
    for message in [
        "@all $census hello",
        "@all $crosscheck hello\n--\nplan",
        "@all $survey hello",
        "@all $probe hello",
    ] {
        let mut room = FakeRoom::default();
        room.context.roster = ["alpha", "beta", "gamma", "delta"]
            .map(|n| n.parse().unwrap())
            .into();
        room.progress.set(Progress::Running);
        let (code, output, _) = send(&room, &args(message, true, true), None).await;
        assert_eq!(code, 0, "{message}: {output}");
        assert_eq!(room.commit_only.borrow().len(), 1);
        assert_eq!(
            room.requests.borrow().len(),
            1,
            "one request, no follow-up waiting"
        );
        assert_eq!(lines(&output).len(), 1);
        assert_eq!(lines(&output)[0]["type"], "ack");
    }
}
