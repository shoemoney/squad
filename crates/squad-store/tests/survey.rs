use rusqlite::{Connection, params};
use squad_store::schema;
#[test]
fn migration_preserves_released_records_and_enforces_ballot_evidence_and_identity() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(schema::INITIAL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES ('old','debate',1,1,'{}',0)",[]).unwrap();
    schema::migrate(&mut db).unwrap();
    schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT kind FROM contests WHERE contest_id='old'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "debate"
    );
    assert!(
        db.execute("INSERT INTO survey_sources VALUES ('old','{}')", [])
            .is_err()
    );
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES ('survey','survey',1,1,'{}',0)",[]).unwrap();
    db.execute(
        "INSERT INTO survey_sources VALUES ('survey',?)",
        [serde_json::json!({"contestId":"survey","participants":["alpha","beta"]}).to_string()],
    )
    .unwrap();
    // Reuse a retained human event identity; SQL enforces the foreign key as well.
    db.execute("INSERT INTO events VALUES (1,'event','main',1,1,'system.notice','c',NULL,'system','[\"me\"]','{}','now','now',1,0,1)",[]).unwrap();
    db.execute(
        "INSERT INTO survey_claims VALUES ('survey','c1',1,'Claim','[\"answer\"]')",
        [],
    )
    .unwrap();
    let sql = "INSERT INTO survey_votes VALUES ('survey','c1',2,?,'AGREE',4,'Reason',?,1)";
    assert!(
        db.execute(sql, params!["alpha", Option::<String>::None])
            .is_err()
    );
    assert!(db.execute(sql, params!["alpha", " "]).is_err());
    assert!(db.execute(sql, params!["foreign", "A test"]).is_err());
    db.execute(sql, params!["alpha", "A test"]).unwrap();
    assert!(
        db.execute(sql, params!["alpha", "Changed duplicate"])
            .is_err()
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM survey_votes", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}
#[test]
fn every_mode_competes_for_the_same_active_claim() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::migrate(&mut db).unwrap();
    for first in [
        "debate",
        "tournament",
        "grill",
        "census",
        "crosscheck",
        "survey",
        "probe",
    ] {
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES ('one',?,1,1,'{}',1)",[first]).unwrap();
        for second in [
            "debate",
            "tournament",
            "grill",
            "census",
            "crosscheck",
            "survey",
            "probe",
        ] {
            assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES ('two',?,1,1,'{}',1)",[second]).is_err());
        }
        db.execute("DELETE FROM contests", []).unwrap();
    }
}
