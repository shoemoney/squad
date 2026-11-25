use squad_store::schema;
#[test]
fn migration_preserves_released_rows_and_extends_only_tournament_kinds() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    schema::configure(&db).unwrap();
    db.execute_batch(schema::INITIAL).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    db.execute(
        "INSERT INTO scores(nick,wins,losses,trophies) VALUES ('fable',2,3,4)",
        [],
    )
    .unwrap();
    schema::migrate(&mut db).unwrap();
    schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row("SELECT trophies FROM scores WHERE nick='fable'", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap(),
        4
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    let events: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(events.contains("tournament.entry"));
    assert!(!events.contains("grill.plan"));
    for table in [
        "tournament_labels",
        "tournament_rounds",
        "tournament_results",
    ] {
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name=?",
                [table],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}
