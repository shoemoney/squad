//! Decide's application-owned rubric, arithmetic and durable engine driver.
use squad_protocol::{decide::*, *};
use std::collections::{BTreeMap, BTreeSet};
pub fn prepare(parsed: &ParsedInput) -> Result<(), crate::prompts::PromptError> {
    if matches!(parsed, ParsedInput::Decide { .. }) {
        Ok(())
    } else {
        Err(crate::prompts::PromptError::new(
            "decide",
            None,
            "invalid command",
        ))
    }
}
pub fn counts(m: u32, generated: bool) -> (u32, u32) {
    let g = u32::from(generated);
    (2 * m + 2 + g * (m + 1), 4 * m + 6 + g * (2 * m + 3))
}
pub fn rubric(
    lists: &BTreeMap<String, DecideCriteria>,
    merged: &DecideFreeze,
) -> Result<DecideRubric, String> {
    if lists.len() < 2 {
        return Err("insufficient_criteria".into());
    }
    let mut all = Vec::new();
    for c in &merged.criteria {
        let mut support = BTreeSet::new();
        let mut sum = 0.0;
        for src in &c.sources {
            let item = lists
                .get(&src.member_label)
                .and_then(|l| l.criteria.iter().find(|x| x.local_id == src.local_id))
                .ok_or("unknown criterion source")?;
            support.insert(src.member_label.clone());
            sum += f64::from(item.weight);
        }
        all.push(DecideWeightedCriterion {
            criterion: c.clone(),
            support: support.len(),
            mean_weight: sum / lists.len() as f64,
            weight: 0.0,
        });
    }
    let mut eligible = all
        .iter()
        .enumerate()
        .filter(|(_, x)| x.support >= 2)
        .map(|(i, x)| (i, x.clone()))
        .collect::<Vec<_>>();
    eligible.sort_by(|(a, x), (b, y)| {
        y.mean_weight
            .total_cmp(&x.mean_weight)
            .then(y.support.cmp(&x.support))
            .then(a.cmp(b))
    });
    let selected = eligible
        .iter()
        .take(7)
        .map(|(i, _)| *i)
        .collect::<BTreeSet<_>>();
    let sum: f64 = all
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.contains(i))
        .map(|(_, x)| x.mean_weight)
        .sum();
    if selected.len() < 2 || sum <= 0.0 {
        return Err("no_shared_rubric".into());
    }
    let mut out = DecideRubric {
        kept: vec![],
        dropped: vec![],
    };
    for (i, mut x) in all.drain(..).enumerate() {
        if selected.contains(&i) {
            x.weight = x.mean_weight / sum;
            out.kept.push(x);
        } else {
            out.dropped.push(x);
        }
    }
    Ok(out)
}
pub fn shortlist(output: &DecideShortlist) -> Vec<DecideOption> {
    let mut xs = output.options.iter().enumerate().collect::<Vec<_>>();
    let support = |x: &DecideMergedOption| {
        x.sources
            .iter()
            .map(|s| &s.member_label)
            .collect::<BTreeSet<_>>()
            .len()
    };
    xs.sort_by(|(i, x), (j, y)| support(y).cmp(&support(x)).then(i.cmp(j)));
    xs.into_iter()
        .take(4)
        .map(|(_, x)| DecideOption {
            option_id: x.option_id.clone(),
            name: x.name.clone(),
            description: x.description.clone(),
        })
        .collect()
}
pub fn matrix(
    options: &[DecideOption],
    rubric: &DecideRubric,
    sheets: &[DecideScores],
) -> Result<DecideMatrix, String> {
    if sheets.len() < 2 {
        return Err("insufficient_scores".into());
    }
    let os = options
        .iter()
        .map(|x| x.option_id.clone())
        .collect::<Vec<_>>();
    let cs = rubric
        .kept
        .iter()
        .map(|x| x.criterion.criterion_id.clone())
        .collect::<Vec<_>>();
    for s in sheets {
        s.validate_against(&os, &cs).map_err(|e| e.message)?;
    }
    let mut cells = vec![];
    for o in &os {
        for c in &cs {
            let mut v = sheets
                .iter()
                .map(|s| {
                    s.cells
                        .iter()
                        .find(|x| &x.option_id == o && &x.criterion_id == c)
                        .expect("validated")
                        .score
                })
                .collect::<Vec<_>>();
            v.sort();
            let n = v.len();
            cells.push(DecideMedian {
                option_id: o.clone(),
                criterion_id: c.clone(),
                median: (f64::from(v[(n - 1) / 2]) + f64::from(v[n / 2])) / 2.0,
                contested: v[n - 1] - v[0] >= 4,
            });
        }
    }
    let score = |o: &str, c: &str| {
        cells
            .iter()
            .find(|x| x.option_id == o && x.criterion_id == c)
            .expect("complete")
            .median
    };
    let ranking = |omit: Option<&str>| {
        let denom = rubric
            .kept
            .iter()
            .filter(|x| Some(x.criterion.criterion_id.as_str()) != omit)
            .map(|x| x.weight)
            .sum::<f64>();
        let mut v = os
            .iter()
            .map(|o| DecideTotal {
                option_id: o.clone(),
                total: rubric
                    .kept
                    .iter()
                    .filter(|x| Some(x.criterion.criterion_id.as_str()) != omit)
                    .map(|x| x.weight * score(o, &x.criterion.criterion_id))
                    .sum::<f64>()
                    / if denom > 0.0 { denom } else { 1.0 },
            })
            .collect::<Vec<_>>();
        v.sort_by(|a, b| {
            b.total
                .total_cmp(&a.total)
                .then(a.option_id.cmp(&b.option_id))
        });
        v
    };
    let totals = ranking(None);
    let leader = &totals[0].option_id;
    let margin = totals[0].total - totals[1].total;
    let leave_one_out = rubric
        .kept
        .iter()
        .map(|c| DecideLeaveOut {
            criterion_id: c.criterion.criterion_id.clone(),
            ranking: ranking(Some(&c.criterion.criterion_id)),
        })
        .collect::<Vec<_>>();
    let mut flips = vec![];
    for c in &rubric.kept {
        let cid = &c.criterion.criterion_id;
        let w = c.weight;
        if w >= 1.0 {
            continue;
        }
        let ra = (totals[0].total - w * score(leader, cid)) / (1.0 - w);
        let mut candidates = vec![];
        for b in totals.iter().skip(1) {
            let rb = (b.total - w * score(&b.option_id, cid)) / (1.0 - w);
            let ds = score(&b.option_id, cid) - score(leader, cid);
            let dr = ra - rb;
            if ds * dr > 0.0 {
                let threshold = dr / (dr + ds);
                if (0.0..=1.0).contains(&threshold) {
                    candidates.push(DecideFlip {
                        criterion_id: cid.clone(),
                        rival_option_id: b.option_id.clone(),
                        threshold,
                        above: ds > 0.0,
                    });
                }
            }
        }
        candidates.sort_by(|a, b| {
            (a.threshold - w)
                .abs()
                .total_cmp(&(b.threshold - w).abs())
                .then(a.rival_option_id.cmp(&b.rival_option_id))
        });
        if let Some(f) = candidates.into_iter().next() {
            flips.push(f);
        }
    }
    let robust = leave_one_out
        .iter()
        .all(|x| x.ranking[0].option_id == *leader && x.ranking[0].total > x.ranking[1].total)
        && flips.iter().all(|f| {
            let w = rubric
                .kept
                .iter()
                .find(|c| c.criterion.criterion_id == f.criterion_id)
                .expect("known")
                .weight;
            (f.threshold - w).abs() > 0.10
        });
    let member_support = sheets
        .iter()
        .filter(|s| {
            let own = |o: &str| {
                rubric
                    .kept
                    .iter()
                    .map(|c| {
                        c.weight
                            * f64::from(
                                s.cells
                                    .iter()
                                    .find(|x| {
                                        x.option_id == o
                                            && x.criterion_id == c.criterion.criterion_id
                                    })
                                    .expect("complete")
                                    .score,
                            )
                    })
                    .sum::<f64>()
            };
            os.iter().all(|o| own(leader) >= own(o))
        })
        .count();
    Ok(DecideMatrix {
        cells,
        ranking: totals.clone(),
        margin,
        winner: (margin >= 0.25).then(|| leader.clone()),
        leave_one_out,
        flips,
        robust,
        member_support,
    })
}
pub fn drive(
    router: &mut crate::router::Router,
) -> squad_store::Result<Vec<squad_store::commit::CommitOutcome>> {
    let ids = router.store.call(|db| {
        let mut q =
            db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='decide'")?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(ids)
    })?;
    let mut outcomes = vec![];
    for raw in ids {
        let contest: ContestId = raw
            .parse()
            .map_err(|_| squad_store::StoreError::Unavailable)?;
        for _ in 0..24 {
            let before = router
                .store
                .call(move |db| squad_store::decide::state(db, contest))?;
            if before.state.is_terminal() {
                break;
            }
            let id = EventId::new();
            outcomes.push(router.store.commit(
                id,
                "contest-engine".into(),
                squad_store::commit::digest(&(contest, before.state_version))?,
                move |tx, g| {
                    crate::router::decide::advance(tx, g, id, contest)?;
                    Ok(g.accepted(id))
                },
            )?);
            let after = router
                .store
                .call(move |db| squad_store::decide::state(db, contest))?;
            if before == after {
                break;
            }
        }
    }
    Ok(outcomes)
}

/// Declarative call schedule; COMPUTING and REPORTING consume no provider calls.
pub fn script(setup: &DecideSetup) -> Vec<super::types::StepSpec> {
    let mut steps = vec![];
    for kind in [
        WorkKind::DecideOptions,
        WorkKind::DecideShortlist,
        WorkKind::DecideCriteria,
        WorkKind::DecideFreeze,
        WorkKind::DecideScore,
        WorkKind::DecideExplain,
    ] {
        if !setup.options.is_empty()
            && matches!(kind, WorkKind::DecideOptions | WorkKind::DecideShortlist)
        {
            continue;
        }
        let judge = kind.new_mode_judge();
        steps.push(super::types::StepSpec {
            move_name: if judge {
                ContestMove::Judge
            } else {
                ContestMove::Fanout
            },
            round: None,
            actors: if judge {
                vec![Target::Judge]
            } else {
                setup
                    .participants
                    .iter()
                    .cloned()
                    .map(Target::Member)
                    .collect()
            },
            composition: kind.new_mode_composition().map(str::to_owned),
            word_limit: Some(match kind {
                WorkKind::DecideOptions => 100,
                WorkKind::DecideShortlist | WorkKind::DecideCriteria => 300,
                WorkKind::DecideFreeze => 500,
                WorkKind::DecideScore => 700,
                _ => 1200,
            }),
            attempts: if judge { 3 } else { 2 },
            deadline_ms: if judge { 300_000 } else { 180_000 },
        });
        if !judge {
            steps.push(super::types::StepSpec {
                move_name: ContestMove::Barrier,
                round: None,
                actors: vec![],
                composition: None,
                word_limit: None,
                attempts: 0,
                deadline_ms: 0,
            });
        }
        if kind == WorkKind::DecideScore {
            steps.push(super::types::StepSpec {
                move_name: ContestMove::Tally,
                round: None,
                actors: vec![],
                composition: None,
                word_limit: None,
                attempts: 0,
                deadline_ms: 0,
            });
        }
    }
    steps.push(super::types::StepSpec {
        move_name: ContestMove::Report,
        round: None,
        actors: vec![],
        composition: None,
        word_limit: None,
        attempts: 0,
        deadline_ms: 0,
    });
    steps
}
