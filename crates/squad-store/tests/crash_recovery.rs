mod support;
use squad_store::Store;
#[test]
fn interrupted_transaction_rolls_back_counters_receipts_and_accounting_under_wal_full() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    support::event(&store);
    store.call(|db| {
        assert_eq!(db.query_row("PRAGMA journal_mode",[],|r|r.get::<_,String>(0))?,"wal");
        assert_eq!(db.query_row("PRAGMA synchronous",[],|r|r.get::<_,i64>(0))?,2);
        assert_eq!(db.query_row("PRAGMA foreign_keys",[],|r|r.get::<_,i64>(0))?,1);
        let tx=db.transaction()?;
        tx.execute_batch("UPDATE room_meta SET next_event_seq=100,next_history_pos=100; DELETE FROM receipts; INSERT INTO spend_daily VALUES ('2026-11-24',900,0,0)")?;
        // Dropping an uncommitted writer transaction models interruption before COMMIT.
        drop(tx);Ok(())
    }).unwrap();
    let other = squad_store::open(&path).unwrap();
    assert_eq!(
        other
            .query_row(
                "SELECT next_event_seq,next_history_pos FROM room_meta",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            )
            .unwrap(),
        (2, 1)
    );
    assert_eq!(
        other
            .query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        other
            .query_row("SELECT COUNT(*) FROM spend_daily", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        other
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
