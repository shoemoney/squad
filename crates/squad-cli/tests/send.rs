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
