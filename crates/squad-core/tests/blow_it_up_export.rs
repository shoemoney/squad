use squad_core::export::{self, ExportSnapshot};
use squad_protocol::blow_it_up::*;
use squad_protocol::*;
#[test]
fn report_discloses_unchallenged_additions_and_skipped_members() {
    let r = BlowItUpReport {
        findings: vec![],
        killed: vec![],
        additions: vec![],
        unchallenged: vec!["v1".parse().unwrap()],
        skipped: vec![SkippedMember {
            member: "alpha".parse().unwrap(),
            step: "challenge".into(),
        }],
    };
    let text = export::blow_it_up::report_text(&r, true);
    for needle in [
        "BLOW IT UP",
        "WHAT SURVIVED",
        "false-positive filter did not run",
        "Skipped @alpha",
        "does not establish safety",
        "Killed appendix",
    ] {
        assert!(text.contains(needle), "{text}");
    }
    assert!(export::blow_it_up::render(&ExportSnapshot::default(), ContestId::new()).is_err());
}
#[test]
fn killed_appendix_is_collapsed_by_default_and_contains_quotes_when_expanded() {
    let vector = PooledVector {
        vector_id: "v1".parse().unwrap(),
        title: "Already protected".parse().unwrap(),
        target_quote: "All writes are authenticated.".parse().unwrap(),
        exploit: "Unprotected writes".parse().unwrap(),
        severity: Severity::High,
        likelihood: Likelihood::Likely,
        blast_radius: "All data".parse().unwrap(),
        addition: false,
    };
    let r = BlowItUpReport {
        findings: vec![],
        killed: vec![KilledVector {
            vector,
            tally: VectorTally {
                vector_id: "v1".parse().unwrap(),
                accepted: 3,
                false_positive: 2,
                weak: 0,
                status: VectorStatus::Killed,
            },
            defeating_quotes: vec!["All writes are authenticated.".parse().unwrap()],
            judge_note: Some("Check bypasses separately".parse().unwrap()),
        }],
        additions: vec![],
        unchallenged: vec![],
        skipped: vec![],
    };
    assert!(!export::blow_it_up::report_text(&r, false).contains("Defeating quote"));
    let text = export::blow_it_up::report_text(&r, true);
    assert!(text.contains("false_positive 2/3"));
    assert!(text.contains("All writes are authenticated."));
    assert!(text.contains("Check bypasses separately"));
}
