use squad_core::orchestrator::rank::*;
use squad_protocol::{rank::*, *};
#[test]
fn seeded_connected_design_and_balanced_deal() {
    let members = (0..6)
        .map(|i| format!("m{i}").parse::<MemberNick>().unwrap())
        .collect::<Vec<_>>();
    for (n, q, v) in [(3, 3, 3), (7, 21, 3), (8, 24, 3), (40, 120, 3)] {
        let (pairs, votes, deal) = design(n, &members, 42);
        assert_eq!((pairs.len(), votes), (q, v));
        assert_eq!(
            (pairs.clone(), votes, deal.clone()),
            design(n, &members, 42)
        );
        let loads = deal.values().map(Vec::len).collect::<Vec<_>>();
        assert!(loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1);
        assert!(*loads.iter().max().unwrap() <= 60);
        for ds in deal.values() {
            let mut ids = ds.iter().map(|d| &d.pair_id).collect::<Vec<_>>();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), ds.len());
        }
        for p in &pairs {
            assert_eq!(
                deal.values()
                    .filter(|ds| ds.iter().any(|d| d.pair_id == p.pair_id))
                    .count(),
                v
            );
        }
        let mut w = vec![vec![0; n]; n];
        for p in &pairs {
            w[p.left][p.right] = 1;
        }
        assert_eq!(fit(&w, &pairs).components.len(), 1);
        let a = deal.values().flatten().filter(|d| d.a < d.b).count();
        assert!(a > 0 && a < v * q);
    }
    assert_eq!(call_bounds(6), (7, 15));
}
#[test]
fn balanced_complete_set_and_phantom_have_unit_strength() {
    let w = vec![vec![0, 1, 1], vec![1, 0, 1], vec![1, 1, 0]];
    let t = fit(&w, &[]);
    assert_eq!(t.strengths, vec![1.0; 3]);
    assert_eq!(t.order, vec![0, 1, 2]);
    assert!(
        t.gaps
            .iter()
            .all(|g| g.label == "close" && g.probability == 0.5)
    );
    assert_eq!(fit(&vec![vec![0; 3]; 3], &[]).strengths, vec![1.0; 3]);
}
#[test]
fn undefeated_and_winless_pair_match_analytic_strengths() {
    // Independent 60-digit decimal MM golden at the required log-delta stop (87 updates).
    // The equilibrium solves p^3 - p^2 - p - 3 = 0; q=1/p.
    let t = fit(&[vec![0, 1], vec![0, 0]], &[]);
    assert!((t.strengths[0] - 2.130395426518053).abs() < 1e-9);
    assert!((t.strengths[1] - 0.4693964235759998).abs() < 1e-9);
    assert_eq!(t.order, vec![0, 1]);
    assert_eq!(t.gaps[0].label, "clear");
}
#[test]
fn cycle_split_and_disconnection_are_preserved() {
    let pairs = vec![RankPair {
        pair_id: "P1".into(),
        left: 0,
        right: 1,
    }];
    let w = vec![
        vec![0, 2, 0, 0],
        vec![1, 0, 2, 0],
        vec![2, 0, 0, 0],
        vec![0, 0, 0, 0],
    ];
    let t = fit(&w, &pairs);
    assert_eq!(t.cycle_count, 1);
    assert_eq!(t.cycles[0].items, [0, 1, 2]);
    assert_eq!(t.split_pairs, vec!["P1"]);
    assert_eq!(t.components, vec![vec![0, 1, 2], vec![3]]);
    let other = fit(&w, &pairs);
    assert_eq!(
        t.strengths.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        other
            .strengths
            .iter()
            .map(|x| x.to_bits())
            .collect::<Vec<_>>()
    );
    let mut even = w;
    even[1][0] = 2;
    assert_eq!(fit(&even, &pairs).cycle_count, 0);
}
#[test]
fn input_limits_are_enforced() {
    use squad_protocol::parser::ConversationalInput;
    let p = |n, word: String| ParsedInput::Rank {
        input: ConversationalInput {
            raw: String::new(),
            quote: vec![],
            variable_updates: vec![],
        },
        participants: vec![],
        criterion: "impact".into(),
        items: vec![word; n],
    };
    assert!(prepare(&p(2, "short".into())).is_err());
    assert!(prepare(&p(41, "short".into())).is_err());
    assert!(prepare(&p(3, "word ".repeat(61))).is_err());
    assert!(prepare(&p(3, "short".into())).is_ok());
}
#[test]
fn two_member_replication_never_assigns_a_pair_twice() {
    let members = vec!["a".parse().unwrap(), "b".parse().unwrap()];
    for n in [3, 7, 8, 40] {
        let (pairs, v, deal) = design(n, &members, 7);
        assert!(v <= 2);
        assert_eq!(deal.values().map(Vec::len).sum::<usize>(), pairs.len() * v);
        for ds in deal.values() {
            let unique = ds
                .iter()
                .map(|d| &d.pair_id)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(unique.len(), ds.len());
            assert!(ds.len() <= 60);
        }
    }
}

#[test]
fn sparse_three_item_chain_matches_independent_decimal_mm_golden() {
    // 104 simultaneous updates at 60 decimal digits; equilibrium is (3,1,1/3).
    let t = fit(&[vec![0, 1, 0], vec![0, 0, 1], vec![0, 0, 0]], &[]);
    for (actual, expected) in
        t.strengths
            .iter()
            .zip([2.9999999866934495, 0.9999999968478517, 0.3333333326641813])
    {
        assert!((actual - expected).abs() < 1e-9);
    }
    assert_eq!(t.order, vec![0, 1, 2]);
}
