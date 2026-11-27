use squad_core::orchestrator::forge::tally;
use squad_protocol::{forge::*, *};
use std::collections::BTreeMap;
fn n(s: &str) -> MemberNick {
    s.parse().unwrap()
}
fn drafts() -> BTreeMap<MemberNick, ForgeDraft> {
    ["a", "b", "c"]
        .map(|s| {
            (
                n(s),
                ForgeDraft {
                    body: format!("Draft by {s}"),
                },
            )
        })
        .into()
}
fn ballot(
    voter: &str,
    order: [&str; 2],
    flaw: Option<&str>,
) -> (MemberNick, ForgeBallot, BTreeMap<String, MemberNick>) {
    let labels = vec![order[0].to_string(), order[1].to_string()];
    (
        n(voter),
        ForgeBallot {
            ranking: labels.clone(),
            notes: labels
                .iter()
                .map(|l| ForgeNote {
                    label: l.clone(),
                    steal: "Clear".into(),
                    fatal_flaw: (Some(l.as_str()) == flaw).then(|| "Vague".into()),
                })
                .collect(),
        },
        labels.iter().map(|l| (l.clone(), n(l))).collect(),
    )
}
fn ties() -> BTreeMap<MemberNick, String> {
    [
        (n("a"), "z".into()),
        (n("b"), "y".into()),
        (n("c"), "x".into()),
    ]
    .into()
}
#[test]
fn complete_and_missing_ballots_hand_calculated() {
    let b = vec![
        ballot("a", ["b", "c"], None),
        ballot("b", ["a", "c"], None),
        ballot("c", ["a", "b"], None),
    ];
    let t = tally(&drafts(), &b, &ties());
    assert_eq!(
        t.scores
            .iter()
            .map(|s| (s.author.to_string(), s.score))
            .collect::<Vec<_>>(),
        vec![("a".into(), 1.), ("b".into(), 0.5), ("c".into(), 0.)]
    );
    assert_eq!(t.max_words, 3);
    assert_eq!(t.fusers, vec![n("a"), n("b")]);
    let t = tally(&drafts(), &b[..2], &ties());
    assert_eq!(t.scores[0].score, 1.);
    assert_eq!(t.scores[1].score, 1.);
}
#[test]
fn ties_use_flaw_voters_then_persisted_private_seed() {
    let mut b = vec![
        ballot("a", ["b", "c"], None),
        ballot("b", ["c", "a"], None),
        ballot("c", ["a", "b"], None),
    ];
    let t = tally(&drafts(), &b, &ties());
    assert_eq!(t.best_original, n("c"));
    b[1].1.notes[0].fatal_flaw = Some("Vague".into());
    let t = tally(&drafts(), &b, &ties());
    assert_eq!(t.best_original, n("b"));
}
#[test]
fn first_places_break_equal_means() {
    // a gets 1 and 0 (mean .5, one first); b gets .5 and .5 (mean .5, no first).
    let mut ds = drafts();
    ds.insert(
        n("d"),
        ForgeDraft {
            body: "Fourth".into(),
        },
    );
    let make = |v: &str, rank: Vec<&str>| {
        let b = ForgeBallot {
            ranking: rank.iter().map(|s| s.to_string()).collect(),
            notes: vec![],
        };
        (
            n(v),
            b,
            ["a", "b", "c", "d"]
                .map(|s| (s.into(), n(s)))
                .into_iter()
                .collect(),
        )
    };
    let b = vec![
        make("c", vec!["a", "b", "d"]),
        make("d", vec!["c", "b", "a"]),
    ];
    let mut ts = ties();
    ts.insert(n("d"), "w".into());
    let t = tally(&ds, &b, &ts);
    assert!(
        t.scores.iter().position(|s| s.author == n("a"))
            < t.scores.iter().position(|s| s.author == n("b"))
    );
}
#[test]
fn six_seat_reservation_table() {
    let p = (0..6).map(|i| n(&format!("m{i}"))).collect::<Vec<_>>();
    let s = squad_core::router::forge::steps(&p, "brief", "constraints", &[100; 4], (500, 1200));
    assert_eq!(s.iter().map(|s| s.calls).sum::<u32>(), 15);
    assert_eq!(s.iter().map(|s| s.calls * s.attempts).sum::<u32>(), 31);
}
