use squad_core::export::readback::report_text;
use squad_protocol::readback::*;
#[test]
fn report_distinguishes_unread_final_rewrite_and_keeps_raw_answers_in_export() {
    let report = ReadbackReport {
        rounds: vec![],
        raw_answers: vec!["1b".into(), "1a".into()],
        final_spec: "Final exact spec".into(),
        reason: "rounds_complete".into(),
        last_rewrite_not_read_back: true,
        convergence: vec![ReadbackConvergence {
            round: 1,
            spec_version: 1,
            accepted_interpretations: 6,
            ledger_entries: 3,
            resolved: 1,
            open: 2,
            cut: 0,
        }],
    };
    let out = report_text(&report, true);
    assert!(out.contains("Final exact spec"));
    assert!(out.contains("last rewrite was not read back"));
    assert!(out.contains("| 1 | 1 | 6 | 3 | 1 | 2 | 0 |"));
    assert!(out.contains("- 1b\n- 1a"));
    assert!(!report_text(&report, false).contains("Human raw answers"));
}
