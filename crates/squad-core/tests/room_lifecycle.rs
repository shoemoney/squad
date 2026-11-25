#[path = "room_support.rs"]
mod support;
use squad_core::room::*;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn lock_diagnostic_shutdown_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, server) = open(root.path(), &launcher, OpenMode::Interactive).await;
    let error = match RoomHandle::open_with_launcher(
        config(root.path()),
        "main",
        OpenMode::OneShot,
        client(&server),
        &launcher,
    )
    .await
    {
        Ok(_) => panic!("second open succeeded"),
        Err(e) => e,
    };
    assert_eq!(
        error.to_string(),
        format!(
            "Room main is already open in another squad process (pid {}).",
            std::process::id()
        )
    );
    room.shutdown().await.unwrap();
    room.shutdown().await.unwrap();
    assert!(*launcher.stopped.lock().unwrap());
    assert!(
        matches!(room.submit(set(3)).await,RequestResult::Rejected {error,..} if error.code==ErrorCode::NOT_READY)
    );
    let (next, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    next.shutdown().await.unwrap();
}
#[tokio::test]
async fn readiness_and_resolution_failures_release_the_lock() {
    let root = tempfile::tempdir().unwrap();
    let server = server().await;
    let failing = Launcher {
        fail_ready: true,
        ..Default::default()
    };
    let result = RoomHandle::open_with_launcher(
        config(root.path()),
        "main",
        OpenMode::Interactive,
        client(&server),
        &failing,
    )
    .await;
    assert!(matches!(result,Err(e) if e.code==ErrorCode::NOT_READY));
    assert!(*failing.stopped.lock().unwrap());
    let mut cfg = config(root.path());
    cfg.agents[0].model = "missing/model".into();
    let result = RoomHandle::open_with_launcher(
        cfg,
        "main",
        OpenMode::Interactive,
        client(&server),
        &Launcher::default(),
    )
    .await;
    assert!(result.is_err());
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::Interactive).await;
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn one_shot_only_wakes_submitted_work_and_commit_only_never_wakes() {
    let root = tempfile::tempdir().unwrap();
    let first = Launcher::default();
    let (room, _) = open(root.path(), &first, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a prior", EventId::new()))
            .await,
    );
    assert!(first.wakes.lock().unwrap().is_empty());
    room.shutdown().await.unwrap();
    let second = Launcher::default();
    let (room, server) = open(root.path(), &second, OpenMode::OneShot).await;
    assert!(second.wakes.lock().unwrap().is_empty());
    let prior = store(&second)
        .call(|db| Ok(db.query_row("SELECT work_id FROM work", [], |r| r.get::<_, String>(0))?))
        .unwrap();
    accepted(room.submit(message("@b current", EventId::new())).await);
    let wakes = second.wakes.lock().unwrap().clone();
    assert_eq!(wakes.len(), 1);
    assert_ne!(wakes[0].to_string(), prior);
    let attempts = store(&second)
        .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))?))
        .unwrap();
    assert_eq!(attempts, 0);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    room.shutdown().await.unwrap();
}

#[test]
fn competing_process_child() {
    let Some(path) = std::env::var_os("SQUAD_ROOM_TEST_LOCK") else {
        return;
    };
    let error = match squad_store::room_lock::RoomLock::acquire(std::path::Path::new(&path)) {
        Ok(_) => panic!("child acquired parent's lock"),
        Err(error) => error,
    };
    assert_eq!(
        error.to_string(),
        format!(
            "Room main is already open in another squad process (pid {}).",
            std::env::var("SQUAD_ROOM_TEST_PARENT_PID").unwrap()
        )
    );
}
#[tokio::test]
async fn independent_process_cannot_acquire_open_rooms_lock() {
    let root = tempfile::tempdir().unwrap();
    let (room, _) = open(root.path(), &Launcher::default(), OpenMode::OneShot).await;
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "competing_process_child", "--nocapture"])
        .env(
            "SQUAD_ROOM_TEST_LOCK",
            config(root.path()).data_dir.join("main.lock"),
        )
        .env("SQUAD_ROOM_TEST_PARENT_PID", std::process::id().to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn default_open_requires_the_real_runtime_binding_and_rejects_unknown_rooms() {
    let root = tempfile::tempdir().unwrap();
    let result = RoomHandle::open(config(root.path()), "main", OpenMode::Interactive).await;
    assert!(matches!(result,Err(error) if error.code==ErrorCode::NOT_READY));
    let result = RoomHandle::open(config(root.path()), "other", OpenMode::Interactive).await;
    assert!(matches!(result,Err(error) if error.code==ErrorCode::ROOM_NOT_FOUND));
    assert!(!config(root.path()).data_dir.exists());
}
