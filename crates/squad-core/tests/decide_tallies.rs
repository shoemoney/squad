use squad_core::orchestrator::decide::*;
use squad_protocol::decide::*;
use std::collections::BTreeMap;
fn criterion(id: &str, weight: u32) -> DecideCriterion {
    DecideCriterion {
        local_id: id.into(),
        name: id.into(),
        definition: "Higher is better".into(),
        weight,
    }
}
fn canonical(id: &str, labels: &[&str]) -> DecideCanonicalCriterion {
    DecideCanonicalCriterion {
        criterion_id: id.into(),
        name: id.into(),
        definition: "Higher is better".into(),
        sources: labels
            .iter()
            .map(|l| DecideSource {
                member_label: l.to_string(),
                local_id: id.into(),
            })
            .collect(),
    }
}
fn rubric_two(weight: f64) -> DecideRubric {
    DecideRubric {
        kept: [("c1", weight), ("c2", 1.0 - weight)]
            .map(|(id, w)| DecideWeightedCriterion {
                criterion: canonical(id, &["a", "b"]),
                support: 2,
                mean_weight: w * 100.0,
                weight: w,
            })
            .to_vec(),
        dropped: vec![],
    }
}
fn options() -> Vec<DecideOption> {
    ["a", "b"]
        .map(|s| DecideOption {
            option_id: s.into(),
            name: s.into(),
            description: String::new(),
        })
        .to_vec()
}
fn sheet(scores: [u8; 4]) -> DecideScores {
    DecideScores {
        cells: [("a", "c1"), ("a", "c2"), ("b", "c1"), ("b", "c2")]
            .iter()
            .zip(scores)
            .map(|((o, c), score)| DecideCell {
                option_id: o.to_string(),
                criterion_id: c.to_string(),
                score,
                evidence: "Evidence".into(),
            })
            .collect(),
    }
}
#[test]
fn median_totals_leave_out_and_flip_match_hand_math() {
    let r = rubric_two(0.6);
    let m = matrix(&options(), &r, &[sheet([9, 4, 4, 8]), sheet([7, 4, 4, 8])]).unwrap();
    assert_eq!(m.cells[0].median, 8.0);
    assert!((m.ranking[0].total - 6.4).abs() < 1e-12);
    assert!((m.ranking[1].total - 5.6).abs() < 1e-12);
    assert!((m.margin - 0.8).abs() < 1e-12);
    assert_eq!(m.leave_one_out[0].ranking[0].option_id, "b");
    assert_eq!(m.leave_one_out[1].ranking[0].option_id, "a");
    assert_eq!(m.flips.len(), 2);
    for f in &m.flips {
        assert!((f.threshold - 0.5).abs() < 1e-12);
    }
    assert!(!m.robust);
}
#[test]
fn close_no_flip_contested_and_outlier() {
    let r = rubric_two(0.6);
    let s = sheet([8, 4, 4, 8]);
    let m = matrix(
        &options(),
        &r,
        &[s.clone(), s.clone(), sheet([0, 10, 10, 0])],
    )
    .unwrap();
    assert_eq!(m.cells[0].median, 8.0);
    assert!(m.cells.iter().all(|c| c.contested));
    let close = matrix(&options(), &rubric_two(0.525), &[s.clone(), s]).unwrap();
    assert!((close.margin - 0.2).abs() < 1e-12);
    assert!(close.winner.is_none());
    let dominant = matrix(&options(), &r, &[sheet([8, 9, 4, 5]), sheet([8, 9, 4, 5])]).unwrap();
    assert!(dominant.flips.is_empty());
    assert!(dominant.robust);
}
#[test]
fn singleton_dropped_cap_seven_and_normalization() {
    let lists = BTreeMap::from([
        (
            "a".into(),
            DecideCriteria {
                criteria: [
                    ("c1", 20),
                    ("c2", 20),
                    ("c3", 20),
                    ("c4", 20),
                    ("c5", 19),
                    ("c8", 1),
                ]
                .map(|(id, w)| criterion(id, w))
                .to_vec(),
            },
        ),
        (
            "b".into(),
            DecideCriteria {
                criteria: [
                    ("c1", 20),
                    ("c2", 20),
                    ("c3", 20),
                    ("c6", 20),
                    ("c7", 19),
                    ("c8", 1),
                ]
                .map(|(id, w)| criterion(id, w))
                .to_vec(),
            },
        ),
        (
            "c".into(),
            DecideCriteria {
                criteria: [
                    ("c4", 25),
                    ("c5", 25),
                    ("c6", 25),
                    ("c7", 24),
                    ("singleton", 1),
                ]
                .map(|(id, w)| criterion(id, w))
                .to_vec(),
            },
        ),
    ]);
    for list in lists.values() {
        list.validate().unwrap();
    }
    let merged = DecideFreeze {
        criteria: (1..=8)
            .map(|i| {
                canonical(
                    &format!("c{i}"),
                    match i {
                        1..=3 | 8 => &["a", "b"],
                        4..=5 => &["a", "c"],
                        _ => &["b", "c"],
                    },
                )
            })
            .chain(std::iter::once(canonical("singleton", &["c"])))
            .collect(),
    };
    let r = rubric(&lists, &merged).unwrap();
    assert_eq!(r.kept.len(), 7);
    assert_eq!(r.dropped.len(), 2);
    assert!((r.kept.iter().map(|x| x.weight).sum::<f64>() - 1.0).abs() < 1e-12);
    assert!(
        r.dropped
            .iter()
            .any(|x| x.criterion.criterion_id == "singleton")
    );
    assert!(r.dropped.iter().any(|x| x.criterion.criterion_id == "c8"));
}
#[test]
fn quorum_and_call_counts() {
    assert_eq!(counts(6, false), (14, 30));
    assert_eq!(counts(6, true), (21, 45));
    assert_eq!(
        matrix(&options(), &rubric_two(0.6), &[sheet([8, 4, 4, 8])]).unwrap_err(),
        "insufficient_scores"
    );
    assert_eq!(
        rubric(&BTreeMap::new(), &DecideFreeze { criteria: vec![] }).unwrap_err(),
        "insufficient_criteria"
    );
}
