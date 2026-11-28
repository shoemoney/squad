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
async fn default_client_is_configured_and_unknown_rooms_fail_before_network() {
    let root = tempfile::tempdir().unwrap();
    let _client =
        squad_agents::openrouter::OpenRouterClient::from_config(&config(root.path())).unwrap();
    let result = RoomHandle::open(config(root.path()), "other", OpenMode::Interactive).await;
    assert!(matches!(result,Err(error) if error.code==ErrorCode::ROOM_NOT_FOUND));
    assert!(!config(root.path()).data_dir.exists());
}

// ---- M3.P1.L1.T7: lifecycle coverage for census, crosscheck, survey and probe ----

const NEW_MODES: [(&str, ContestKind); 4] = [
    (
        "@all $census Build a database service.",
        ContestKind::Census,
    ),
    (
        "@all $crosscheck Maintain service.\n--\nKeep service available.",
        ContestKind::Crosscheck,
    ),
    ("@all $survey Which checks help?", ContestKind::Survey),
    (
        "@all $probe Intermittent noise, fan ruled out, inspect indicator only",
        ContestKind::Probe,
    ),
];

fn pending(result: &RequestResult) -> (&PendingPreflight, Cursor) {
    match result {
        RequestResult::PendingConfirmation {
            preflight, cursor, ..
        } => (preflight, *cursor),
        other => panic!("expected a pending confirmation, got {other:?}"),
    }
}

fn contest_plan(preflight: &PendingPreflight) -> &ContestPreflight {
    match preflight {
        PendingPreflight::Contest(plan) => plan,
        other => panic!("expected a contest preflight, got {other:?}"),
    }
}

fn work_counts(launcher: &Launcher) -> (i64, i64) {
    store(launcher)
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))?,
                db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))?,
            ))
        })
        .unwrap()
}

#[tokio::test]
async fn new_mode_preflights_recover_without_waking_work_in_one_shot_opens() {
    for (text, kind) in NEW_MODES {
        let root = tempfile::tempdir().unwrap();
        let first = Launcher::default();
        let (room, _) = open4(root.path(), &first, OpenMode::OneShot).await;
        let id = EventId::new();
        let request = message4(text, id);
        let result = room.submit(request.clone()).await;
        let (preflight, cursor) = pending(&result);
        let plan = contest_plan(preflight).clone();
        assert_eq!(plan.kind, kind, "{text}");
        assert!(plan.confirmation_required);
        assert!(cursor.get() > 0);
        // An unconfirmed preflight admits no work and never wakes the runtime.
        assert!(first.wakes.lock().unwrap().is_empty(), "{kind:?}");
        assert_eq!(work_counts(&first), (0, 0), "{kind:?}");
        // The same receipt replays to the same pending confirmation.
        assert_eq!(room.submit(request.clone()).await, result, "{kind:?}");
        let before = room.state().await;
        assert_eq!(before.pending_preflights.len(), 1);
        room.shutdown().await.unwrap();
        let next = Launcher::default();
        let (room, _) = open4(root.path(), &next, OpenMode::OneShot).await;
        assert!(next.wakes.lock().unwrap().is_empty(), "{kind:?}");
        let after = room.state().await;
        assert_eq!(after.pending_preflights, before.pending_preflights);
        assert_eq!(after.active_contest, before.active_contest);
        assert_eq!(room.submit(request).await, result, "{kind:?}");
        assert_eq!(work_counts(&next), (0, 0));
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn new_mode_confirmation_is_commit_only_without_provider_attempts() {
    for (text, kind) in NEW_MODES {
        let root = tempfile::tempdir().unwrap();
        let launcher = Launcher::default();
        let (room, server) = open4(root.path(), &launcher, OpenMode::OneShot).await;
        let result = room.submit(message4(text, EventId::new())).await;
        let plan = contest_plan(pending(&result).0).clone();
        let catalog_calls = server.received_requests().await.unwrap().len();
        let confirm = Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        };
        let receipt = room.submit_commit_only(confirm.clone()).await;
        accepted(receipt.clone());
        assert!(launcher.wakes.lock().unwrap().is_empty(), "{kind:?}");
        let (work, attempts) = work_counts(&launcher);
        assert!(work > 0, "{kind:?} admitted no work");
        assert_eq!(attempts, 0, "{kind:?}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            catalog_calls,
            "commit-only confirmation never calls the provider"
        );
        assert!(room.state().await.pending_preflights.is_empty());
        // Receipt replay is idempotent and admits nothing more.
        assert_eq!(room.submit_commit_only(confirm).await, receipt);
        assert_eq!(work_counts(&launcher).0, work);
        room.shutdown().await.unwrap();
        // A restarted one-shot open preserves the committed contest and work without waking it.
        let next = Launcher::default();
        let (room, _) = open4(root.path(), &next, OpenMode::OneShot).await;
        assert!(next.wakes.lock().unwrap().is_empty(), "{kind:?}");
        assert_eq!(work_counts(&next).0, work);
        assert!(room.state().await.active_contest.is_some(), "{kind:?}");
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn stopping_a_new_mode_preflight_cancels_it_and_a_second_stop_is_rejected() {
    for (text, kind) in NEW_MODES {
        let root = tempfile::tempdir().unwrap();
        let launcher = Launcher::default();
        let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
        room.submit(message4(text, EventId::new())).await;
        accepted(
            room.submit(Request::Stop {
                request_id: EventId::new(),
                target: None,
            })
            .await,
        );
        let state = room.state().await;
        assert!(state.pending_preflights.is_empty(), "{kind:?}");
        assert!(state.active_contest.is_none(), "{kind:?}");
        assert!(launcher.wakes.lock().unwrap().is_empty());
        assert!(
            matches!(
                room.submit(Request::Stop { request_id: EventId::new(), target: None }).await,
                RequestResult::Rejected { error, .. } if error.code == ErrorCode::NOTHING_TO_STOP
            ),
            "{kind:?}"
        );
        room.shutdown().await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn read_only_data_dir_fails_before_creating_lock() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let root = tempfile::tempdir().unwrap();
    if std::fs::metadata(root.path()).unwrap().uid() == 0 {
        eprintln!("skipped: root bypasses directory write permissions");
        return;
    }
    let cfg = config(root.path());
    std::fs::create_dir_all(&cfg.data_dir).unwrap();
    std::fs::set_permissions(&cfg.data_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = RoomHandle::open(cfg.clone(), "main", OpenMode::OneShot).await;
    // Restore permissions before assertions so TempDir can always clean up.
    std::fs::set_permissions(&cfg.data_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let error = match result {
        Ok(_) => panic!("read-only room open succeeded"),
        Err(error) => error,
    };
    assert!(!cfg.data_dir.join("main.lock").exists());
    // Debug comparison lets this regression run against the unchanged base API.
    assert_eq!(format!("{:?}", error.code), "DATA_DIR_NOT_WRITABLE");
    assert!(error.message.contains(&cfg.data_dir.display().to_string()));
    assert!(error.message.contains("grant write access"));
    assert!(error.message.contains("--data-dir"));
    assert_eq!(std::fs::read_dir(&cfg.data_dir).unwrap().count(), 0);
}
