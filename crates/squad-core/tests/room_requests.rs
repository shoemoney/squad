#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn receipt_identity_and_conflicts_pass_through_shared_router() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let id = EventId::new();
    let request = message("@all -b $ROUNDS=4 hello", id);
    let result = room.submit(request.clone()).await;
    let RequestResult::Accepted {
        request_id,
        event_ids,
        cursor,
    } = &result
    else {
        panic!("{result:?}")
    };
    assert_eq!(*request_id, id);
    assert_eq!(event_ids.len(), 2);
    assert!(cursor.is_some());
    let wakes = launcher.wakes.lock().unwrap().clone();
    assert_eq!(wakes.len(), 1);
    assert_eq!(room.submit(request).await, result);
    assert_eq!(*launcher.wakes.lock().unwrap(), wakes);
    assert!(
        matches!(room.submit(message("@a changed",id)).await,RequestResult::Rejected {error,..} if error.code==ErrorCode::IDEMPOTENCY_CONFLICT)
    );
    assert_eq!(room.state().await.vars.rounds, 4);
    let json = serde_json::to_value(result).unwrap().to_string();
    assert!(json.contains("requestId"));
    assert!(!json.contains("commandId"));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn restarted_one_shot_receipt_replay_does_not_wake_prior_work() {
    let root = tempfile::tempdir().unwrap();
    let first = Launcher::default();
    let (room, _) = open(root.path(), &first, OpenMode::OneShot).await;
    let request = message("@a hello", EventId::new());
    let result = room.submit_commit_only(request.clone()).await;
    accepted(result.clone());
    room.shutdown().await.unwrap();
    let next = Launcher::default();
    let (room, _) = open(root.path(), &next, OpenMode::OneShot).await;
    assert_eq!(room.submit(request).await, result);
    assert!(next.wakes.lock().unwrap().is_empty());
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn one_shot_retry_wakes_new_work_with_the_original_trigger() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a retry me", EventId::new()))
            .await,
    );
    let original = store(&launcher)
        .call(|db| {
            let id: String = db.query_row("SELECT work_id FROM work", [], |r| r.get(0))?;
            db.execute("UPDATE work SET status='failed'", [])?;
            Ok(id.parse::<WorkId>().unwrap())
        })
        .unwrap();
    accepted(
        room.submit(Request::Retry {
            request_id: EventId::new(),
            work_id: original,
        })
        .await,
    );
    let wakes = launcher.wakes.lock().unwrap().clone();
    assert_eq!(wakes.len(), 1);
    assert_ne!(wakes[0], original);
    room.shutdown().await.unwrap();
}
