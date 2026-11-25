//! Census is a script over the shared step/barrier/work engine.
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn script(setup: &CensusSetup) -> Vec<super::types::StepSpec> {
    let mut steps = Vec::new();
    for pass in 1..=setup.passes {
        for (name, movement) in [
            ("census.hunt", ContestMove::Fanout),
            ("", ContestMove::Barrier),
            ("census.merge", ContestMove::Judge),
            ("census.rate", ContestMove::Fanout),
            ("", ContestMove::Barrier),
            ("census.final", ContestMove::Judge),
            ("", ContestMove::Tally),
            ("", ContestMove::Report),
        ] {
            let judge = movement == ContestMove::Judge;
            let model = !name.is_empty();
            steps.push(super::types::StepSpec {
                move_name: movement,
                round: Some(u32::from(pass)),
                actors: if !model {
                    vec![]
                } else if judge {
                    vec![Target::Judge]
                } else {
                    setup
                        .participants
                        .iter()
                        .cloned()
                        .map(Target::Member)
                        .collect()
                },
                composition: model.then(|| name.into()),
                word_limit: model.then_some(if judge {
                    setup.plan_words
                } else {
                    setup.room_words
                }),
                attempts: if !model {
                    0
                } else if judge {
                    3
                } else {
                    2
                },
                deadline_ms: if !model {
                    0
                } else if judge {
                    300_000
                } else {
                    180_000
                },
            });
        }
    }
    steps
}
/// Rotates a random, persisted permutation. Public roster order is never a seed.
pub fn lenses(participants: &[MemberNick]) -> BTreeMap<MemberNick, usize> {
    let mut order = participants
        .iter()
        .map(|n| (CellId::new().to_string(), n.clone()))
        .collect::<Vec<_>>();
    order.sort();
    order
        .into_iter()
        .enumerate()
        .map(|(i, (_, n))| (n, i))
        .collect()
}
pub fn lens(index: usize, pass: u8) -> &'static str {
    CENSUS_LENSES[(index + usize::from(pass) - 1) % CENSUS_LENSES.len()]
}
/// Deterministic removal of attribution in blind text, retaining human originals.
pub fn blind_text(text: &str, setup: &CensusSetup) -> String {
    let mut tokens = setup.redaction_tokens.clone();
    tokens.extend(setup.participants.iter().map(ToString::to_string));
    for model in setup.models.values() {
        tokens.push(model.model.to_string());
    }
    let slugs = tokens
        .iter()
        .filter(|t| t.contains('/'))
        .cloned()
        .collect::<Vec<_>>();
    for slug in slugs {
        tokens.extend(
            slug.split(['/', '-', '_', '.', '~'])
                .filter(|p| p.chars().any(char::is_alphabetic))
                .map(str::to_owned),
        );
    }
    tokens.retain(|t| !t.trim().is_empty());
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    if tokens.is_empty() {
        return text.into();
    }
    let alternatives = tokens
        .iter()
        .map(|t| regex::escape(t.trim_start_matches('~')))
        .collect::<Vec<_>>()
        .join("|");
    let pattern = format!(r"(?i)\b(?:{alternatives})\b");
    regex::Regex::new(&pattern)
        .expect("escaped redaction tokens")
        .replace_all(text, "[redacted]")
        .into_owned()
}
/// Anonymous rating projection contains no lenses, source references or counts.
pub fn rating_data(
    plan: &CensusPlan,
    ledger: &CensusMergeOutput,
    setup: &CensusSetup,
) -> serde_json::Value {
    serde_json::json!({"plan":blind_text(plan.text.as_str(),setup),"planVersion":plan.version,"ledger":ledger.ledger.iter().map(|g|serde_json::json!({"id":g.id,"omission":blind_text(g.omission.as_str(),setup),"consequence":blind_text(g.consequence.as_str(),setup)})).collect::<Vec<_>>()})
}
/// Count actual accepted ballots. Missing ballots never vote; incomplete support
/// that has not produced an exclusion remains unresolved.
pub fn outcomes(
    ledger: &CensusMergeOutput,
    ratings: &[CensusRatingOutput],
    participants: usize,
    finders: &BTreeMap<String, Vec<MemberNick>>,
) -> Vec<CensusGapOutcome> {
    ledger
        .ledger
        .iter()
        .map(|g| {
            let votes = ratings
                .iter()
                .flat_map(|r| &r.ratings)
                .filter(|r| r.gap_id == g.id)
                .collect::<Vec<_>>();
            let covered = votes
                .iter()
                .filter(|r| r.rating == CensusVote::Covered)
                .count() as u32;
            let trivial = votes
                .iter()
                .filter(|r| r.rating == CensusVote::Trivial)
                .count() as u32;
            let v = votes.len() as u32;
            let disposition = if (covered + trivial) * 2 > v {
                if covered >= trivial {
                    CensusDisposition::Covered
                } else {
                    CensusDisposition::Trivial
                }
            } else if votes.len() < participants {
                CensusDisposition::Unresolved
            } else {
                CensusDisposition::Confirmed
            };
            let mut members = finders.get(g.id.as_str()).cloned().unwrap_or_default();
            members.sort();
            members.dedup();
            CensusGapOutcome {
                gap: g.clone(),
                disposition,
                votes: v,
                covered,
                trivial,
                finder_count: members.len() as u32,
                priority: None,
                finders: members,
                citations: votes.iter().filter_map(|r| r.citation.clone()).collect(),
            }
        })
        .collect()
}
pub fn report(
    id: ContestId,
    pass: u8,
    version: u32,
    gaps: Vec<CensusGapOutcome>,
    additions: Vec<CensusFinalAddition>,
    missing: Vec<CensusMissing>,
    prior: &[CensusReport],
) -> CensusReport {
    let inputs = gaps
        .iter()
        .filter(|g| g.disposition == CensusDisposition::Confirmed && !g.finders.is_empty())
        .collect::<Vec<_>>();
    let f1 = inputs.iter().filter(|g| g.finders.len() == 1).count() as u32;
    let f2 = inputs.iter().filter(|g| g.finders.len() == 2).count() as u32;
    let (unseen, branch) = census_chao1(f1, f2);
    let input_gap_ids = inputs.iter().map(|g| g.gap.id.clone()).collect();
    let identity = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let mut known = prior
        .iter()
        .flat_map(|p| {
            p.gaps
                .iter()
                .filter(|g| g.disposition == CensusDisposition::Confirmed)
                .map(|g| identity(g.gap.omission.as_str()))
        })
        .collect::<BTreeSet<_>>();
    for old in prior {
        for addition in &old.additions {
            known.insert(identity(addition.omission.as_str()));
        }
    }
    let before = known.len();
    for gap in gaps
        .iter()
        .filter(|g| g.disposition == CensusDisposition::Confirmed)
    {
        known.insert(identity(gap.gap.omission.as_str()));
    }
    for addition in &additions {
        known.insert(identity(addition.omission.as_str()));
    }
    let mut discovery = prior
        .last()
        .map(|p| p.discovery.clone())
        .unwrap_or_default();
    discovery.push(CensusDiscovery {
        pass,
        plan_version: version,
        new_confirmed: (known.len() - before) as u32,
        cumulative_confirmed: known.len() as u32,
        singletons: f1,
    });
    CensusReport {
        contest_id: id,
        pass,
        plan_version: version,
        gaps,
        additions,
        missing,
        input_gap_ids,
        f1,
        f2,
        formula_branch: Text::new(branch).expect("branch"),
        unseen,
        estimate_low: None,
        estimate_high: None,
        discovery,
    }
}

pub fn redact_prose(value: &mut serde_json::Value, setup: &CensusSetup) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map {
                if [
                    "plan",
                    "omission",
                    "consequence",
                    "reason",
                    "location",
                    "quote",
                    "priority",
                ]
                .contains(&key.as_str())
                {
                    if let Some(s) = v.as_str() {
                        *v = serde_json::Value::String(blind_text(s, setup));
                    }
                } else {
                    redact_prose(v, setup);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                redact_prose(v, setup);
            }
        }
        _ => {}
    }
}

// Mode script driver. The shared orchestrator invokes this on the serialized
// router thread; each move uses the same transactional step/work/barrier store.
use crate::router::{
    Router,
    census::{drive_move, invalid, save},
};
use squad_store::{Result, commit::CommitOutcome};
impl Router {
    pub(crate) fn drive_census(&mut self) -> Result<Vec<CommitOutcome>> {
        let ids = self.store.call(|db| {
            let mut q =
                db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='census' AND json_type(state_json,'$.pass')='integer'")?;
            Ok(q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut outcomes = vec![];
        for raw in ids {
            let contest: ContestId = raw.parse().map_err(|_| invalid())?;
            if self
                .engine_scope
                .as_ref()
                .is_some_and(|s| !s.contains(&contest))
            {
                continue;
            }
            for _ in 0..64 {
                let snapshot = self
                    .store
                    .call(move |db| squad_store::census::state(db, contest))?;
                if snapshot.status.is_terminal() || snapshot.status == CensusStatus::WaitingForPlan
                {
                    break;
                }
                let id = EventId::new();
                let state = snapshot.clone();
                let outcome = self.store.commit(
                    id,
                    "contest-engine".into(),
                    squad_store::commit::digest(&snapshot)?,
                    move |tx, group| drive_move(tx, group, id, state),
                );
                match outcome {
                    Ok(o) if o.events.is_empty() => break,
                    Ok(o) => outcomes.push(o),
                    Err(e) => {
                        tracing::error!(?e,%contest,"Census engine move failed");
                        outcomes.push(self.store.commit(
                            EventId::new(),
                            "contest-engine".into(),
                            "census-failure".into(),
                            move |tx, group| {
                                let mut state = squad_store::census::state(tx, contest)?;
                                let version = state.state_version;
                                state.status = CensusStatus::Failed;
                                state.terminal_reason =
                                    Some(Text::new("invalid_step").expect("reason"));
                                state.state_version += 1;
                                save(tx, group, id, &state, Some(version))?;
                                Ok(group.accepted(id))
                            },
                        )?);
                        break;
                    }
                }
            }
        }
        Ok(outcomes)
    }
}

/// Read the canonical GrillPlan wire view without coupling this mode branch to
/// the sibling mode's payload variant. Callers supply retained typed envelopes.
pub fn grill_plan_text(value: &serde_json::Value) -> Option<String> {
    (value["kind"] == "grill.plan")
        .then(|| {
            value["payload"]["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        })
        .flatten()
}
