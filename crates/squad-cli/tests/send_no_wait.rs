mod support;
use squad_cli::room_api::OpenPolicy;
use squad_protocol::*;
use support::*;

#[tokio::test]
async fn no_wait_returns_receipt_without_waiting_or_stopping_work() {
    let room = FakeRoom::default();
    room.progress.set(squad_cli::room_api::Progress::Running);
    let (code, output, _) = send(&room, &args("@all hello", true, true), None).await;
    assert_eq!(code, 0);
    assert_eq!(output.lines().count(), 1);
    assert_eq!(room.requests.borrow().len(), 1);
    assert!(matches!(
        room.requests.borrow()[0],
        Request::UserMessage { .. }
    ));
    assert_eq!(room.shutdowns.get(), 1);
    assert_eq!(
        OpenPolicy::send(true),
        OpenPolicy {
            mode: OpenMode::OneShot,
            commit_only: true
        }
    );
    insta::assert_snapshot!(normalize(&output), @r#"{"cursor":"1","eventIds":["01900000-0000-7000-8000-000000000001"],"requestId":"01900000-0000-7000-8000-000000000001","type":"ack"}"#);
}

#[tokio::test]
async fn local_no_wait_result_has_no_cursor_or_ack() {
    let room = FakeRoom::default();
    let (code, output, _) = send(&room, &args("/who", true, true), None).await;
    assert_eq!(code, 0);
    assert!(room.requests.borrow().is_empty());
    insta::assert_snapshot!(normalize(&output), @r#"{"requestId":"01900000-0000-7000-8000-000000000001","result":{"command":"who","state":{"room":"main"}},"type":"command.result"}"#);
}
