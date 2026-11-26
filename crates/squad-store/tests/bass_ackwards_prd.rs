use squad_protocol::*;
use squad_store::{Store, bass_ackwards_prd::*};
#[test]
fn migration_records_round_trip_and_cascade_on_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("room.db");
    let store = Store::open(&path).unwrap();
    let id = ContestId::new();
    store.call(move|db|{db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?,'bass_ackwards_prd',1,1,'{}',0)",[id.to_string()])?;put(db,id,"lens",&serde_json::json!({"rotation":["a","b","c"]}))?;put(db,id,"version:1",&serde_json::json!({"key":"permanent"}))?;Ok(())}).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    store
        .call(move |db| {
            let v: serde_json::Value = get(db, id, "version:1")?;
            assert_eq!(v["key"], "permanent");
            assert!(optional::<serde_json::Value>(db, id, "missing")?.is_none());
            db.execute("DELETE FROM contests WHERE contest_id=?", [id.to_string()])?;
            let n: i64 =
                db.query_row("SELECT COUNT(*) FROM bass_ackwards_prd_records", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(n, 0);
            Ok(())
        })
        .unwrap();
}
