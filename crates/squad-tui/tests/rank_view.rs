use squad_protocol::*;
use squad_tui::widgets::rank::{View, render};
#[test]
fn rank_progress_replay_and_state_projection() {
    let mut v = View::default();
    let contest = ContestId::new();
    let state = RankState {
        contest_id: contest,
        state_version: 2,
        state: RankStatus::Comparing,
        step_id: Some(StepId::new()),
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    v.apply(EventId::new(), &EnvelopePayload::RankState(state));
    let id = EventId::new();
    let a = ContestArtifact {
        contest_id: contest,
        step_id: StepId::new(),
        artifact_kind: "rank.compare".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Rank),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::json!({"choices":[{"pairId":"P1","winner":"A","reason":"Impact"}]})),
    };
    v.apply(id, &EnvelopePayload::ContestArtifact(a.clone()));
    v.apply(id, &EnvelopePayload::ContestArtifact(a));
    let text = render(&v).join("\n");
    assert!(text.contains("RANK"));
    assert!(text.contains("COMPARING 1 judgments returned"));
    assert_eq!(v.artifacts.len(), 1);
}
#[test]
fn fitted_order_has_gap_markers_and_cycle_section() {
    use squad_protocol::rank::*;
    use std::collections::BTreeMap;
    let participants = vec![
        "a".parse().unwrap(),
        "b".parse().unwrap(),
        "c".parse().unwrap(),
    ];
    let (pairs, votes_per_pair, deal) = squad_core::orchestrator::rank::design(3, &participants, 2);
    let tally =
        squad_core::orchestrator::rank::fit(&[vec![0, 3, 3], vec![0, 0, 1], vec![0, 1, 0]], &pairs);
    let r = RankReport {
        setup: RankSetup {
            criterion: "Impact".into(),
            items: (1..=3)
                .map(|i| RankItem {
                    item_id: format!("I{i}"),
                    text: format!("Item {i}"),
                })
                .collect(),
            participants,
            pairs,
            votes_per_pair,
            deal,
            models: BTreeMap::new(),
            prompt_refs: BTreeMap::new(),
            room_words: 500,
        },
        batches: BTreeMap::new(),
        tally: Some(tally),
        explanation: None,
        skipped: vec![],
        reason: "ranked".into(),
    };
    let a = ContestArtifact {
        contest_id: ContestId::new(),
        step_id: StepId::new(),
        artifact_kind: "rank.report".parse().unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Rank),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::to_value(r).unwrap()),
    };
    let mut v = View::default();
    v.apply(EventId::new(), &EnvelopePayload::ContestArtifact(a));
    let text = render(&v).join("\n");
    assert!(text.contains("ORDER") && text.contains("CYCLES"));
    assert!(text.contains("────────    clear"));
    assert!(text.contains("········    close"));
}

#[test]
fn new_contest_clears_cached_report() {
    let old = RankState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: RankStatus::Complete,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: Some("ranked".parse().unwrap()),
    };
    let mut view = View {
        state: Some(old),
        report_lines: Some(vec!["old report".into()]),
        ..Default::default()
    };
    let next = RankState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: RankStatus::Comparing,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    view.apply(EventId::new(), &EnvelopePayload::RankState(next));
    assert!(!render(&view).join("\n").contains("old report"));
}
