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
