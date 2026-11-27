use squad_core::{orchestrator::negotiate::*, router::negotiate::steps};
use squad_protocol::{negotiate::*, *};
fn table(outcome: NegotiateOutcome, rounds_used: u32) -> NegotiateTable {
    NegotiateTable {
        table: 1,
        negotiator: "a".parse().unwrap(),
        counterpart: "b".parse().unwrap(),
        rounds_used,
        outcome: Some(outcome),
        turns: vec![],
    }
}
#[test]
fn hand_computed_call_attempt_and_bound_totals() {
    for (m, r, c, a) in [
        (6, 3, 29, 60),
        (4, 3, 20, 42),
        (12, 3, 41, 84),
        (5, 1, 14, 30),
    ] {
        assert_eq!((calls(m, r), attempts(m, r)), (c, a));
        let roster = (0..m)
            .map(|i| format!("m{i}").parse().unwrap())
            .collect::<Vec<_>>();
        let s = steps(&roster, "our side", "counterpart", &[10; 6], 500, r);
        assert_eq!(s.iter().map(|s| s.calls).sum::<u32>(), c);
        assert_eq!(s.iter().map(|s| s.calls * s.attempts).sum::<u32>(), a);
    }
}
#[test]
fn hand_computed_outcomes_and_reservation_release() {
    let ts = [
        table(NegotiateOutcome::Deal, 1),
        table(NegotiateOutcome::NoDeal, 2),
        table(NegotiateOutcome::Unfinished, 3),
        table(NegotiateOutcome::Abandoned, 1),
    ];
    let t = tally(&ts);
    for k in ["deal", "no_deal", "unfinished", "abandoned"] {
        assert_eq!(t[k], 1);
    }
    assert_eq!(unused_turns(&ts, 3), 5);
    assert_eq!(released_calls(&ts, 3, true), (5, 5));
    assert_eq!(
        released_calls(&[table(NegotiateOutcome::Abandoned, 1)], 3, false),
        (2, 3)
    );
    // 2 attempts each, move share=7, counter share=11: 94 released micros.
    let (m, c) = released_calls(&[table(NegotiateOutcome::Abandoned, 1)], 3, false);
    assert_eq!(2 * (m * 7 + c * 11), 94);
    assert_eq!(
        outcome(NegotiateResponse::Accept, 1, 3),
        Some(NegotiateOutcome::Deal)
    );
    assert_eq!(
        outcome(NegotiateResponse::WalkAway, 1, 3),
        Some(NegotiateOutcome::NoDeal)
    );
    assert_eq!(outcome(NegotiateResponse::Counter, 2, 3), None);
    assert_eq!(
        outcome(NegotiateResponse::Reject, 3, 3),
        Some(NegotiateOutcome::Unfinished)
    );
}
#[test]
fn role_draw_has_unique_disjoint_players() {
    for m in 4..=12 {
        let ns = (0..m)
            .map(|i| format!("m{i}").parse().unwrap())
            .collect::<Vec<_>>();
        let ts = draw(&ns);
        assert_eq!(ts.len(), table_count(m));
        let roles = ts
            .iter()
            .flat_map(|t| [&t.negotiator, &t.counterpart])
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(roles.len(), 2 * table_count(m));
        assert!(roles.iter().all(|n| ns.contains(n)));
    }
}
