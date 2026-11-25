mod support;
use squad_protocol::*;
use squad_store::{Store, accounting, commit};

#[test]
fn sqlite_busy_commit_is_not_acknowledged_and_preserves_history_and_accounting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let retained = support::event(&store);
    let mut db = squad_store::open(&path).unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let lock = squad_store::open(&path).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    let id = EventId::new();
    let result = commit::commit(&mut db, id, "m6", "m6", |tx, group| {
        accounting::reserve(tx, "failed", None, "2026-11-25", 100, 1000)?;
        Ok(group.accepted(id))
    });
    assert!(
        matches!(result,Err(squad_store::StoreError::Sql(rusqlite::Error::SqliteFailure(error,_))) if error.code==rusqlite::ErrorCode::DatabaseBusy)
    );
    lock.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        squad_store::events::at(&db, retained.commit.seq.get()).unwrap(),
        retained
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipts WHERE request_id=?",
            [id.to_string()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        accounting::daily(&db, "2026-11-25")
            .unwrap()
            .reserved_micros,
        0
    );
    assert!(commit::commit(&mut db, id, "m6", "m6", |_, group| Ok(group.accepted(id))).is_ok());
}

#[test]
fn full_database_rolls_back_the_whole_commit_and_can_be_read_afterward() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let retained = support::event(&store);
    let mut db = squad_store::open(&path).unwrap();
    db.execute_batch("CREATE TABLE m6_fill(value BLOB)")
        .unwrap();
    let pages: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
    db.pragma_update(None, "max_page_count", pages).unwrap();
    let id = EventId::new();
    let result = commit::commit(&mut db, id, "m6", "m6", |tx, group| {
        accounting::reserve(tx, "rolled-back", None, "2026-11-25", 100, 1000)?;
        tx.execute("INSERT INTO m6_fill VALUES (zeroblob(8388608))", [])?;
        Ok(group.accepted(id))
    });
    assert!(
        matches!(result,Err(squad_store::StoreError::Sql(rusqlite::Error::SqliteFailure(error,_))) if error.code==rusqlite::ErrorCode::DiskFull)
    );
    assert_eq!(
        accounting::daily(&db, "2026-11-25")
            .unwrap()
            .reserved_micros,
        0
    );
    assert_eq!(
        squad_store::events::at(&db, retained.commit.seq.get()).unwrap(),
        retained
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM receipts WHERE request_id=?",
            [id.to_string()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn corrupt_database_is_rejected_without_replacing_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let bytes = b"M6 deliberately corrupt sqlite: preserve for recovery";
    std::fs::write(&path, bytes).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn concurrent_reservations_serialize_at_equality_and_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let tasks: Vec<_> = (0..16)
        .map(|i| {
            let store = store.clone();
            std::thread::spawn(move || {
                store.call(move |db| {
                    let tx =
                        db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                    accounting::reserve(&tx, &format!("m6-{i}"), None, "2026-11-25", 100, 800)?;
                    tx.commit()?;
                    Ok(())
                })
            })
        })
        .collect();
    let accepted = tasks
        .into_iter()
        .map(|t| t.join().unwrap())
        .filter(Result::is_ok)
        .count();
    assert_eq!(accepted, 8);
    drop(store);
    let reopened = Store::open(path).unwrap();
    reopened
        .call(|db| {
            assert_eq!(accounting::daily(db, "2026-11-25")?.reserved_micros, 800);
            assert!(accounting::admit(db, "2026-11-25", 1, 800).is_err());
            assert!(accounting::admit(db, "2026-11-26", 100, 800).is_ok());
            Ok(())
        })
        .unwrap();
}
