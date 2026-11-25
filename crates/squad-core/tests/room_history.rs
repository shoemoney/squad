#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn backward_pages_are_ascending_exclusive_and_validate_limits() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    assert!(room.page_events(None, 100).await.unwrap().is_empty());
    for value in [3, 4, 5] {
        accepted(room.submit(set(value)).await);
    }
    let all = room.page_events(None, 100).await.unwrap();
    assert_eq!(all.len(), 3);
    let newest = room.page_events(None, 2).await.unwrap();
    assert_eq!(newest, all[1..]);
    assert_eq!(
        room.page_events(Some(newest[0].commit.seq), 2)
            .await
            .unwrap(),
        all[..1]
    );
    assert!(
        room.page_events(Some(all[0].commit.seq), 1)
            .await
            .unwrap()
            .is_empty()
    );
    for limit in [0, 501, usize::MAX] {
        assert_eq!(
            room.page_events(None, limit).await.unwrap_err().code,
            ErrorCode::INVALID_COMMAND
        );
    }
    assert_eq!(
        room.page_events(Some(Cursor::new(999).unwrap()), 1)
            .await
            .unwrap_err()
            .code,
        ErrorCode::CURSOR_INVALID
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn subscribe_before_history_allows_overlap_and_clear_expires_old_cursor() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    let cursor = accepted(room.submit(set(3)).await);
    let FeedItem::Committed(event) = feed.recv().await.unwrap() else {
        panic!()
    };
    assert_eq!(room.page_events(None, 100).await.unwrap()[0], event);
    let clear = accepted(
        room.submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await,
    );
    assert!(clear > cursor);
    assert_eq!(
        room.page_events(Some(cursor), 100).await.unwrap_err().code,
        ErrorCode::CURSOR_EXPIRED
    );
    let events = room.page_events(None, 100).await.unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].content, EnvelopePayload::RoomCleared(_)));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn finalized_events_survive_restart_without_provisional_text() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    accepted(room.submit(message("@a durable", EventId::new())).await);
    let before = room.page_events(None, 100).await.unwrap();
    room.shutdown().await.unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    assert_eq!(room.page_events(None, 100).await.unwrap(), before);
    assert!(matches!(
        room.subscribe().try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn backward_pagination_keeps_whole_commit_groups_even_above_target_limit() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    let oldest = accepted(room.submit(set(3)).await);
    accepted(
        room.submit(Request::SetVars {
            request_id: EventId::new(),
            updates: vec![
                VariableUpdate {
                    name: VariableName::Rounds,
                    value: 4,
                },
                VariableUpdate {
                    name: VariableName::GrillRounds,
                    value: 3,
                },
            ],
        })
        .await,
    );
    let page = room.page_events(None, 1).await.unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].commit.commit_index, 0);
    assert_eq!(page[1].commit.commit_index, 1);
    let older = room.page_events(Some(page[0].commit.seq), 1).await.unwrap();
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].commit.seq, oldest);
    let interior = room.page_events(Some(page[1].commit.seq), 1).await.unwrap();
    assert_eq!(interior, older);
    room.shutdown().await.unwrap();
}
