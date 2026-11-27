use squad_core::export::trace::report_text;
use squad_protocol::trace::*;
use std::collections::BTreeMap;
#[test]
fn report_separates_flagged_claims_and_exports_typed_packet_evidence() {
    let r = TraceReport {
        question: "Retention?".into(),
        sources: vec![TraceSource {
            source_id: "S1".into(),
            text: "Records are retained for thirty days.".into(),
        }],
        extractions: BTreeMap::new(),
        provenance: BTreeMap::new(),
        quote_checks: vec![],
        claims: TraceClaims { claims: vec![] },
        assignments: BTreeMap::new(),
        verifications: BTreeMap::new(),
        ledger: TraceLedger {
            claims: vec![TraceLedgerEntry {
                claim_id: "C1".into(),
                text: "Conditional claim".into(),
                status: TraceClaimStatus::Unverified,
                quotes: vec![],
                independent_sources: 1,
                contributing_members: 3,
                verdicts: BTreeMap::new(),
            }],
        },
        brief: Some(TraceBrief {
            sections: vec![],
            disputed: vec![],
            gaps: vec!["Missing corroboration".into()],
        }),
        skipped: vec!["member3: Verifying".into()],
        reason: "judge_failed".into(),
    };
    let public = report_text(&r, false);
    let export = report_text(&r, true);
    assert!(public.contains("independent sources: 1; contributing members: 3"));
    assert!(public.contains("Flagged claims (separate from the brief)"));
    assert!(public.contains("[C1] Unverified"));
    assert!(public.contains("Missing corroboration"));
    assert!(!public.contains("Numbered source packet"));
    assert!(export.contains("### S1"));
    assert!(export.contains("Every verdict"));
    assert!(export.contains("Verifier assignments"));
    assert!(!export.contains("repairTemplate"));
}
