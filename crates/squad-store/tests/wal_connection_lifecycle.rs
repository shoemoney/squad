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
