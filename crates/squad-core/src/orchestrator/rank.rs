//! Seeded comparison design and deterministic Bradley–Terry fitting (D86).
use rand::{Rng, SeedableRng, seq::SliceRandom};
use squad_protocol::{rank::*, *};
use std::collections::{BTreeMap, BTreeSet};
pub fn prepare(parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    if let ParsedInput::Rank {
        items, criterion, ..
    } = parsed
        && (3..=40).contains(&items.len())
        && !criterion.trim().is_empty()
        && items
            .iter()
            .all(|s| !s.trim().is_empty() && s.split_whitespace().count() <= 60)
    {
        return Ok(());
    }
    Err(crate::prompts::PromptError::new(
        "rank",
        None,
        "rank requires a criterion and 3 to 40 items, each at most 60 words",
    ))
}
pub fn call_bounds(m: u32) -> (u32, u32) {
    (m + 1, 2 * m + 3)
}
/// Every pair's replicas occupy consecutive slots of a shuffled member ring.
/// This guarantees distinct judges, balanced loads and the per-member cap.
pub fn design(
    n: usize,
    members: &[MemberNick],
    seed: u64,
) -> (Vec<RankPair>, usize, BTreeMap<MemberNick, Vec<RankDeal>>) {
    assert!((3..=40).contains(&n) && members.len() >= 2);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let q = (n * (n - 1) / 2).min(3 * n);
    let mut chosen = BTreeSet::new();
    let mut pairs = vec![];
    while pairs.len() < q {
        let mut ring = (0..n).collect::<Vec<_>>();
        ring.shuffle(&mut rng);
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let pair = (a.min(b), a.max(b));
            if chosen.insert(pair) {
                pairs.push(RankPair {
                    pair_id: format!("P{}", pairs.len() + 1),
                    left: pair.0,
                    right: pair.1,
                });
            }
            if pairs.len() == q {
                break;
            }
        }
    }
    let v = (60 * members.len() / q).clamp(1, 3).min(members.len());
    let mut roster = members.to_vec();
    roster.shuffle(&mut rng);
    let mut deal = members
        .iter()
        .cloned()
        .map(|n| (n, vec![]))
        .collect::<BTreeMap<_, _>>();
    for (slot, p) in pairs
        .iter()
        .flat_map(|p| std::iter::repeat_n(p, v))
        .enumerate()
    {
        let (a, b) = if rng.random_bool(0.5) {
            (p.left, p.right)
        } else {
            (p.right, p.left)
        };
        deal.get_mut(&roster[slot % roster.len()])
            .expect("member")
            .push(RankDeal {
                pair_id: p.pair_id.clone(),
                a,
                b,
            });
    }
    (pairs, v, deal)
}
pub fn aggregate(setup: &RankSetup, batches: &BTreeMap<MemberNick, RankBatch>) -> RankTally {
    let n = setup.items.len();
    let mut wins = vec![vec![0u32; n]; n];
    for (nick, batch) in batches {
        for c in &batch.choices {
            let d = setup.deal[nick]
                .iter()
                .find(|d| d.pair_id == c.pair_id)
                .expect("validated pair");
            let (w, l) = if c.winner == RankWinner::A {
                (d.a, d.b)
            } else {
                (d.b, d.a)
            };
            wins[w][l] += 1;
        }
    }
    fit(&wins, &setup.pairs)
}
pub fn fit(wins: &[Vec<u32>], pairs: &[RankPair]) -> RankTally {
    let n = wins.len();
    assert!(n > 0 && wins.iter().all(|r| r.len() == n));
    let totals = wins
        .iter()
        .map(|r| r.iter().sum::<u32>())
        .collect::<Vec<_>>();
    let mut p = vec![1f64; n];
    for _ in 0..1000 {
        let next = (0..n)
            .map(|i| {
                let denom = 1.0 / (p[i] + 1.0)
                    + (0..n)
                        .filter(|&j| j != i)
                        .map(|j| f64::from(wins[i][j] + wins[j][i]) / (p[i] + p[j]))
                        .sum::<f64>();
                (f64::from(totals[i]) + 0.5) / denom
            })
            .collect::<Vec<_>>();
        let delta = (0..n)
            .map(|i| (next[i].ln() - p[i].ln()).abs())
            .fold(0f64, f64::max);
        p = next;
        if delta < 1e-9 {
            break;
        }
    }
    let scores = p.iter().map(|p| p.ln()).collect::<Vec<_>>();
    let mut order = (0..n).collect::<Vec<_>>();
    // Insertion order with the specified near-tie comparison avoids relying on
    // sort's total-order contract for an epsilon-based comparison.
    for i in 1..n {
        let mut j = i;
        while j > 0 {
            let (a, b) = (order[j], order[j - 1]);
            let before = if (scores[a] - scores[b]).abs() <= 1e-9 {
                totals[a] > totals[b] || (totals[a] == totals[b] && a < b)
            } else {
                scores[a] > scores[b]
            };
            if !before {
                break;
            }
            order.swap(j, j - 1);
            j -= 1;
        }
    }
    let gaps = order
        .windows(2)
        .map(|a| {
            let gap = scores[a[0]] - scores[a[1]];
            let probability = 1.0 / (1.0 + (-gap).exp());
            RankGap {
                higher: a[0],
                lower: a[1],
                gap,
                probability,
                label: if probability >= 0.70 {
                    "clear"
                } else {
                    "close"
                }
                .into(),
            }
        })
        .collect();
    let edge = |i: usize, j: usize| wins[i][j] > wins[j][i];
    let mut cycles = vec![];
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let items = if edge(i, j) && edge(j, k) && edge(k, i) {
                    Some([i, j, k])
                } else if edge(i, k) && edge(k, j) && edge(j, i) {
                    Some([i, k, j])
                } else {
                    None
                };
                if let Some(items) = items {
                    cycles.push(RankCycle {
                        cycle_id: format!("C{}-{}-{}", i + 1, j + 1, k + 1),
                        items,
                        margin: wins[i][j].abs_diff(wins[j][i])
                            + wins[j][k].abs_diff(wins[k][j])
                            + wins[k][i].abs_diff(wins[i][k]),
                    });
                }
            }
        }
    }
    cycles.sort_by(|a, b| a.margin.cmp(&b.margin).then(a.items.cmp(&b.items)));
    let cycle_count = cycles.len();
    cycles.truncate(10);
    let split_pairs = pairs
        .iter()
        .filter(|p| wins[p.left][p.right] > 0 && wins[p.right][p.left] > 0)
        .map(|p| p.pair_id.clone())
        .collect();
    let mut visited = vec![false; n];
    let mut components = vec![];
    for i in 0..n {
        if visited[i] {
            continue;
        }
        let mut stack = vec![i];
        let mut component = vec![];
        visited[i] = true;
        while let Some(j) = stack.pop() {
            component.push(j);
            for k in 0..n {
                if !visited[k] && wins[j][k] + wins[k][j] > 0 {
                    visited[k] = true;
                    stack.push(k);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    RankTally {
        wins: wins.to_vec(),
        strengths: p,
        scores,
        order,
        gaps,
        cycles,
        cycle_count,
        split_pairs,
        components,
        judgments: totals.iter().sum::<u32>() as usize,
    }
}
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    output: &serde_json::Value,
) -> validation::ValidationResult {
    match kind {
        WorkKind::RankCompare => {
            let b: RankBatch = validation::decode(output)?;
            let assigned = data["pairs"]
                .as_array()
                .map(|ps| {
                    ps.iter()
                        .filter_map(|p| p["pairId"].as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            b.validate(&assigned)
        }
        WorkKind::RankExplain => {
            let e: RankExplanation = validation::decode(output)?;
            let top: Vec<String> =
                serde_json::from_value(data["topIds"].clone()).map_err(|_| {
                    validation::ValidationError {
                        message: "missing top ids".into(),
                    }
                })?;
            let cycles: Vec<String> =
                serde_json::from_value(data["cycleIds"].clone()).map_err(|_| {
                    validation::ValidationError {
                        message: "missing cycle ids".into(),
                    }
                })?;
            e.validate(
                &top,
                &cycles,
                data["wordLimit"].as_u64().unwrap_or(500) as usize,
            )
        }
        _ => validation::ensure(false, "invalid rank work"),
    }
}

use crate::router::{
    Router,
    rank::{drive_move, finish, invalid, load_state},
};
use squad_store::{Result as StoreResult, commit::CommitOutcome};
pub fn drive(router: &mut Router) -> StoreResult<Vec<CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q = db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='rank'")?;
        Ok(q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut outcomes = vec![];
    for raw in ids {
        let contest: ContestId = raw.parse().map_err(|_| invalid())?;
        if router
            .engine_scope
            .as_ref()
            .is_some_and(|s| !s.contains(&contest))
        {
            continue;
        }
        for _ in 0..64 {
            let state = router.store.call(move |db| load_state(db, contest))?;
            if state.state.is_terminal() {
                break;
            }
            let id = EventId::new();
            let digest = squad_store::commit::digest(&state)?;
            match router
                .store
                .commit(id, "contest-engine".into(), digest, move |tx, g| {
                    drive_move(tx, g, id, state)
                }) {
                Ok(o) if o.events.is_empty() => break,
                Ok(o) => outcomes.push(o),
                Err(e) => {
                    tracing::error!(?e,%contest,"Rank engine failed");
                    outcomes.push(router.store.commit(
                        EventId::new(),
                        "contest-engine".into(),
                        "rank-failure".into(),
                        move |tx, g| {
                            let mut s = load_state(tx, contest)?;
                            finish(tx, g, id, &mut s, "invalid_step", true)?;
                            Ok(g.accepted(id))
                        },
                    )?);
                    break;
                }
            }
        }
    }
    Ok(outcomes)
}
