mod support;
use support::*;

#[test]
fn unknown_charges_keep_reservations_and_are_never_reported_as_zero_cost() {
    let temp = tempfile::tempdir().unwrap();
    let db = database(temp.path(), vec![]);
    db.execute(
        "INSERT INTO spend_daily VALUES ('2026-11-24',1234567,2500000,2)",
        [],
    )
    .unwrap();
    let output = squad_cli::diagnostics::accounting(temp.path(), "2026-11-24", 20.).unwrap();
    insta::assert_snapshot!(output, @"
    Daily accounting: 2026-11-24; actual $1.23; reserved $2.50; effective max $20.00; unknown-cost calls: 2; unreconciled charges (~)
    ");
    let before = db.total_changes();
    squad_cli::diagnostics::accounting(temp.path(), "2026-11-25", 35.).unwrap();
    assert_eq!(db.total_changes(), before);
}

#[test]
fn absent_database_is_read_only_and_reports_no_ledger_yet() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        squad_cli::diagnostics::accounting(temp.path(), "2026-11-24", 20.)
            .unwrap()
            .contains("no ledger yet")
    );
    assert!(
        squad_cli::diagnostics::identities(temp.path())
            .unwrap()
            .contains("no recorded attempts")
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn response_model_mismatches_keep_work_attempt_and_concrete_slugs() {
    let temp = tempfile::tempdir().unwrap();
    let db = database(temp.path(), vec![]);
    db.execute("INSERT INTO attempts(work_id,attempt,epoch,target,context_id,model,response_model,day,started_at,outcome,estimated_cost_usd,unresolved) VALUES (?1,1,1,'alpha','member:alpha','p/model-20261101','p/other-20261102','2026-11-24','2026-11-24T00:00:00Z','unknown',0.5,1)", [ID]).unwrap();
    let output = squad_cli::diagnostics::identities(temp.path()).unwrap();
    assert!(output.contains("attempt 1"));
    assert!(output.contains(ID));
    assert!(output.contains("selected p/model-20261101; response p/other-20261102"));
}
#[test]
fn census_diagnostics_read_persisted_value_without_writing_or_replacing_seed() {
    let temp = tempfile::tempdir().unwrap();
    let absent =
        squad_cli::diagnostics::census_passes(temp.path(), Some(5), "process environment").unwrap();
    assert!(absent.contains("effective 5; source process environment"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let db = database(temp.path(), vec![]);
    db.execute("INSERT INTO vars(name,value) VALUES ('CENSUS_PASSES',3) ON CONFLICT(name) DO UPDATE SET value=3",[]).unwrap();
    let before = db.total_changes();
    let stored =
        squad_cli::diagnostics::census_passes(temp.path(), Some(5), "process environment").unwrap();
    assert!(stored.contains("effective 3; source persisted room value"));
    assert!(stored.contains("seed 5; default 2; range 1..=5"));
    assert_eq!(db.total_changes(), before);
}

#[test]
fn storage_reports_used_of_capacity_and_warns_near_and_at_the_ceiling() {
    let temp = tempfile::tempdir().unwrap();
    let absent = squad_cli::diagnostics::storage(temp.path(), 1 << 30).unwrap();
    assert!(absent.contains("Storage: 0.0 MiB used of 1024.0 MiB (0.0%)"));
    assert!(!absent.contains("WARNING"));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    let db = database(temp.path(), vec![]);
    let used = squad_store::storage::used_bytes(&db).unwrap();
    assert!(used > 0);
    let before = db.total_changes();
    let roomy = squad_cli::diagnostics::storage(temp.path(), used * 100).unwrap();
    assert!(roomy.starts_with("Storage: "));
    assert!(!roomy.contains("WARNING"));
    let near = squad_cli::diagnostics::storage(temp.path(), used * 105 / 100).unwrap();
    assert!(near.contains("WARNING - near the ceiling"));
    let full = squad_cli::diagnostics::storage(temp.path(), used).unwrap();
    assert!(full.contains("at the ceiling") && full.contains("STORAGE_LIMIT"));
    assert_eq!(db.total_changes(), before);
}
