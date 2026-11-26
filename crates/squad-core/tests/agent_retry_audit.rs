#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use support::{Fixture, fake_openrouter::*};

#[tokio::test]
async fn user_retry_executes_new_work_without_repeating_the_human_message() {
    let fixture = Fixture::new().await;
    fixture
        .fake
        .completions(vec![
            provider_error(400),
            streaming("recovered answer", "stop"),
        ])
        .await;
    let room = fixture.open(OpenMode::Interactive).await;
    assert!(matches!(
        room.submit(fixture.request("@member0 UNIQUE_RETRY_QUESTION"))
            .await,
        RequestResult::Accepted { .. }
    ));
    let original: WorkId = fixture
        .db()
        .query_row("SELECT work_id FROM work", [], |r| r.get::<_, String>(0))
        .unwrap()
        .parse()
        .unwrap();
    fixture
        .wait(|| fixture.scalar("SELECT COUNT(*) FROM work WHERE status='failed'") == 1)
        .await;
    let retry = Request::Retry {
        request_id: EventId::new(),
        work_id: original,
    };
    let receipt = room.submit(retry.clone()).await;
    assert!(matches!(receipt, RequestResult::Accepted { .. }));
    assert_eq!(room.submit(retry).await, receipt);
    fixture.finals(1).await;
    assert_eq!(
        fixture.scalar("SELECT COUNT(*) FROM events WHERE kind='user.message'"),
        1
    );
    assert_eq!(fixture.scalar("SELECT COUNT(*) FROM work"), 2);
    let calls = fixture.fake.captured_bodies().await;
    assert_eq!(calls.len(), 2);
    for request in calls {
        assert_eq!(
            request.to_string().matches("UNIQUE_RETRY_QUESTION").count(),
            1
        );
    }
    room.shutdown().await.unwrap();
}
