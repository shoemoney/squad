use rusqlite::Connection;
#[test]
fn migration_preserves_released_records_and_foreign_keys() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(squad_store::schema::INITIAL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('released','debate',1,0,'{}',0)",[]).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT kind FROM contests WHERE contest_id='released'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "debate"
    );
    assert!(
        db.execute(
            "INSERT INTO probe_roles(contest_id,nick,role) VALUES('released','member','explainer')",
            []
        )
        .is_err()
    );
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
}
#[test]
fn all_seven_modes_share_one_active_claim() {
    let mut db = Connection::open_in_memory().unwrap();
    squad_store::schema::migrate(&mut db).unwrap();
    for active in [
        "debate",
        "grill",
        "tournament",
        "census",
        "crosscheck",
        "survey",
        "probe",
    ] {
        db.execute("DELETE FROM contests", []).unwrap();
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('one',?,1,0,'{}',1)",[active]).unwrap();
        for other in [
            "debate",
            "grill",
            "tournament",
            "census",
            "crosscheck",
            "survey",
            "probe",
        ] {
            assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('two',?,1,0,'{}',1)",[other]).is_err());
        }
    }
}
