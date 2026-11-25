#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn stalled_feed_recovers_committed_events_from_the_last_seen_cursor() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    let mut feed = room.subscribe();
    let first = accepted(room.submit(set(3)).await);
    let FeedItem::Committed(event) = feed.recv().await.unwrap() else {
        panic!()
    };
    assert_eq!(event.commit.seq, first);
    for _ in 0..constants::FEED_CAPACITY + 1 {
        accepted(room.submit(set(4)).await);
    }
    assert!(matches!(
        feed.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ));
    let mut cursor = first;
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
    room.shutdown().await.unwrap();
}
