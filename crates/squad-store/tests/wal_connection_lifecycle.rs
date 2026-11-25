//! WAL connections must remain safe while the owning writer closes asynchronously.
use std::{process::Command, sync::Arc, time::Duration};

#[test]
fn concurrent_wal_open_and_close_completes() {
    const CHILD: &str = "SQUAD_WAL_LIFECYCLE_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // 3.51.0/1 invert the global VFS and inode mutexes in findReusableFd
        // and unixIsSharingShmNode. Keep the fixed bundled engine selected.
        assert!(rusqlite::version_number() >= 3_051_002);
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join("lifecycle.db"));
        let db = squad_store::open(path.as_ref()).unwrap();
        db.execute_batch("CREATE TABLE proof(value); INSERT INTO proof VALUES(42);")
            .unwrap();
        drop(db);
        let barrier = Arc::new(std::sync::Barrier::new(8));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let path = path.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    for _ in 0..200 {
                        let db = rusqlite::Connection::open(path.as_ref()).unwrap();
                        let value: i64 = db
                            .query_row("SELECT value FROM proof", [], |r| r.get(0))
                            .unwrap();
                        assert_eq!(value, 42);
                        drop(db);
                    }
                });
            }
        });
        return;
    }
    // A native SQLite mutex deadlock cannot be interrupted by a Tokio timeout.
    // Isolate the probe so a regression fails without hanging the test harness.
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "concurrent_wal_open_and_close_completes",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "WAL lifecycle subprocess failed: {status}"
            );
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("concurrent WAL open/close deadlocked");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Opening a second store must queue behind a live writer instead of failing.
/// SQLite never runs the busy handler for a read transaction upgrading to a
/// write, so a migration that reads before it writes fails at once with
/// `SQLITE_BUSY` whenever the room writer is mid-commit.
#[test]
fn second_store_open_waits_for_the_active_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("busy.db");
    let _owner = squad_store::Store::open(&path).unwrap();
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; UPDATE vars SET value=value WHERE name='ROUNDS';")
        .unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        writer.execute_batch("COMMIT").unwrap();
    });
    let second = squad_store::Store::open(&path);
    release.join().unwrap();
    let second = second.unwrap();
    let rounds: i64 = second
        .call(|db| {
            Ok(
                db.query_row("SELECT value FROM vars WHERE name='ROUNDS'", [], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap();
    assert_eq!(rounds, 10);
}

/// Reopening a current database must not rewrite its header on every open.
#[test]
fn reopening_a_current_schema_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.db");
    let mut db = squad_store::open(&path).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    let before: i64 = db
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap();
    let mut other = squad_store::open(&path).unwrap();
    squad_store::schema::migrate(&mut other).unwrap();
    let after: i64 = db
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
}
