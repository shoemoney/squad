#[test]
fn advisory_lock_uses_live_ownership_and_records_pid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.lock");
    let lock = squad_store::room_lock::RoomLock::acquire(&path).unwrap();
    let conflict = squad_store::room_lock::RoomLock::acquire(&path)
        .err()
        .unwrap();
    assert_eq!(
        conflict.to_string(),
        format!(
            "Room main is already open in another squad process (pid {}).",
            std::process::id()
        )
    );
    drop(lock);
    assert!(squad_store::room_lock::RoomLock::acquire(&path).is_ok());
}
#[cfg(unix)]
#[test]
fn database_wal_and_lock_are_private_on_creation_and_reopen() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("squad.db");
    let db = squad_store::open(&path).unwrap();
    db.execute_batch("CREATE TABLE private(value TEXT); INSERT INTO private VALUES ('fixture');")
        .unwrap();
    for path in [
        &path,
        &dir.path().join("squad.db-wal"),
        &dir.path().join("squad.db-shm"),
    ] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(db);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let _db = squad_store::open(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let lock_path = dir.path().join("main.lock");
    let _lock = squad_store::room_lock::RoomLock::acquire(&lock_path).unwrap();
    assert_eq!(
        std::fs::metadata(lock_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn locked_room_open_keeps_the_descriptor_for_every_store_clone() {
    let dir = tempfile::tempdir().unwrap();
    let store = squad_store::Store::open_room(dir.path()).unwrap();
    let clone = store.clone();
    drop(store);
    let error = squad_store::Store::open_room(dir.path()).err().unwrap();
    assert_eq!(
        error.to_string(),
        format!(
            "Room main is already open in another squad process (pid {}).",
            std::process::id()
        )
    );
    clone
        .call(|db| {
            assert_eq!(
                db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?,
                1
            );
            Ok(())
        })
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

#[cfg(unix)]
#[test]
fn reopening_repairs_existing_auxiliary_and_lock_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let db = squad_store::open(&path).unwrap();
    db.execute_batch("CREATE TABLE x(value); INSERT INTO x VALUES (1)")
        .unwrap();
    let files = [
        dir.path().join("room.db-wal"),
        dir.path().join("room.db-shm"),
    ];
    for file in &files {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o666)).unwrap();
    }
    let _other = squad_store::open(&path).unwrap();
    for file in &files {
        assert_eq!(
            std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let path = dir.path().join("main.lock");
    std::fs::write(&path, "999999").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
    let _lock = squad_store::room_lock::RoomLock::acquire(&path).unwrap();
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
