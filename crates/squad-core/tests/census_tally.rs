use squad_core::orchestrator::census::*;
use squad_protocol::*;
use std::collections::BTreeMap;
fn text(s: &str) -> Text {
    Text::new(s).unwrap()
}
fn gap(id: &str) -> CensusGap {
    CensusGap {
        id: text(id),
        omission: text("Missing rollback"),
        consequence: text("Outage"),
        lenses: vec![text("failure modes")],
        source_find_ids: vec![text("a"), text("b")],
    }
}
fn rating(vote: CensusVote) -> CensusRatingOutput {
    CensusRatingOutput {
        ratings: vec![CensusGapRating {
            gap_id: text("gap"),
            rating: vote,
            reason: text("evidence"),
            citation: None,
        }],
        additions: vec![],
    }
}
#[test]
fn majority_missing_ballots_and_distinct_initial_finders() {
    let ledger = CensusMergeOutput {
        ledger: vec![gap("gap")],
    };
    let finders = BTreeMap::from([(
        "gap".into(),
        vec![
            "member0".parse().unwrap(),
            "member0".parse().unwrap(),
            "member1".parse().unwrap(),
        ],
    )]);
    let tied = outcomes(
        &ledger,
        &[rating(CensusVote::Real), rating(CensusVote::Covered)],
        3,
        &finders,
    );
    assert_eq!(tied[0].disposition, CensusDisposition::Unresolved);
    assert_eq!(tied[0].finders.len(), 2);
    let excluded = outcomes(
        &ledger,
        &[
            rating(CensusVote::Trivial),
            rating(CensusVote::Real),
            rating(CensusVote::Covered),
        ],
        3,
        &finders,
    );
    assert_eq!(excluded[0].disposition, CensusDisposition::Covered);
    assert_eq!(excluded[0].votes, 3);
    let supported = outcomes(
        &ledger,
        &[
            rating(CensusVote::Real),
            rating(CensusVote::Real),
            rating(CensusVote::Trivial),
        ],
        3,
        &finders,
    );
    assert_eq!(supported[0].disposition, CensusDisposition::Confirmed);
    let id = ContestId::new();
    let r = report(id, 1, 1, supported, vec![], vec![], &[]);
    assert_eq!(r.f2, 1);
    assert_eq!(r.f1, 0);
    r.validate().unwrap();
    assert!(!r.quiet());
}
#[test]
fn singleton_zero_doubleton_estimate_and_cross_pass_identity() {
    let mut gaps = vec![];
    for i in 0..3 {
        let mut g = gap(&format!("g{i}"));
        g.omission = text(&format!("Missing item {i}"));
        gaps.push(CensusGapOutcome {
            gap: g,
            disposition: CensusDisposition::Confirmed,
            votes: 3,
            covered: 0,
            trivial: 0,
            finders: vec!["member0".parse().unwrap()],
            citations: vec![],
        });
    }
    let id = ContestId::new();
    let r = report(id, 1, 1, gaps.clone(), vec![], vec![], &[]);
    assert_eq!((r.f1, r.f2, r.unseen), (3, 0, 3.0));
    r.validate().unwrap();
    let next = report(id, 2, 2, gaps, vec![], vec![], &[r]);
    assert_eq!(next.discovery[1].new_confirmed, 0);
    assert_eq!(next.discovery[1].cumulative_confirmed, 3);
    assert_eq!(next.estimate_high, None);
    assert!(!next.quiet());
}
#[test]
fn redaction_uses_unicode_boundaries_and_leaves_originals() {
    let id = ContestId::new();
    let original = "MEMBER0, member01 and Provider/Family-v2. Quiet Persona. café!";
    let setup = CensusSetup {
        contest_id: id,
        plan: CensusPlan {
            contest_id: id,
            version: 1,
            text: text(original),
            source_event_ids: vec![],
        },
        participants: (0..6)
            .map(|i| format!("member{i}").parse().unwrap())
            .collect(),
        passes: 2,
        room_words: 200,
        plan_words: 500,
        models: BTreeMap::from([(
            "judge".into(),
            ExecutorBinding {
                model: text("Provider/Family-v2"),
                effort: ReasoningEffort::Medium,
            },
        )]),
        prompt_refs: BTreeMap::new(),
        redaction_tokens: vec!["Quiet Persona".into(), "café".into()],
    };
    let redacted = blind_text(original, &setup);
    assert_eq!(
        redacted,
        "[redacted], member01 and [redacted]. [redacted]. [redacted]!"
    );
    assert_eq!(setup.plan.text.as_str(), original);
    assert_eq!(setup.normal_calls(), 28);
    assert_eq!(setup.reserved_calls(), 60);
    assert_eq!(
        script(&setup)
            .iter()
            .filter(|s| s.composition.is_some())
            .count(),
        8
    );
}
