use squad_core::export::{self, ExportSnapshot};
use squad_protocol::{forge::*, *};
use std::collections::BTreeMap;
#[test]
fn report_preserves_winning_bytes_and_human_provenance() {
    let r = ForgeReport {
        winner: "a".parse().unwrap(),
        reason: "fusion_won".into(),
        body: "  exact\nartifact\n".into(),
        tally: ForgeTally {
            scores: vec![],
            best_original: "b".parse().unwrap(),
            fusers: vec![],
            max_words: 3,
        },
        provenance: vec![ForgeProvenance {
            element: "Opening".into(),
            source_labels: vec!["D1".into()],
        }],
        sources: BTreeMap::from([("D1".into(), "b".parse().unwrap())]),
        handled_flaws: vec![],
        notes: vec![],
        verdict: None,
        skipped: vec![],
        ballot_maps: serde_json::json!({}),
    };
    let s = export::forge::report(&r);
    assert!(s.contains(&r.body));
    assert!(s.contains("Fusion by @a beat the best draft."));
    assert!(s.contains("| Opening | D1 | @b |"));
    assert!(export::forge::render(&ExportSnapshot::default(), ContestId::new()).is_err());
}
