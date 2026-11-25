//! M3.P1.L2.T4: stop-all, stop-one, permitted ordinary retry and regeneration through `RoomHandle`.
#[path = "support/m6_room.rs"]
mod live;
#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;

fn rejected_code(result: RequestResult) -> ErrorCode {
    match result {
        RequestResult::Rejected { error, .. } => error.code,
        other => panic!("expected rejection, got {other:?}"),
    }
}

fn statuses(launcher: &Launcher) -> Vec<(String, String)> {
    store(launcher)
        .call(|db| {
            let mut q = db.prepare("SELECT target,status FROM work ORDER BY rowid")?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .unwrap()
}

#[tokio::test]
async fn stop_all_cancels_every_queued_work_item_and_reports_one_receipt() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@all queued", EventId::new()))
            .await,
    );
    assert_eq!(statuses(&launcher).len(), 2);
    let id = EventId::new();
    let result = room
        .submit(Request::Stop {
            request_id: id,
            target: None,
        })
        .await;
    let RequestResult::Accepted {
        request_id,
        event_ids,
        ..
    } = &result
    else {
        panic!("{result:?}")
    };
    assert_eq!(*request_id, id);
    assert_eq!(event_ids.len(), 2, "one cancellation event per work item");
    assert!(statuses(&launcher).iter().all(|(_, s)| s == "cancelled"));
    assert!(
        room.state()
            .await
            .participants
            .iter()
            .all(|p| p.queue_depth == 0)
    );
    // Replaying the same receipt neither errors nor cancels anything again.
    assert_eq!(
        room.submit(Request::Stop {
            request_id: id,
            target: None
        })
        .await,
        result
    );
    // A fresh stop with nothing left is a typed rejection.
    assert_eq!(
        rejected_code(
            room.submit(Request::Stop {
                request_id: EventId::new(),
                target: None
            })
            .await
        ),
        ErrorCode::NOTHING_TO_STOP
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn stop_one_cancels_only_the_named_member() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@all queued", EventId::new()))
            .await,
    );
    accepted(
        room.submit(Request::Stop {
            request_id: EventId::new(),
            target: Some(nick("a")),
        })
        .await,
    );
    let rows = statuses(&launcher);
    assert!(rows.contains(&("a".into(), "cancelled".into())));
    assert!(rows.contains(&("b".into(), "queued".into())));
    let state = room.state().await;
    assert_eq!(state.participants[0].queue_depth, 0);
    assert_eq!(state.participants[1].queue_depth, 1);
    // `a` has nothing left, so stopping it again is rejected while `b` stays queued.
    assert_eq!(
        rejected_code(
            room.submit(Request::Stop {
                request_id: EventId::new(),
                target: Some(nick("a"))
            })
            .await
        ),
        ErrorCode::NOTHING_TO_STOP
    );
    assert!(statuses(&launcher).contains(&("b".into(), "queued".into())));
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn retry_allows_only_failed_ordinary_work_and_allocates_a_new_identity() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a retry", EventId::new()))
            .await,
    );
    let (work, trigger): (WorkId, String) = store(&launcher)
        .call(|db| {
            Ok(
                db.query_row("SELECT work_id,trigger_event_id FROM work", [], |r| {
                    Ok((r.get::<_, String>(0)?.parse().unwrap(), r.get(1)?))
                })?,
            )
        })
        .unwrap();
    // Queued work cannot be retried.
    assert_eq!(
        rejected_code(
            room.submit(Request::Retry {
                request_id: EventId::new(),
                work_id: work
            })
            .await
        ),
        ErrorCode::INVALID_STATE_TRANSITION
    );
    // Unknown work is an invalid reference.
    assert_eq!(
        rejected_code(
            room.submit(Request::Retry {
                request_id: EventId::new(),
                work_id: WorkId::new()
            })
            .await
        ),
        ErrorCode::INVALID_REFERENCE
    );
    store(&launcher)
        .call(|db| Ok(db.execute("UPDATE work SET status='failed'", [])?))
        .unwrap();
    let id = EventId::new();
    let retry = Request::Retry {
        request_id: id,
        work_id: work,
    };
    let result = room.submit(retry.clone()).await;
    accepted(result.clone());
    let rows: Vec<(String, String, String)> = store(&launcher)
        .call(|db| {
            let mut q =
                db.prepare("SELECT work_id,status,trigger_event_id FROM work ORDER BY rowid")?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].1, "queued");
    assert_ne!(rows[1].0, work.to_string());
    assert_eq!(rows[1].2, trigger, "retry keeps the original trigger");
    // Replaying the receipt does not create another retry.
    assert_eq!(room.submit(retry).await, result);
    assert_eq!(statuses(&launcher).len(), 2);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn regeneration_supersedes_the_reply_and_refuses_ineligible_targets() {
    let f = live::Fixture::new().await;
    f.fake
        .completions(vec![
            live::fake_openrouter::streaming("first draft", "stop"),
            live::fake_openrouter::streaming("second draft", "stop"),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    // Nothing to regenerate yet.
    assert_eq!(
        rejected_code(
            room.submit(Request::Regen {
                request_id: EventId::new(),
                message_id: None
            })
            .await
        ),
        ErrorCode::REGEN_NOT_ALLOWED
    );
    room.submit(f.request("@member0 draft")).await;
    f.finals(1).await;
    let first = room
        .events_after(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find_map(|e| match e.content {
            EnvelopePayload::MessageFinal(m) => Some(m),
            _ => None,
        })
        .unwrap();
    let receipt = room
        .submit(Request::Regen {
            request_id: EventId::new(),
            message_id: Some(first.completed.generation.message_id),
        })
        .await;
    accepted(receipt);
    f.finals(2).await;
    let events = room.events_after(None, 500).await.unwrap();
    let marker = events
        .iter()
        .find_map(|e| match &e.content {
            EnvelopePayload::MessageSuperseded(m) => Some(m.clone()),
            _ => None,
        })
        .expect("supersession marker is committed");
    assert_eq!(
        marker.superseded_message_id,
        first.completed.generation.message_id
    );
    assert_eq!(marker.member.as_str(), "member0");
    let replacement = events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::MessageFinal(m) => Some(m),
            _ => None,
        })
        .find(|m| m.completed.generation.message_id == marker.replacement_message_id)
        .expect("replacement final");
    assert_eq!(replacement.content, "second draft");
    assert_eq!(
        replacement.in_reply_to_message_id,
        first.in_reply_to_message_id
    );
    // The superseded message cannot be regenerated twice.
    assert_eq!(
        rejected_code(
            room.submit(Request::Regen {
                request_id: EventId::new(),
                message_id: Some(first.completed.generation.message_id)
            })
            .await
        ),
        ErrorCode::REGEN_NOT_ALLOWED
    );
    room.shutdown().await.unwrap();
}
