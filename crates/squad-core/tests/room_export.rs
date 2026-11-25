#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn live_and_offline_export_share_the_snapshot_while_lock_is_held() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    accepted(
        room.submit(message("@a exported text", EventId::new()))
            .await,
    );
    let live = room.export(ExportScope::Room, None).await.unwrap();
    assert!(live.contains("exported text"));
    let offline = squad_core::room::export_offline(
        &config(root.path()).data_dir,
        "main",
        ExportScope::Room,
        None,
    )
    .unwrap();
    assert_eq!(offline, live);
    assert_eq!(
        room.export(ExportScope::Debate, None)
            .await
            .unwrap_err()
            .code,
        ErrorCode::INVALID_COMMAND
    );
    room.shutdown().await.unwrap();
    assert_eq!(
        squad_core::room::export_offline(
            &config(root.path()).data_dir,
            "main",
            ExportScope::Room,
            None
        )
        .unwrap(),
        live
    );
}
