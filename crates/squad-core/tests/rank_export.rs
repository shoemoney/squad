use squad_core::export::{
    ExportSnapshot,
    rank::{render, report_text},
};
use squad_protocol::{rank::*, *};
use std::collections::BTreeMap;
fn report() -> RankReport {
    let nicks = vec![
        "a".parse().unwrap(),
        "b".parse().unwrap(),
        "c".parse().unwrap(),
    ];
    let (pairs, votes_per_pair, deal) = squad_core::orchestrator::rank::design(3, &nicks, 9);
    RankReport {
        setup: RankSetup {
            criterion: "Impact".into(),
            items: vec![
                RankItem {
                    item_id: "I1".into(),
                    text: "Fix data loss".into(),
                },
                RankItem {
                    item_id: "I2".into(),
                    text: "Search".into(),
                },
                RankItem {
                    item_id: "I3".into(),
                    text: "Icons".into(),
                },
            ],
            participants: nicks,
            pairs,
            votes_per_pair,
            deal,
            models: BTreeMap::new(),
            prompt_refs: BTreeMap::new(),
            room_words: 500,
        },
        batches: BTreeMap::new(),
        tally: Some(squad_core::orchestrator::rank::fit(
            &[vec![0, 3, 3], vec![0, 0, 2], vec![0, 1, 0]],
            &[],
        )),
        explanation: None,
        skipped: vec!["b".parse().unwrap()],
        reason: "judge_failed".into(),
    }
}
#[test]
fn report_preserves_app_order_audit_and_judge_failure() {
    let r = report();
    let human = report_text(&r, false);
    let full = report_text(&r, true);
    assert!(human.contains("1. I1 — Fix data loss"));
    assert!(human.contains("judge_failed"));
    assert!(human.contains("Skipped at rank.compare: @b"));
    assert!(full.contains("Win matrix and fitted strengths"));
    assert!(full.contains("All judgments"));
    assert!(!full.contains("promptRefs") && !full.contains("models"));
    assert_eq!(full, report_text(&r, true));
}
#[test]
fn snapshot_export_requires_matching_contest_and_human_audience() {
    let snapshot = ExportSnapshot::default();
    assert!(render(&snapshot, ContestId::new()).is_err());
}
