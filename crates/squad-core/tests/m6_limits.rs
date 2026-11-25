#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use support::*;

#[tokio::test]
async fn queue_ceiling_bounds_durable_work_and_keeps_input_available_for_export() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    for _ in 0..constants::AGENT_QUEUE_DEPTH + 1 {
        assert!(matches!(
            room.submit_commit_only(f.request("@member0 queue boundary"))
                .await,
            RequestResult::Accepted { .. }
        ));
    }
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work"),
        constants::AGENT_QUEUE_DEPTH as i64
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='user.message'"),
        constants::AGENT_QUEUE_DEPTH as i64 + 1
    );
    assert!(room.events_after(None,500).await.unwrap().iter().any(|e|matches!(&e.content,EnvelopePayload::Error(error) if error.code==ErrorCode::AGENT_QUEUE_FULL)));
    assert!(
        room.export(ExportScope::Room, None)
            .await
            .unwrap()
            .contains("queue boundary")
    );
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn locked_room_reports_exact_owner_while_readonly_export_remains_available() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    room.submit_commit_only(f.request("@member0 retained under lock"))
        .await;
    let client = squad_agents::openrouter::OpenRouterClient::new(&f.fake.base_url(), KEY).unwrap();
    let error = squad_core::room::RoomHandle::open_with_client(
        f.config.clone(),
        "main",
        OpenMode::Interactive,
        client,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        error.message,
        format!(
            "Room main is already open in another squad process (pid {}).",
            std::process::id()
        )
    );
    let export =
        squad_core::room::export_offline(&f.config.data_dir, "main", ExportScope::Room, None)
            .unwrap();
    assert!(export.contains("retained under lock"));
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn real_room_feed_lag_recovers_whole_commits_after_last_seen_cursor() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    let mut feed = room.subscribe();
    let set = || Request::SetVars {
        request_id: EventId::new(),
        updates: vec![VariableUpdate {
            name: VariableName::Rounds,
            value: 3,
        }],
    };
    let result = room.submit(set()).await;
    let RequestResult::Accepted { cursor, .. } = result else {
        panic!("variable update must commit: {result:?}")
    };
    // Worker presence can arrive after subscription even in commit-only mode.
    // Recover from the acknowledged durable update, not an ephemeral feed item.
    let first = loop {
        if let FeedItem::Committed(event) = feed.recv().await.unwrap() {
            assert_eq!(Some(event.commit.seq), cursor);
            assert!(matches!(event.content, EnvelopePayload::VarChange(_)));
            break event;
        }
    };
    for _ in 0..constants::FEED_CAPACITY + 1 {
        assert!(matches!(
            room.submit(set()).await,
            RequestResult::Accepted { .. }
        ));
    }
    assert!(matches!(
        feed.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ));
    let mut cursor = first.commit.seq;
    let mut count = 0;
    loop {
        let page = room.events_after(Some(cursor), 500).await.unwrap();
        if page.is_empty() {
            break;
        }
        for event in page {
            assert!(event.commit.seq > cursor);
            event.validate().unwrap();
            cursor = event.commit.seq;
            count += 1;
        }
    }
    assert_eq!(count, constants::FEED_CAPACITY + 1);
    for limit in [0, 501, usize::MAX] {
        assert_eq!(
            room.page_events(None, limit).await.unwrap_err().code,
            ErrorCode::INVALID_COMMAND
        );
    }
    room.shutdown().await.unwrap();
}
