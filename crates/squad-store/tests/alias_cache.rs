use rusqlite::Connection;
use squad_store::schema;

#[test]
fn alias_cache_migration_is_additive_strict_and_idempotent() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::migrate(&mut db).unwrap();
    db.execute(
        "INSERT INTO alias_cache VALUES ('fixture','~vendor/latest','{}',100)",
        [],
    )
    .unwrap();
    schema::migrate(&mut db).unwrap();
    assert_eq!(
        db.query_row("SELECT resolved_at FROM alias_cache", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        100
    );
    assert!(
        db.execute(
            "INSERT INTO alias_cache VALUES ('fixture','concrete/model','{}',100)",
            []
        )
        .is_err()
    );
    assert!(
        db.execute(
            "INSERT INTO alias_cache VALUES ('fixture','~vendor/latest','{}',100)",
            []
        )
        .is_err()
    );
    assert!(
        db.execute("UPDATE alias_cache SET resolved_at=-1", [])
            .is_err()
    );
    assert!(
        db.execute("UPDATE alias_cache SET model_json='broken'", [])
            .is_err()
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
