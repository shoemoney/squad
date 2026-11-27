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
    let mut r = report();
    let pair = r
        .setup
        .pairs
        .iter()
        .find(|p| (p.left, p.right) == (1, 2))
        .unwrap();
    r.tally.as_mut().unwrap().split_pairs = vec![pair.pair_id.clone()];
    let human = report_text(&r, false);
    assert!(human.contains("I2 vs I3 (2–1)"));
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

fn event(payload: serde_json::Value) -> CommittedEnvelope {
    serde_json::from_value(serde_json::json!({"version":1,"id":EventId::new(),"roomId":"main","correlationId":EventId::new(),"sender":"system","recipients":["me"],"createdAt":"2026-11-26T00:00:00Z","seq":"1","acceptedAt":"2026-11-26T00:00:00Z","commitIndex":0,"commitSize":1,"kind":"contest.artifact","payload":payload})).unwrap()
}
#[test]
fn full_export_maps_raw_judgments_and_honors_visibility() {
    let mut r = report();
    let nick = r.setup.participants[0].clone();
    let d = &r.setup.deal[&nick][0];
    r.batches.insert(
        nick,
        RankBatch {
            choices: vec![RankChoice {
                pair_id: d.pair_id.clone(),
                winner: RankWinner::B,
                reason: "More impact".into(),
            }],
        },
    );
    let id = ContestId::new();
    let a = ContestArtifact {
        contest_id: id,
        step_id: StepId::new(),
        artifact_kind: "rank.report".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Rank),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::to_value(&r).unwrap()),
    };
    let mut snapshot = ExportSnapshot {
        events: vec![event(serde_json::to_value(&a).unwrap())],
        ..Default::default()
    };
    let output = render(&snapshot, id).unwrap();
    assert!(output.contains("winner=B; More impact"));
    for leak in ["Some(", "Member(", "Nick("] {
        assert!(!output.contains(leak), "export leaks {leak}: {output}");
    }
    assert!(output.contains("A=I") && output.contains("B=I"));
    assert!(render(&snapshot, ContestId::new()).is_err());
    let mut hidden = a;
    hidden.audience = Some(ArtifactAudience::Case);
    snapshot.events = vec![event(serde_json::to_value(hidden).unwrap())];
    assert!(render(&snapshot, id).is_err());
}
