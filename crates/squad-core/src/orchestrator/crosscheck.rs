//! Crosscheck script policy over the shared contest engine. All randomness is
//! frozen by the router before dispatch; replay calls only these pure functions.
use serde::{Deserialize, Serialize};
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivateRouting {
    pub lens_order: Vec<CrosscheckLens>,
    pub tie_break_seed: CellId,
}
impl PrivateRouting {
    pub fn draw() -> Self {
        let tie_break_seed = CellId::new();
        let mut lens_order = CrosscheckLens::ALL.to_vec();
        lens_order.sort_by_key(|lens| rank(tie_break_seed, lens));
        Self {
            lens_order,
            tie_break_seed,
        }
    }
    pub fn lens(&self, index: usize) -> CrosscheckLens {
        self.lens_order[index % 6]
    }
}
fn rank(seed: CellId, value: &impl Serialize) -> String {
    // SHA-256 of a private random seed and stable identity, independent of roster order.
    squad_store::commit::digest(&(seed, value)).expect("serializable routing identity")
}
fn invalid() -> ErrorCode {
    ErrorCode::INVALID_STATE_TRANSITION
}
pub fn advance(
    state: &CrosscheckState,
    signal: &CrosscheckTransition,
    extraction: bool,
) -> Result<CrosscheckState, ErrorCode> {
    use CrosscheckStatus as S;
    use CrosscheckTransition as T;
    let mut next = state.clone();
    next.status = match (state.status, signal) {
        (S::Preflight, T::Confirm) => S::WaitingForParticipants,
        (S::WaitingForParticipants, T::ParticipantsReady) => {
            if extraction {
                S::Extracting
            } else {
                S::Inventorying
            }
        }
        (S::Extracting, T::ExtractionCommitted) => S::Inventorying,
        (S::Inventorying, T::InventoriesCommitted) => S::BuildingGrid,
        (S::BuildingGrid, T::GridCommitted) => {
            next.grid_version += 1;
            S::Reviewing
        }
        (S::Reviewing, T::ReviewsCommitted) => S::Auditing,
        (S::Auditing, T::AuditsCommitted) => S::Ruling,
        (S::Ruling, T::RulingCommitted) => S::Reporting,
        (S::Reporting, T::ReportCommitted) => S::Complete,
        (s, T::Stop) if !s.is_terminal() => S::Stopped,
        (s, T::Fail) if !s.is_terminal() => S::Failed,
        _ => return Err(invalid()),
    };
    next.step_id = None;
    next.state_version = next.state_version.checked_add(1).ok_or_else(invalid)?;
    Ok(next)
}
/// Complete candidate keys use normalized stable item IDs, never model cell IDs.
pub fn candidates(
    items: &[CrosscheckNormalizedItem],
) -> Result<Vec<(String, Vec<usize>)>, ErrorCode> {
    let mut ids = BTreeSet::new();
    for item in items {
        if item.id.as_str().contains(['+', ':']) || !ids.insert(item.id.as_str()) {
            return Err(invalid());
        }
    }
    let mut cells = Vec::new();
    for (i, item) in items.iter().enumerate() {
        cells.push((format!("single:{}", item.id), vec![i]));
        for (j, other) in items.iter().enumerate().skip(i + 1) {
            if item.lens != other.lens {
                let mut pair = [item.id.as_str(), other.id.as_str()];
                pair.sort();
                cells.push((format!("pair:{}+{}", pair[0], pair[1]), vec![i, j]));
            }
        }
    }
    cells.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(cells)
}
/// A normalized item must account for actual sources, in the original lens,
/// exactly once. Merging retains the union of all originating participants.
pub fn build_grid(
    setup: &CrosscheckSetup,
    normalized: CrosscheckNormalization,
    inventories: &[(EventId, CrosscheckInventory)],
    routing: &PrivateRouting,
) -> Result<CrosscheckGrid, ErrorCode> {
    setup.validate().map_err(|_| invalid())?;
    let mut sources = BTreeMap::new();
    for (event, inventory) in inventories {
        inventory.response.validate().map_err(|_| invalid())?;
        if inventory.contest_id != setup.contest_id
            || !setup.participants.contains(&inventory.member)
        {
            return Err(invalid());
        }
        for item in &inventory.response.items {
            if sources
                .insert(
                    (*event, item.local_id.as_str()),
                    (inventory.lens, inventory.member.clone()),
                )
                .is_some()
            {
                return Err(invalid());
            }
        }
    }
    let mut used = BTreeSet::new();
    let mut origins = Vec::new();
    for item in &normalized.items {
        if item.source_item_ids.is_empty() {
            return Err(invalid());
        }
        let mut members = BTreeSet::new();
        for source in &item.source_item_ids {
            let key = (source.inventory_event_id, source.local_id.as_str());
            let (lens, member) = sources.get(&key).ok_or_else(invalid)?;
            if *lens != item.lens || !used.insert(key) {
                return Err(invalid());
            }
            members.insert(member.clone());
        }
        origins.push(members);
    }
    if used.len() != sources.len() {
        return Err(invalid());
    }
    let mut candidate = candidates(&normalized.items)?;
    let priority: BTreeMap<_, _> = normalized
        .priority_order
        .iter()
        .enumerate()
        .map(|(i, key)| (key.as_str(), i))
        .collect();
    if priority.len() != candidate.len()
        || normalized.priority_order.len() != candidate.len()
        || candidate
            .iter()
            .any(|(k, _)| !priority.contains_key(k.as_str()))
    {
        return Err(invalid());
    }
    candidate.sort_by_key(|(key, _)| priority[key.as_str()]);
    let mut counts: BTreeMap<_, usize> =
        setup.participants.iter().cloned().map(|n| (n, 0)).collect();
    let mut all = Vec::new();
    for (index, (key, indices)) in candidate.into_iter().enumerate() {
        let originators: BTreeSet<_> = indices
            .iter()
            .flat_map(|i| origins[*i].iter().cloned())
            .collect();
        let reviewer = if index < 200 {
            setup
                .participants
                .iter()
                .filter(|n| !originators.contains(*n))
                .min_by_key(|n| (counts[*n], rank(routing.tie_break_seed, &(&key, *n))))
                .cloned()
        } else {
            None
        };
        if let Some(nick) = &reviewer {
            *counts.get_mut(nick).expect("admitted reviewer") += 1;
        }
        all.push(CrosscheckCell {
            cell_id: CellId::new(),
            key,
            items: indices
                .iter()
                .map(|i| normalized.items[*i].item.clone())
                .collect(),
            originators: originators.into_iter().collect(),
            reviewer,
        });
    }
    let omitted = if all.len() > 200 {
        all.split_off(200)
    } else {
        vec![]
    };
    Ok(CrosscheckGrid {
        contest_id: setup.contest_id,
        plan_id: setup.plan_id,
        plan_version: setup.plan_version,
        grid_version: 1,
        items: normalized.items,
        cells: all,
        omitted,
    })
}
pub fn visible(cell: &CrosscheckCell) -> CrosscheckVisibleCell {
    CrosscheckVisibleCell {
        cell_id: cell.cell_id,
        items: cell.items.clone(),
    }
}
pub fn review_material(
    setup: &CrosscheckSetup,
    grid: &CrosscheckGrid,
    reviewer: &MemberNick,
) -> CrosscheckCaseMaterial {
    CrosscheckCaseMaterial::Review {
        plan: setup.plan.clone(),
        cells: grid
            .cells
            .iter()
            .filter(|c| c.reviewer.as_ref() == Some(reviewer))
            .map(visible)
            .collect(),
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditSelection {
    pub auditor: MemberNick,
    pub sample: Vec<CellId>,
}
pub fn audit_selections(
    participants: &[MemberNick],
    grid: &CrosscheckGrid,
    reviews: &[CrosscheckReview],
    seed: CellId,
) -> Vec<AuditSelection> {
    let mut auditors = participants.to_vec();
    auditors.sort_by_key(|n| rank(seed, &("auditor", n)));
    auditors
        .into_iter()
        .take(2)
        .map(|auditor| {
            let closures = reviews
                .iter()
                .flat_map(|r| {
                    r.response
                        .cells
                        .iter()
                        .filter(|c| c.status != CrosscheckDisposition::Gap)
                        .map(move |c| (&r.reviewer, c.cell_id))
                })
                .collect::<Vec<_>>();
            let alternatives = closures.iter().any(|(n, _)| **n != auditor);
            let mut sample = closures
                .into_iter()
                .filter(|(n, id)| {
                    (!alternatives || **n != auditor) && grid.cells.iter().any(|c| c.cell_id == *id)
                })
                .map(|(_, id)| id)
                .collect::<Vec<_>>();
            sample.sort_by_key(|id| rank(seed, &(&auditor, id)));
            sample.dedup();
            sample.truncate(5);
            AuditSelection { auditor, sample }
        })
        .collect()
}
pub fn report(
    setup: &CrosscheckSetup,
    grid: &CrosscheckGrid,
    reviews: &[CrosscheckReview],
    audits: &[CrosscheckAudit],
    ruling: &CrosscheckRuling,
    selections: &[AuditSelection],
) -> CrosscheckReport {
    let mut ledger = grid
        .cells
        .iter()
        .map(|cell| CrosscheckLedgerCell {
            cell: cell.clone(),
            review: reviews
                .iter()
                .flat_map(|r| &r.response.cells)
                .find(|r| r.cell_id == cell.cell_id)
                .cloned(),
            challenges: audits
                .iter()
                .flat_map(|a| &a.response.audits)
                .filter(|a| {
                    a.cell_id == cell.cell_id && a.verdict == CrosscheckAuditVerdict::Challenged
                })
                .cloned()
                .collect(),
            ruling: reviews
                .iter()
                .flat_map(|r| &r.response.cells)
                .any(|r| r.cell_id == cell.cell_id)
                .then(|| {
                    ruling
                        .cells
                        .iter()
                        .find(|c| c.cell_id == cell.cell_id)
                        .cloned()
                })
                .flatten(),
            acceptance_event_id: None,
        })
        .collect::<Vec<_>>();
    let mut residual_risks = ruling
        .residual_risks
        .iter()
        .map(|r| CrosscheckResidualRisk {
            id: Text::new(CellId::new().to_string()).expect("id"),
            case: r.clone(),
            proposed_fix: r.clone(),
            acceptance_event_id: None,
        })
        .collect::<Vec<_>>();
    for addition in &ruling.additions {
        if addition.decision != CrosscheckAdditionDecision::Added {
            continue;
        }
        if let Some(audit) = audits.iter().find(|a| a.work_id == addition.audit_work_id) {
            let missed = &audit.response.missed_case;
            if ledger.len() < 200 {
                let cell_id = CellId::new();
                ledger.push(CrosscheckLedgerCell {
                    cell: CrosscheckCell {
                        cell_id,
                        key: format!("audit:{}", audit.work_id),
                        items: vec![missed.case.clone()],
                        originators: vec![audit.auditor.clone()],
                        reviewer: None,
                    },
                    review: None,
                    challenges: vec![],
                    ruling: Some(CrosscheckCellReview {
                        cell_id,
                        status: CrosscheckDisposition::Gap,
                        citation: None,
                        reason: missed.reason.clone(),
                        proposed_fix: Some(missed.proposed_fix.clone()),
                    }),
                    acceptance_event_id: None,
                });
            } else {
                residual_risks.push(CrosscheckResidualRisk {
                    id: Text::new(CellId::new().to_string()).expect("id"),
                    case: missed.case.clone(),
                    proposed_fix: missed.proposed_fix.clone(),
                    acceptance_event_id: None,
                });
            }
        }
    }
    let failed_inventories = setup
        .participants
        .iter()
        .filter(|n| {
            !grid
                .cells
                .iter()
                .chain(grid.omitted.iter())
                .any(|c| c.originators.contains(n))
        })
        .cloned()
        .collect::<Vec<_>>();
    let failed_reviewers = setup
        .participants
        .iter()
        .filter(|n| {
            grid.cells.iter().any(|c| c.reviewer.as_ref() == Some(*n))
                && !reviews.iter().any(|r| &r.reviewer == *n)
        })
        .cloned()
        .collect::<Vec<_>>();
    let failed_auditors = selections
        .iter()
        .filter(|s| !audits.iter().any(|a| a.auditor == s.auditor))
        .map(|s| s.auditor.clone())
        .collect::<Vec<_>>();
    let audit_clean = failed_auditors.is_empty()
        && audits.len() == 2
        && audits.iter().all(|a| {
            a.response
                .audits
                .iter()
                .all(|f| f.verdict == CrosscheckAuditVerdict::Upheld)
        })
        && ruling
            .additions
            .iter()
            .all(|a| a.decision != CrosscheckAdditionDecision::Added);
    let mut report = CrosscheckReport {
        contest_id: setup.contest_id,
        plan_id: setup.plan_id,
        plan_version: setup.plan_version,
        grid_version: grid.grid_version,
        ledger,
        omitted: grid.omitted.clone(),
        residual_risks,
        failed_inventories,
        failed_reviewers,
        failed_auditors,
        audit_clean,
        bounded_stop: false,
    };
    recompute_stop(&mut report, &[]);
    report
}
pub fn recompute_stop(report: &mut CrosscheckReport, accepted_residuals: &[Text]) {
    report.bounded_stop = report.audit_clean
        && report.failed_inventories.is_empty()
        && report.failed_reviewers.is_empty()
        && report.failed_auditors.is_empty()
        && report.ledger.iter().all(|c| {
            c.acceptance_event_id.is_some()
                || c.ruling
                    .as_ref()
                    .or(c.review.as_ref())
                    .is_some_and(|r| r.status != CrosscheckDisposition::Gap)
        })
        && report
            .residual_risks
            .iter()
            .all(|r| r.acceptance_event_id.is_some() || accepted_residuals.contains(&r.id));
}

use crate::router::{Router, crosscheck as adapter};
use rusqlite::Connection;
use squad_store::{Result as StoreResult, StoreError, commit::CommitOutcome};
fn store_error() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Crosscheck script cannot advance",
    )
}
pub fn load_inventories(
    db: &Connection,
    id: ContestId,
) -> StoreResult<Vec<(EventId, CrosscheckInventory)>> {
    let mut q=db.prepare("SELECT e.seq FROM events e JOIN contest_events c ON c.event_seq=e.seq WHERE c.contest_id=? AND e.kind='crosscheck.inventory' ORDER BY e.seq")?;
    q.query_map([id.to_string()], |r| r.get::<_, i64>(0))?
        .map(|seq| {
            let e = squad_store::events::at(db, seq?)?;
            let EnvelopePayload::CrosscheckInventory(i) = e.content else {
                return Err(store_error());
            };
            Ok((e.base.id, i))
        })
        .collect()
}
pub(crate) fn step_outputs(
    db: &Connection,
    step: StepId,
) -> StoreResult<Vec<(Work, Option<ContestArtifact>)>> {
    let mut q =
        db.prepare("SELECT work_id,completed_event_seq FROM work WHERE step_id=? ORDER BY rowid")?;
    let rows = q
        .query_map([step.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, seq)| {
            let (w, _) = squad_store::work::template(db, id.parse().map_err(|_| store_error())?)?
                .ok_or_else(store_error)?;
            let output = seq
                .map(|seq| squad_store::events::at(db, seq))
                .transpose()?
                .and_then(|e| {
                    if let EnvelopePayload::ContestArtifact(a) = e.content {
                        Some(a)
                    } else {
                        None
                    }
                });
            Ok((w, output))
        })
        .collect()
}
impl Router {
    pub(crate) fn drive_crosschecks(&mut self) -> StoreResult<Vec<CommitOutcome>> {
        let ids = self.store.call(|db| {
            let mut q =
                db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='crosscheck'")?;
            Ok(q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut outcomes = vec![];
        for id in ids {
            let id: ContestId = id.parse().map_err(|_| store_error())?;
            if self
                .engine_scope
                .as_ref()
                .is_some_and(|scope| !scope.contains(&id))
            {
                continue;
            }
            for _ in 0..32 {
                let state = self.crosscheck_state(id)?;
                if state.status.is_terminal() {
                    break;
                }
                match self.crosscheck_move(state.clone()) {
                    Ok(Some(outcome)) => outcomes.push(outcome),
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!(?e,contest_id=%id,"Crosscheck engine move failed");
                        outcomes.push(self.transition(
                            EventId::new(),
                            ContestTransitionSubmission {
                                contest_id: id,
                                kind: ContestKind::Crosscheck,
                                expected_state_version: state.state_version,
                                transition: ContestTransition::Crosscheck(
                                    CrosscheckTransition::Fail,
                                ),
                            },
                        )?);
                        break;
                    }
                }
            }
        }
        Ok(outcomes)
    }
    fn crosscheck_move(&mut self, state: CrosscheckState) -> StoreResult<Option<CommitOutcome>> {
        let id = state.contest_id;
        let setup: CrosscheckSetup = self
            .store
            .call(move |db| adapter::map(db, id, "crosscheck.setup"))?;
        if state.status == CrosscheckStatus::Preflight {
            return Ok(None);
        }
        if state.status == CrosscheckStatus::WaitingForParticipants {
            let members = setup.participants.clone();
            let pending=self.store.call(move |db|{
                Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND status IN ('queued','running') AND target IN (SELECT value FROM json_each(?)))",[serde_json::to_string(&members)?],|r|r.get::<_,bool>(0))?)
            })?;
            if pending {
                return Ok(None);
            }
            return self
                .transition(
                    EventId::new(),
                    ContestTransitionSubmission {
                        contest_id: id,
                        kind: ContestKind::Crosscheck,
                        expected_state_version: state.state_version,
                        transition: ContestTransition::Crosscheck(
                            CrosscheckTransition::ParticipantsReady,
                        ),
                    },
                )
                .map(Some);
        }
        if let Some(step_id) = state.step_id {
            let (pending,skipped,artifacts)=self.store.call(move |db|{
                let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE step_id=? AND status IN ('queued','running') AND (status='running' OR attempt<CASE kind WHEN 'crosscheck.judge' THEN 3 ELSE 2 END))",[step_id.to_string()],|r|r.get(0))?;
                let rows=step_outputs(db,step_id)?;
                let skipped=rows.iter().filter(|(_,a)|a.is_none()).map(|(w,_)|w.work_id).collect();
                let mut q=db.prepare("SELECT e.id FROM events e JOIN mode_artifacts a ON a.event_seq=e.seq WHERE a.step_id=? ORDER BY e.seq")?;
                let artifacts=q.query_map([step_id.to_string()],|r|r.get::<_,String>(0))?.map(|r|r?.parse().map_err(|_|rusqlite::Error::InvalidQuery)).collect::<rusqlite::Result<Vec<EventId>>>()?;
                Ok((pending,skipped,artifacts))
            })?;
            if pending {
                return Ok(None);
            }
            let report = ContestReport {
                contest_id: id,
                step_id,
                artifact_event_ids: artifacts,
                skipped_work_ids: skipped,
                data: serde_json::json!({"crosscheckStatus":state.status}),
            };
            let barrier = BarrierCommit {
                contest_id: id,
                step_id,
                expected_state_version: state.state_version,
                barrier_event: crate::router::envelope(
                    EventId::new(),
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::ContestReport(report),
                ),
                projections: vec![],
                next_work: vec![],
            };
            return self.barrier(EventId::new(), barrier).map(Some);
        }
        let frozen = state.clone();
        let (source, materials, mut private_maps) = self.store.call(move |db| {
            let routing: PrivateRouting = adapter::map(db, id, "crosscheck.routing")?;
            let seq: i64 = db.query_row(
                "SELECT MAX(event_seq) FROM contest_events WHERE contest_id=?",
                [id.to_string()],
                |r| r.get(0),
            )?;
            let source = squad_store::events::at(db, seq)?;
            let mut maps = BTreeMap::new();
            let goal = setup.goal.clone();
            let materials: Vec<(Target, CrosscheckCaseMaterial)> = match frozen.status {
                CrosscheckStatus::Extracting => vec![(
                    Target::Judge,
                    CrosscheckCaseMaterial::ExtractGoal {
                        plan: setup.plan.clone(),
                    },
                )],
                CrosscheckStatus::Inventorying => setup
                    .participants
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        Ok((
                            Target::Member(n.clone()),
                            CrosscheckCaseMaterial::Inventory {
                                goal: goal.clone().ok_or_else(store_error)?,
                                lens: routing.lens(i),
                            },
                        ))
                    })
                    .collect::<StoreResult<_>>()?,
                CrosscheckStatus::BuildingGrid => vec![(
                    Target::Judge,
                    CrosscheckCaseMaterial::NormalizeInventory {
                        goal: goal.ok_or_else(store_error)?,
                        inventories: load_inventories(db, id)?
                            .into_iter()
                            .map(|(event, i)| (event, i.lens, i.response))
                            .collect(),
                    },
                )],
                CrosscheckStatus::Reviewing => {
                    let grid: CrosscheckGrid = adapter::map(db, id, "crosscheck.grid")?;
                    setup
                        .participants
                        .iter()
                        .map(|n| (Target::Member(n.clone()), review_material(&setup, &grid, n)))
                        .collect()
                }
                CrosscheckStatus::Auditing => {
                    let grid: CrosscheckGrid = adapter::map(db, id, "crosscheck.grid")?;
                    let reviews: Vec<CrosscheckReview> =
                        adapter::map(db, id, "crosscheck.reviews")?;
                    let samples = audit_selections(
                        &setup.participants,
                        &grid,
                        &reviews,
                        routing.tie_break_seed,
                    );
                    maps.insert("crosscheck.samples".into(), serde_json::to_value(&samples)?);
                    samples
                        .into_iter()
                        .map(|s| {
                            (
                                Target::Member(s.auditor),
                                CrosscheckCaseMaterial::Audit {
                                    plan: setup.plan.clone(),
                                    grid: grid.cells.iter().map(visible).collect(),
                                    samples: reviews
                                        .iter()
                                        .flat_map(|r| &r.response.cells)
                                        .filter(|c| s.sample.contains(&c.cell_id))
                                        .cloned()
                                        .collect(),
                                },
                            )
                        })
                        .collect()
                }
                CrosscheckStatus::Ruling => vec![(
                    Target::Judge,
                    CrosscheckCaseMaterial::Rule {
                        plan: setup.plan.clone(),
                        grid: adapter::map(db, id, "crosscheck.grid")?,
                        reviews: adapter::map(db, id, "crosscheck.reviews")?,
                        audits: adapter::map(db, id, "crosscheck.audits")?,
                    },
                )],
                CrosscheckStatus::Reporting => vec![],
                _ => return Err(store_error()),
            };
            let tokens: Vec<String> = adapter::map(db, id, "crosscheck.redaction")?;
            let mut materials = materials;
            for (_, material) in &mut materials {
                super::crosscheck_redaction::project(material, &tokens);
            }
            Ok((source, materials, maps))
        })?;
        let step_id = StepId::new();
        let mut work = vec![];
        let mut assignments = vec![];
        let mut projections = vec![];
        for (target, material) in materials {
            let case = (target == Target::Judge).then(CaseId::new);
            let context = if let Some(case) = case {
                ContextId(format!("judge:{case}"))
            } else {
                ContextId(format!("contest:{id}:{step_id}:{target}"))
            };
            let work_id = WorkId::new();
            assignments.push(ContestAssignment {
                target: target.clone(),
                context_id: context.clone(),
                payload_refs: vec![source.base.id],
                work_id: Some(work_id),
            });
            // Only the typed authorized material is projected; metadata uses this
            // router-created step source rather than a private human transcript.
            let mut projection = squad_store::history::attributed(
                &source,
                context.clone(),
                "crosscheck.input",
                HistoryRole::User,
                Some("squad".into()),
                serde_json::to_string(&material)?,
            );
            if case.is_none() {
                projection.meta = HistoryMetadata::Anonymous { anonymous: True };
            }
            projections.push(projection);
            private_maps.insert(
                format!("crosscheck.input:{work_id}"),
                serde_json::to_value(&material)?,
            );
            work.push(Work {
                work_id,
                message_id: MessageId::new(),
                in_reply_to_message_id: None,
                kind: if case.is_some() {
                    WorkKind::CrosscheckJudge
                } else {
                    WorkKind::CrosscheckMember
                },
                target,
                context_id: context,
                trigger_event_id: source.base.id,
                history_through: HistoryPosition::new(1).expect("positive"),
                attempt: 1,
                debate_id: None,
                round: None,
                replay: None,
                case_id: case,
                prompt_ref: None,
                max_output_tokens: Some(if case.is_some() { 4096 } else { 2048 }),
                contest_id: Some(id),
                step_id: Some(step_id),
            });
        }
        let ordinal = match state.status {
            CrosscheckStatus::Extracting => 1,
            CrosscheckStatus::Inventorying => 2,
            CrosscheckStatus::BuildingGrid => 3,
            CrosscheckStatus::Reviewing => 4,
            CrosscheckStatus::Auditing => 5,
            CrosscheckStatus::Ruling => 6,
            CrosscheckStatus::Reporting => 7,
            _ => return Err(store_error()),
        };
        self.allocate_step(
            EventId::new(),
            ContestStep {
                contest_id: id,
                step_id,
                ordinal,
                script_version: Text::new("crosscheck-v1").expect("literal"),
                move_name: if work.len() == 1 && work[0].target == Target::Judge {
                    ContestMove::Judge
                } else if work.is_empty() {
                    ContestMove::Report
                } else {
                    ContestMove::Fanout
                },
                inputs: vec![source.base.id],
                assignments,
            },
            projections,
            work,
            private_maps,
        )
        .map(Some)
    }
}
/// Canonical payload for router validation of every proposed delivery.
pub(crate) fn expected_material(
    db: &Connection,
    state: &CrosscheckState,
    target: &Target,
) -> StoreResult<CrosscheckCaseMaterial> {
    let id = state.contest_id;
    let setup: CrosscheckSetup = adapter::map(db, id, "crosscheck.setup")?;
    let routing: PrivateRouting = adapter::map(db, id, "crosscheck.routing")?;
    let member = || -> StoreResult<&MemberNick> {
        let Target::Member(n) = target else {
            return Err(store_error());
        };
        if !setup.participants.contains(n) {
            return Err(store_error());
        }
        Ok(n)
    };
    let judge = || -> StoreResult<()> {
        if *target != Target::Judge {
            return Err(store_error());
        }
        Ok(())
    };
    let mut material = match state.status {
        CrosscheckStatus::Extracting => {
            judge()?;
            CrosscheckCaseMaterial::ExtractGoal { plan: setup.plan }
        }
        CrosscheckStatus::Inventorying => {
            let nick = member()?;
            let i = setup
                .participants
                .iter()
                .position(|n| n == nick)
                .ok_or_else(store_error)?;
            CrosscheckCaseMaterial::Inventory {
                goal: setup.goal.ok_or_else(store_error)?,
                lens: routing.lens(i),
            }
        }
        CrosscheckStatus::BuildingGrid => {
            judge()?;
            CrosscheckCaseMaterial::NormalizeInventory {
                goal: setup.goal.ok_or_else(store_error)?,
                inventories: load_inventories(db, id)?
                    .into_iter()
                    .map(|(e, i)| (e, i.lens, i.response))
                    .collect(),
            }
        }
        CrosscheckStatus::Reviewing => {
            let nick = member()?;
            let grid: CrosscheckGrid = adapter::map(db, id, "crosscheck.grid")?;
            review_material(&setup, &grid, nick)
        }
        CrosscheckStatus::Auditing => {
            let nick = member()?;
            let grid: CrosscheckGrid = adapter::map(db, id, "crosscheck.grid")?;
            let reviews: Vec<CrosscheckReview> = adapter::map(db, id, "crosscheck.reviews")?;
            let selections =
                audit_selections(&setup.participants, &grid, &reviews, routing.tie_break_seed);
            let selected = selections
                .iter()
                .find(|s| &s.auditor == nick)
                .ok_or_else(store_error)?;
            CrosscheckCaseMaterial::Audit {
                plan: setup.plan,
                grid: grid.cells.iter().map(visible).collect(),
                samples: reviews
                    .iter()
                    .flat_map(|r| &r.response.cells)
                    .filter(|c| selected.sample.contains(&c.cell_id))
                    .cloned()
                    .collect(),
            }
        }
        CrosscheckStatus::Ruling => {
            judge()?;
            CrosscheckCaseMaterial::Rule {
                plan: setup.plan,
                grid: adapter::map(db, id, "crosscheck.grid")?,
                reviews: adapter::map(db, id, "crosscheck.reviews")?,
                audits: adapter::map(db, id, "crosscheck.audits")?,
            }
        }
        _ => return Err(store_error()),
    };
    let tokens: Vec<String> = adapter::map(db, id, "crosscheck.redaction")?;
    super::crosscheck_redaction::project(&mut material, &tokens);
    Ok(material)
}
