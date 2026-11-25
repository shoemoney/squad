use rusqlite::Connection;
use squad_store::schema;
#[test]
fn census_migration_preserves_released_records_and_integrity() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(schema::INITIAL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    db.execute_batch("INSERT INTO events(seq,id,room_id,epoch,version,kind,correlation_id,sender,recipients,payload,created_at,accepted_at,commit_start_seq,commit_index,commit_size) VALUES(1,'old','main',1,1,'system.notice','old','system','[\"me\"]','{}','time','time',1,0,1); INSERT INTO history(pos,context_id,event_seq,origin_event_id,projection_key,role,content,meta) VALUES(1,'member',1,'old','input','user','retained','{}');").unwrap();
    schema::migrate(&mut db).unwrap();
    schema::migrate(&mut db).unwrap();
    let text: String = db
        .query_row("SELECT content FROM history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(text, "retained");
    assert_eq!(
        db.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        !db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    let sql: String = db
        .query_row("SELECT sql FROM sqlite_master WHERE name='work'", [], |r| {
            r.get(0)
        })
        .unwrap();
    for kind in ["census.hunt", "census.rate", "census.merge", "census.final"] {
        assert!(sql.contains(kind));
    }
    for table in [
        "census_passes",
        "census_finders",
        "census_runs",
        "census_work_inputs",
    ] {
        assert!(db.prepare(&format!("SELECT * FROM {table}")).is_ok());
    }
}
#[test]
fn all_seven_modes_compete_for_one_active_claim() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::migrate(&mut db).unwrap();
    for active in [
        "debate",
        "grill",
        "census",
        "crosscheck",
        "tournament",
        "survey",
        "probe",
    ] {
        db.execute("DELETE FROM contests", []).unwrap();
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES ('active',?,1,1,'{}',1)",[active]).unwrap();
        for other in [
            "debate",
            "grill",
            "census",
            "crosscheck",
            "tournament",
            "survey",
            "probe",
        ] {
            assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES ('other',?,1,1,'{}',1)",[other]).is_err());
        }
    }
}
