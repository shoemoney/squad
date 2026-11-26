use squad_core::orchestrator::tune::*;
use squad_protocol::{tune::*, *};
use std::collections::BTreeMap;
fn candidate(id: &str, g: u32, prompt: &str) -> TuneCandidate {
    TuneCandidate {
        id: id.into(),
        parent: None,
        generation: g,
        author: None,
        prompt: prompt.into(),
        changes: vec![],
        incomplete: false,
    }
}
fn case(i: usize) -> TuneFrozenCase {
    TuneFrozenCase {
        label: i.to_string(),
        source_ref: format!("P{i}:a"),
        case: TuneTestCase {
            local_id: "a".into(),
            kind: TuneCaseKind::Typical,
            input: format!("input {i}"),
            expect: vec!["works".into()],
            must_not: vec![],
        },
    }
}
#[test]
fn frozen_split_and_rotation_are_stable_and_balanced() {
    for (n, t, h) in [(4, 2, 2), (5, 3, 2), (6, 3, 3)] {
        let cases: Vec<_> = (0..n).map(case).collect();
        let partition = split(&cases, "secret");
        assert_eq!((partition.0.len(), partition.1.len()), (t, h));
        assert_eq!(partition, split(&cases, "secret"));
        assert!(
            partition
                .0
                .iter()
                .all(|t| partition.1.iter().all(|h| t.source_ref != h.source_ref))
        );
    }
    for m in [3, 4, 12] {
        let n: Vec<MemberNick> = (0..m)
            .map(|i| format!("member{i}").parse().unwrap())
            .collect();
        let mut counts = BTreeMap::new();
        for g in 1..=6 {
            let chosen = mutators(&n, g);
            assert_eq!(chosen.len(), 3);
            for n in chosen {
                *counts.entry(n).or_insert(0) += 1;
            }
        }
        assert!(counts.values().max().unwrap() - counts.values().min().unwrap() <= 1);
    }
}
#[test]
fn arithmetic_is_unrounded_and_comparison_is_within_one_sheet() {
    let sheet = TuneScoreSheet {
        cases: vec![
            TuneCaseScores {
                case_label: "T1".into(),
                outputs: vec![TuneOutputScore {
                    label: "A".into(),
                    scores: vec![
                        TuneCriterionScore {
                            criterion: "x".into(),
                            score: 1,
                        },
                        TuneCriterionScore {
                            criterion: "y".into(),
                            score: 4,
                        },
                    ],
                    note: "partial".into(),
                }],
            },
            TuneCaseScores {
                case_label: "T2".into(),
                outputs: vec![TuneOutputScore {
                    label: "B".into(),
                    scores: vec![
                        TuneCriterionScore {
                            criterion: "x".into(),
                            score: 2,
                        },
                        TuneCriterionScore {
                            criterion: "y".into(),
                            score: 4,
                        },
                    ],
                    note: "partial".into(),
                }],
            },
        ],
    };
    let maps = BTreeMap::from([
        ("T1".into(), BTreeMap::from([("A".into(), "c".into())])),
        ("T2".into(), BTreeMap::from([("B".into(), "c".into())])),
    ]);
    assert_eq!(scores(&sheet, &maps)["c"], 11.0 / 16.0);
}
#[test]
fn promotion_threshold_tie_breaks_incomplete_and_empty() {
    let mut cs = vec![
        candidate("c0", 0, "original"),
        candidate("a", 1, "longer prompt"),
        candidate("b", 1, "short"),
    ];
    for (gain, promoted) in [(0.04, false), (0.05, true)] {
        let s = BTreeMap::from([
            ("c0".into(), 0.5),
            ("a".into(), 0.5 + gain),
            ("b".into(), 0.5 + gain),
        ]);
        let g = promotion(&cs, 1, "c0", &s, "seed");
        assert_eq!(g.promoted, promoted);
        assert_eq!(g.best.as_deref(), Some("b"));
    }
    cs[2].incomplete = true;
    let s = BTreeMap::from([("c0".into(), 0.5), ("a".into(), 0.75), ("b".into(), 1.0)]);
    assert_eq!(
        promotion(&cs, 1, "c0", &s, "seed").best.as_deref(),
        Some("a")
    );
    assert_eq!(promotion(&cs, 2, "c0", &s, "seed").best, None);
    cs[2].incomplete = false;
    cs[2].prompt = cs[1].prompt.clone();
    let mut s = s;
    s.insert("b".into(), 0.75);
    assert_eq!(
        promotion(&cs, 1, "c0", &s, "seed"),
        promotion(&cs, 1, "c0", &s, "seed")
    );
}
#[test]
fn heldout_alone_picks_with_earlier_champion_ties() {
    let finalists = vec!["early".into(), "late".into()];
    for (a, b, expected) in [
        (0.54, 0.54, "c0"),
        (0.55, 0.54, "early"),
        (0.55, 0.75, "late"),
        (0.75, 0.75, "early"),
    ] {
        let scores = BTreeMap::from([("c0".into(), 0.5), ("early".into(), a), ("late".into(), b)]);
        assert_eq!(final_pick("c0", &finalists, &scores), expected);
    }
}
#[test]
fn leak_normalization_includes_unicode_quotes_dashes_and_short_words() {
    assert!(note_leaks(
        "The ‘first’ two—three four five six seven",
        "‘first’ two—three four five six seven here"
    ));
    assert!(note_leaks("a b c d e f", "a\nb\tc d e f"));
    assert!(note_leaks("Ａ b c d e f", "A b c d e f"));
    assert!(!note_leaks("A b c d e f", "a b c d e f"));
}
#[test]
fn cap_counts_distinguish_member_executor_and_judge_attempts() {
    let pack = squad_core::prompts::PromptPack::from_override_dir(None).unwrap();
    for (m, g, calls, attempts) in [(6, 3, 60, 126), (3, 1, 31, 66)] {
        let participants: Vec<MemberNick> = (0..m)
            .map(|i| format!("member{i}").parse().unwrap())
            .collect();
        let setup = TuneSetup {
            prompt: "prompt".into(),
            task: "task".into(),
            rotation: participants.clone(),
            participants,
            executor: ExecutorBinding {
                model: Text::new("fixture/model").unwrap(),
                effort: ReasoningEffort::Low,
            },
            generations: g,
            length_budget: 151,
            seed: "seed".into(),
        };
        let steps = squad_core::router::tune::steps(&pack, &setup, 300).unwrap();
        assert_eq!(steps.iter().map(|s| s.calls).sum::<u32>(), calls);
        assert_eq!(
            steps.iter().map(|s| s.calls * s.attempts).sum::<u32>(),
            attempts
        );
        assert_eq!(
            steps
                .iter()
                .find(|s| s.kind == WorkKind::TuneExecute)
                .unwrap()
                .calls,
            9 * u32::from(g) + 12
        );
    }
}
