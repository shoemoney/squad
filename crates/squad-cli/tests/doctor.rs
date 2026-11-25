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
