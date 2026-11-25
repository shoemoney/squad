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
            finder_count: 1,
            priority: None,
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
    assert_eq!(next.estimate_high.map(|h| (h * 1e3).round()), Some(23709.0));
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
#[test]
fn canonical_grill_plan_text_is_available_to_the_census_source_adapter() {
    let plan = serde_json::json!({"kind":"grill.plan","payload":{"contestId":"01900000-0000-7000-8000-000000000001","round":1,"version":2,"leader":"member0","text":"Complete rebuilt plan"}});
    assert_eq!(
        grill_plan_text(&plan).as_deref(),
        Some("Complete rebuilt plan")
    );
    assert!(grill_plan_text(&serde_json::json!({"kind":"contest.artifact","payload":{"text":"not an admitted plan"}})).is_none());
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} vs {b}");
}
fn confirmed(i: usize, finders: usize) -> CensusGapOutcome {
    let mut g = gap(&format!("g{i}"));
    g.omission = text(&format!("Missing item {i}"));
    CensusGapOutcome {
        gap: g,
        disposition: CensusDisposition::Confirmed,
        votes: 3,
        covered: 0,
        trivial: 0,
        finders: (0..finders)
            .map(|m| format!("member{m}").parse().unwrap())
            .collect(),
        finder_count: finders as u32,
        priority: None,
        citations: vec![],
    }
}
fn pass_report(finder_counts: &[usize], prior: &[CensusReport]) -> CensusReport {
    let gaps = finder_counts
        .iter()
        .enumerate()
        .map(|(i, n)| confirmed(i, *n))
        .collect();
    report(ContestId::new(), 1, 1, gaps, vec![], vec![], prior)
}

#[test]
fn range_f2_positive_hand_computed() {
    // f1=4, f2=2: U=4, var=2*(0.5*4+8+0.25*16)=28, K=exp(1.96*sqrt(ln(2.75)))
    let (lo, hi) = census_range(4, 2, 6).unwrap();
    close(lo, 0.5570826032270546);
    close(hi, 28.72105484413907);
    let r = pass_report(&[1, 1, 1, 1, 2, 2], &[]);
    assert_eq!((r.f1, r.f2, r.unseen), (4, 2, 4.0));
    close(r.estimate_low.unwrap(), lo);
    close(r.estimate_high.unwrap(), hi);
    assert_eq!(
        r.headline(),
        "Found 6; estimated 0.6 to 28.7 still unfound."
    );
    r.validate().unwrap();
}
#[test]
fn range_zero_doubletons_hand_computed() {
    // f1=3, f2=0, S_obs=3: U=3, S^=6, var=3+3*25/4-81/24=18.375
    let (lo, hi) = census_range(3, 0, 3).unwrap();
    close(lo, 0.379609466497077);
    close(hi, 23.708576298292343);
    let r = pass_report(&[1, 1, 1], &[]);
    assert_eq!(r.formula_branch.as_str(), "zero_doubletons");
    assert_eq!(
        r.headline(),
        "Found 3; estimated 0.4 to 23.7 still unfound."
    );
    r.validate().unwrap();
}
#[test]
fn range_zero_unseen_and_nothing_found() {
    assert_eq!(census_range(0, 0, 4), Some((0.0, 0.0)));
    assert_eq!(census_range(1, 0, 1), Some((0.0, 0.0)));
    assert_eq!(census_range(0, 0, 0), None);
    let none = pass_report(&[], &[]);
    assert_eq!((none.estimate_low, none.estimate_high), (None, None));
    assert_eq!(none.headline(), "Nothing found yet.");
    assert!(!none.quiet());
    none.validate().unwrap();
    let zero = pass_report(&[3, 3], &[]);
    assert_eq!(
        (zero.estimate_low, zero.estimate_high),
        (Some(0.0), Some(0.0))
    );
    assert_eq!(
        zero.headline(),
        "Found 2; estimated 0.0 to 0.0 still unfound."
    );
}
#[test]
fn quiet_boundaries() {
    let first = pass_report(&[3, 3], &[]);
    assert!(!first.quiet(), "first pass always finds new gaps");
    let mut quiet = pass_report(&[3, 3], std::slice::from_ref(&first));
    assert_eq!(quiet.discovery[1].new_confirmed, 0);
    assert!(quiet.quiet());
    quiet.estimate_high = Some(1.999);
    assert!(quiet.quiet());
    quiet.estimate_high = Some(2.0);
    assert!(!quiet.quiet());
    quiet.estimate_high = Some(0.0);
    quiet.missing = vec![CensusMissing {
        member: "member0".parse().unwrap(),
        step: text("census.hunt"),
    }];
    assert!(!quiet.quiet());
    quiet.missing.clear();
    quiet.gaps[0].disposition = CensusDisposition::Unresolved;
    assert!(!quiet.quiet());
    quiet.gaps[0].disposition = CensusDisposition::Confirmed;
    assert!(quiet.quiet());
    quiet.discovery[1].new_confirmed = 1;
    assert!(!quiet.quiet());
    let mut grown = pass_report(&[3, 3, 3], std::slice::from_ref(&first));
    assert_eq!(grown.discovery[1].new_confirmed, 1);
    assert!(!grown.quiet());
    grown.estimate_high = None;
    assert!(!grown.quiet());
}
