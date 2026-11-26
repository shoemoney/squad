//! Deterministic challenge arithmetic and ranked reports; the script consumes commits.
use squad_protocol::blow_it_up::Verdict;
use squad_protocol::blow_it_up::*;
use squad_protocol::*;
use std::collections::BTreeSet;
pub fn prepare(parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::BlowItUp { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "blow_it_up",
            None,
            "wrong mode",
        ))
    }
}
pub fn tally(pool: &BlowItUpPool, ballots: &[(String, BlowItUpChallenge)]) -> BlowItUpTally {
    BlowItUpTally {
        vectors: pool
            .vectors
            .iter()
            .map(|v| {
                let verdicts = ballots
                    .iter()
                    .flat_map(|(_, b)| &b.verdicts)
                    .filter(|b| b.vector_id == v.vector_id)
                    .collect::<Vec<_>>();
                let n = verdicts.len() as u32;
                let f = verdicts
                    .iter()
                    .filter(|b| b.verdict == Verdict::FalsePositive)
                    .count() as u32;
                let w = verdicts
                    .iter()
                    .filter(|b| b.verdict == Verdict::Weak)
                    .count() as u32;
                VectorTally {
                    vector_id: v.vector_id.clone(),
                    accepted: n,
                    false_positive: f,
                    weak: w,
                    status: if n < 2 {
                        VectorStatus::Unchallenged
                    } else if f * 2 > n {
                        VectorStatus::Killed
                    } else if (f + w) * 2 > n {
                        VectorStatus::Contested
                    } else {
                        VectorStatus::Standing
                    },
                }
            })
            .collect(),
    }
}
pub fn rank(ruling: &BlowItUpRuling, pool: &BlowItUpPool) -> Vec<RankedFinding> {
    let mut rows = ruling
        .findings
        .iter()
        .enumerate()
        .map(|(order, f)| {
            let finders = pool
                .provenance
                .iter()
                .filter(|p| !p.addition && f.source_vector_ids.contains(&p.vector_id))
                .map(|p| &p.attacker)
                .collect::<BTreeSet<_>>()
                .len() as u32;
            (
                order,
                RankedFinding {
                    finding: f.clone(),
                    score: f.severity.weight() * f.likelihood.weight(),
                    finder_count: finders,
                    lone: finders == 1,
                    addition: f.source_vector_ids.iter().any(|id| {
                        pool.vectors
                            .iter()
                            .any(|v| &v.vector_id == id && v.addition)
                    }),
                },
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|(i, a), (j, b)| {
        b.score
            .cmp(&a.score)
            .then(b.finder_count.cmp(&a.finder_count))
            .then(i.cmp(j))
    });
    rows.into_iter().map(|(_, r)| r).collect()
}
pub fn report(
    ruling: &BlowItUpRuling,
    pool: &BlowItUpPool,
    tally: &BlowItUpTally,
    ballots: &[(String, BlowItUpChallenge)],
    skipped: Vec<SkippedMember>,
) -> BlowItUpReport {
    let killed = tally
        .vectors
        .iter()
        .filter(|t| t.status == VectorStatus::Killed)
        .filter_map(|t| {
            let vector = pool
                .vectors
                .iter()
                .find(|v| v.vector_id == t.vector_id)?
                .clone();
            let quotes = ballots
                .iter()
                .flat_map(|(_, b)| &b.verdicts)
                .filter(|v| v.vector_id == t.vector_id)
                .filter_map(|v| v.defeating_quote.as_ref().map(ToString::to_string))
                .collect::<BTreeSet<_>>();
            Some(KilledVector {
                vector,
                tally: t.clone(),
                defeating_quotes: quotes.into_iter().map(|s| Text::new(s).unwrap()).collect(),
                judge_note: ruling
                    .killed_notes
                    .iter()
                    .find(|n| n.vector_id == t.vector_id)
                    .map(|n| n.note.clone()),
            })
        })
        .collect();
    BlowItUpReport {
        findings: rank(ruling, pool),
        killed,
        additions: pool
            .vectors
            .iter()
            .filter(|v| v.addition)
            .cloned()
            .collect(),
        unchallenged: tally
            .vectors
            .iter()
            .filter(|v| v.status == VectorStatus::Unchallenged)
            .map(|v| v.vector_id.clone())
            .collect(),
        skipped,
    }
}
pub fn drive(
    router: &mut crate::router::Router,
) -> squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q =
            db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='blow_it_up'")?;
        Ok(q.query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut out = vec![];
    for id in ids {
        let id: ContestId = id
            .parse()
            .map_err(|_| squad_store::StoreError::Unavailable)?;
        if router
            .engine_scope
            .as_ref()
            .is_some_and(|s| !s.contains(&id))
        {
            continue;
        }
        for _ in 0..12 {
            let request = EventId::new();
            let result = router.store.commit(
                request,
                "contest-engine".into(),
                squad_store::commit::digest(&(id, request))?,
                move |tx, group| {
                    crate::router::blow_it_up::advance(tx, group, request, id)?;
                    Ok(group.accepted(request))
                },
            )?;
            // Empty engine passes are harmless, but stop as soon as no canonical move occurred.
            let changed = !result.events.is_empty();
            out.push(result);
            if !changed {
                break;
            }
        }
    }
    Ok(out)
}
