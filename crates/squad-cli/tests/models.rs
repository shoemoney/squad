mod support;
use support::*;

#[test]
fn cache_order_and_concrete_judge_are_read_without_room_lock() {
    let temp = tempfile::tempdir().unwrap();
    let db = database(temp.path(), vec![]);
    for (nick, role, order, requested, resolved) in [
        ("beta", "member", Some(1), "p/beta", "p/beta-20261101"),
        ("judge", "judge", None, "p/judge", "p/judge-20261101"),
        ("alpha", "member", Some(0), "p/alpha", "p/alpha-20261101"),
    ] {
        db.execute(
            "INSERT INTO models VALUES (?1,?2,?3,?4,?5,1000,'{}','2026-11-24T00:00:00Z')",
            rusqlite::params![nick, role, order, requested, resolved],
        )
        .unwrap();
    }
    let _lock = squad_store::room_lock::RoomLock::acquire(&temp.path().join("main.lock")).unwrap();
    let output = squad_cli::models::run(temp.path(), None).unwrap();
    insta::assert_snapshot!(output, @"
    alpha (member): p/alpha -> p/alpha-20261101
    beta (member): p/beta -> p/beta-20261101
    judge (judge): p/judge -> p/judge-20261101
    ");
}

#[test]
fn missing_cache_does_not_create_a_database_or_runtime_lock() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        squad_cli::models::run(temp.path(), None)
            .unwrap()
            .contains("Open the room")
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn malformed_database_errors_are_sanitized() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("squad.db"), b"PRIVATE-SECRET-NOT-SQLITE").unwrap();
    let error = squad_cli::models::run(temp.path(), None).unwrap_err();
    assert!(!error.contains("PRIVATE-SECRET"));
    assert_eq!(error, "Could not read model cache.");
}
