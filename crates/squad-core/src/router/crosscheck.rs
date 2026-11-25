//! Crosscheck adapter for the shared admission, step, work and barrier engine.
use super::{Router, envelope};
use crate::orchestrator::crosscheck::{self as script, PrivateRouting};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    super::debates::invalid()
}
fn event(payload: EnvelopePayload) -> ProducerEnvelope {
    envelope(EventId::new(), Actor::System, vec![Recipient::Me], payload)
}
pub fn save(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    state: &CrosscheckState,
    expected: Option<u64>,
) -> Result<()> {
    let seq = group.push(tx, event(EnvelopePayload::CrosscheckState(state.clone())))?;
    squad_store::crosscheck::save_state(tx, state, expected, seq)?;
    if state.status.is_terminal() {
        squad_store::crosscheck::release_plan(tx, state.contest_id)?;
    }
    Ok(())
}
pub fn map<T: serde::de::DeserializeOwned>(db: &Connection, id: ContestId, key: &str) -> Result<T> {
    let raw: String = db.query_row(
        "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",
        params![id.to_string(), key],
        |r| r.get(0),
    )?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    Ok(serde_json::from_value(value["value"].clone())?)
}
pub fn put(tx: &Connection, id: ContestId, key: &str, value: &impl serde::Serialize) -> Result<()> {
    tx.execute("INSERT INTO contest_private_maps VALUES (?,?,?) ON CONFLICT(contest_id,scope_key) DO UPDATE SET map_json=excluded.map_json",params![id.to_string(),key,serde_json::json!({"value":value}).to_string()])?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    parsed: ParsedInput,
    bindings: &BTreeMap<String, ExecutorBinding>,
    daily_max: i64,
    threshold: f64,
    preconfirmed: bool,
    config: Option<&SquadConfig>,
) -> Result<RequestResult> {
    let ParsedInput::Crosscheck {
        participants,
        plan,
        goal_constraints,
        ..
    } = parsed
    else {
        return Err(invalid());
    };
    let source = match plan {
        parser::PlanSource::Pasted { text } => text,
        parser::PlanSource::Quoted { event_id } => {
            super::references::quotes(tx, &[event_id])?.remove(0).text
        }
        parser::PlanSource::LatestReply { nick } => {
            let mut q =
                tx.prepare("SELECT seq FROM events WHERE kind='message.final' ORDER BY seq DESC")?;
            let seqs = q
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut found = None;
            for seq in seqs {
                let e = squad_store::events::at(tx, seq)?;
                if e.base.sender == Actor::Member(nick.clone())
                    && let EnvelopePayload::MessageFinal(m) = e.content
                {
                    found = Some(m.content);
                    break;
                }
            }
            found.ok_or_else(invalid)?
        }
    };
    let contest_id = ContestId::new();
    let plan_id = PlanId::new();
    let setup = CrosscheckSetup {
        contest_id,
        plan_id,
        plan_version: 1,
        plan: Text::new(source).map_err(|_| invalid())?,
        participants: participants.clone(),
        goal: goal_constraints.map(|goal| CrosscheckGoal {
            goal: Text::new(goal).expect("parser nonempty brief"),
            constraints: vec![],
            uncertainties: vec![],
        }),
    };
    setup.validate().map_err(|_| invalid())?;
    let redaction = crate::orchestrator::crosscheck_redaction::tokens(
        config,
        &setup.participants,
        bindings.values().map(|b| b.model.to_string()),
    );
    let extraction = setup.goal.is_none();
    let m = participants.len() as u32;
    let reserved_attempts = 4 * m + 10 + 3 * u32::from(extraction);
    let mut estimated = 0.;
    let mut reserved = 0.;
    for target in participants
        .iter()
        .map(|n| n.as_str())
        .chain(std::iter::once("judge"))
    {
        let b = bindings.get(target).ok_or(StoreError::Domain(
            ErrorCode::MODEL_UNAVAILABLE,
            "Crosscheck model unavailable",
        ))?;
        let pricing: String = tx.query_row(
            "SELECT pricing_json FROM catalog WHERE slug=?",
            [b.model.as_str()],
            |r| r.get(0),
        )?;
        let p: serde_json::Value = serde_json::from_str(&pricing)?;
        let price = |key: &str| {
            p[key]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or(p[key].as_f64())
                .unwrap_or(0.)
        };
        let unit = 32768. * price("prompt") + (4096. + 16384.) * price("completion");
        let calls = if target == "judge" {
            2 + u32::from(extraction)
        } else {
            2
        } as f64;
        estimated += calls * unit;
        reserved += calls * unit * if target == "judge" { 3. } else { 2. };
        if target != "judge" {
            estimated += 2. * unit / m as f64;
            reserved += 4. * unit / m as f64;
        }
    }
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let reserve = squad_store::accounting::reservation_micros(reserved).map_err(|_| invalid())?;
    match squad_store::accounting::admit(tx, &day, reserve, daily_max) {
        Ok(()) => {}
        Err(StoreError::Domain(ErrorCode::DAILY_MAX_REACHED, _)) => {
            return Ok(RequestResult::Rejected {
                request_id: id,
                error: super::daily_cap_error(tx, &day, reserve, daily_max),
            });
        }
        Err(StoreError::Domain(code, text)) => {
            return Ok(RequestResult::Rejected {
                request_id: id,
                error: super::error(code, text),
            });
        }
        Err(error) => return Err(error),
    }
    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get(0),
    )?;
    if active {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running",
        ));
    }
    let preflight = CrosscheckPreflight {
        base: ContestPreflight {
            contest_id,
            kind: ContestKind::Crosscheck,
            plan_id,
            expires_at: group.accepted_at + chrono::Duration::milliseconds(squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64),
            estimate: DebateEstimate {
                estimated_usd: estimated,
                reserved_usd: reserved,
                remaining_usd: daily_max as f64 / 1e6,
                model_calls: 2 * m + 4 + u32::from(extraction),
            },
            participants,
            confirmation_required: estimated > threshold,
            warning: None,
            settings: serde_json::json!({"extractionRequired":extraction}),
        },
        extraction_required: extraction,
        reserved_attempts,
    };
    let state = CrosscheckState {
        contest_id,
        plan_id,
        plan_version: 1,
        grid_version: 0,
        state_version: 1,
        status: CrosscheckStatus::Preflight,
        step_id: None,
        terminal_reason: None,
    };
    save(tx, group, &state, None)?;
    let quotes = super::references::quotes(tx, input.quote.as_deref().unwrap_or_default())?;
    let mut human = envelope(
        id,
        Actor::Me,
        vec![Recipient::Me],
        EnvelopePayload::UserMessage(UserMessage {
            input,
            body: setup.plan.to_string(),
            resolved_quotes: quotes,
            debate_id: None,
            case_id: None,
            contest_id: Some(contest_id),
        }),
    );
    human.base.id = id;
    let seq = group.push(tx, human)?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Crosscheck, seq)?;
    for payload in [
        EnvelopePayload::CrosscheckSetup(setup.clone()),
        EnvelopePayload::CrosscheckPreflight(preflight.clone()),
    ] {
        let seq = group.push(tx, event(payload))?;
        squad_store::contests::associate(tx, contest_id, ContestKind::Crosscheck, seq)?;
    }
    put(tx, contest_id, "crosscheck.redaction", &redaction)?;
    put(tx, contest_id, "crosscheck.setup", &setup)?;
    put(
        tx,
        contest_id,
        "crosscheck.routing",
        &PrivateRouting::draw(),
    )?;
    tx.execute(
        "UPDATE contests SET plan_id=?,plan_json=?,expires_at=? WHERE contest_id=?",
        params![
            plan_id.to_string(),
            serde_json::to_string(&preflight)?,
            preflight.base.expires_at.to_rfc3339(),
            contest_id.to_string()
        ],
    )?;
    tx.execute(
        "UPDATE contests SET pending_request_json=? WHERE contest_id=?",
        params![
            serde_json::json!({"dailyMax":daily_max}).to_string(),
            contest_id.to_string()
        ],
    )?;
    if !preflight.base.confirmation_required || preconfirmed {
        resolve(
            tx,
            group,
            id,
            contest_id,
            plan_id,
            false,
            daily_max,
            if preconfirmed {
                Confirmation::Preconfirmed
            } else {
                Confirmation::Automatic
            },
        )
    } else {
        let RequestResult::Accepted {
            cursor: Some(cursor),
            ..
        } = group.accepted(id)
        else {
            return Err(invalid());
        };
        Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(preflight.base),
            cursor,
        })
    }
}
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan_id: PlanId,
    cancel: bool,
    max: i64,
    confirmation: Confirmation,
) -> Result<RequestResult> {
    let state = squad_store::crosscheck::state(tx, contest)?;
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let plan: CrosscheckPreflight = serde_json::from_str(&raw)?;
    if state.status != CrosscheckStatus::Preflight || state.plan_id != plan_id {
        return Err(invalid());
    }
    let mut next = state.clone();
    next.state_version += 1;
    let status = if cancel {
        next.status = CrosscheckStatus::Stopped;
        PreflightStatus::Cancelled
    } else if group.accepted_at >= plan.base.expires_at {
        next.status = CrosscheckStatus::Failed;
        PreflightStatus::Expired
    } else {
        let reserve = squad_store::accounting::reservation_micros(plan.base.estimate.reserved_usd)
            .map_err(|_| invalid())?;
        squad_store::accounting::reserve(
            tx,
            &format!("plan:{plan_id}"),
            None,
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
            reserve,
            max,
        )?;
        tx.execute(
            "UPDATE reservations SET plan_id=?,contest_id=? WHERE reservation_id=?",
            params![
                plan_id.to_string(),
                contest.to_string(),
                format!("plan:{plan_id}")
            ],
        )?;
        next.status = CrosscheckStatus::WaitingForParticipants;
        PreflightStatus::Confirmed
    };
    let seq = group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: contest,
                plan_id,
                confirmation: (status == PreflightStatus::Confirmed).then_some(confirmation),
                status,
                error: None,
            }),
        ),
    )?;
    squad_store::contests::associate(tx, contest, ContestKind::Crosscheck, seq)?;
    save(tx, group, &next, Some(state.state_version))?;
    tx.execute(
        "UPDATE contests SET expires_at=NULL WHERE contest_id=?",
        [contest.to_string()],
    )?;
    Ok(group.accepted(id))
}
#[allow(clippy::too_many_arguments)]
pub fn allocate(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    step: ContestStep,
    projections: Vec<HistoryProjection>,
    work: Vec<Work>,
    private_maps: BTreeMap<String, serde_json::Value>,
    bindings: &BTreeMap<String, ExecutorBinding>,
) -> Result<()> {
    let mut state = squad_store::crosscheck::state(tx, step.contest_id)?;
    if state.status.is_terminal()
        || state.status == CrosscheckStatus::Preflight
        || state.step_id.is_some()
    {
        return Err(invalid());
    }
    let setup: CrosscheckSetup = map(tx, step.contest_id, "crosscheck.setup")?;
    let (ordinal, actors) = match state.status {
        CrosscheckStatus::Extracting => (1, vec![Target::Judge]),
        CrosscheckStatus::Inventorying => (
            2,
            setup
                .participants
                .iter()
                .cloned()
                .map(Target::Member)
                .collect(),
        ),
        CrosscheckStatus::BuildingGrid => (3, vec![Target::Judge]),
        CrosscheckStatus::Reviewing => (
            4,
            setup
                .participants
                .iter()
                .cloned()
                .map(Target::Member)
                .collect(),
        ),
        CrosscheckStatus::Auditing => {
            let grid: CrosscheckGrid = map(tx, state.contest_id, "crosscheck.grid")?;
            let reviews: Vec<CrosscheckReview> = map(tx, state.contest_id, "crosscheck.reviews")?;
            let routing: PrivateRouting = map(tx, state.contest_id, "crosscheck.routing")?;
            (
                5,
                script::audit_selections(
                    &setup.participants,
                    &grid,
                    &reviews,
                    routing.tie_break_seed,
                )
                .into_iter()
                .map(|s| Target::Member(s.auditor))
                .collect(),
            )
        }
        CrosscheckStatus::Ruling => (6, vec![Target::Judge]),
        CrosscheckStatus::Reporting => (7, vec![]),
        _ => return Err(invalid()),
    };
    let expected_move = if actors.is_empty() {
        ContestMove::Report
    } else if actors == vec![Target::Judge] {
        ContestMove::Judge
    } else {
        ContestMove::Fanout
    };
    if step.ordinal != ordinal
        || step.move_name != expected_move
        || step.script_version.as_str() != "crosscheck-v1"
        || work.iter().map(|w| &w.target).ne(actors.iter())
        || step.assignments.iter().map(|a| &a.target).ne(actors.iter())
        || projections.len() != work.len()
    {
        return Err(invalid());
    }
    let version = state.state_version;
    state.step_id = Some(step.step_id);
    state.state_version += 1;
    let seq = group.push(tx, event(EnvelopePayload::ContestStep(step.clone())))?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (key, value) in private_maps {
        if key != "crosscheck.samples"
            && !work
                .iter()
                .any(|w| key == format!("crosscheck.input:{}", w.work_id))
        {
            return Err(super::authorization::forbidden());
        }
        put(tx, step.contest_id, &key, &value)?;
    }
    for p in projections {
        let assignment = step
            .assignments
            .iter()
            .find(|a| a.context_id == p.context_id)
            .ok_or_else(super::authorization::forbidden)?;
        let work_id = assignment.work_id.ok_or_else(invalid)?;
        let material: CrosscheckCaseMaterial =
            map(tx, step.contest_id, &format!("crosscheck.input:{work_id}"))?;
        if material != script::expected_material(tx, &state, &assignment.target)?
            || p.content != serde_json::to_string(&material)?
            || p.role != HistoryRole::User
            || p.name.as_deref() != Some("squad")
            || !assignment.payload_refs.contains(&p.origin_event_id)
            || (assignment.target != Target::Judge
                && p.meta != HistoryMetadata::Anonymous { anonymous: True })
        {
            return Err(super::authorization::forbidden());
        }
        squad_store::history::project(tx, &p)?;
    }
    if state.status == CrosscheckStatus::Auditing {
        let selections: Vec<script::AuditSelection> =
            map(tx, step.contest_id, "crosscheck.samples")?;
        let setup: CrosscheckSetup = map(tx, step.contest_id, "crosscheck.setup")?;
        let grid: CrosscheckGrid = map(tx, step.contest_id, "crosscheck.grid")?;
        let reviews: Vec<CrosscheckReview> = map(tx, step.contest_id, "crosscheck.reviews")?;
        let routing: PrivateRouting = map(tx, step.contest_id, "crosscheck.routing")?;
        if selections
            != script::audit_selections(
                &setup.participants,
                &grid,
                &reviews,
                routing.tie_break_seed,
            )
        {
            return Err(invalid());
        }
        squad_store::crosscheck::freeze_samples(
            tx,
            step.step_id,
            &selections
                .into_iter()
                .map(|s| (s.auditor, s.sample))
                .collect::<Vec<_>>(),
        )?;
    }
    for mut w in work {
        let material: CrosscheckCaseMaterial = map(
            tx,
            step.contest_id,
            &format!("crosscheck.input:{}", w.work_id),
        )?;
        if material != script::expected_material(tx, &state, &w.target)?
            || w.kind != material.work_kind()
        {
            return Err(super::authorization::forbidden());
        }
        let judge = matches!(
            material,
            CrosscheckCaseMaterial::ExtractGoal { .. }
                | CrosscheckCaseMaterial::NormalizeInventory { .. }
                | CrosscheckCaseMaterial::Rule { .. }
        );
        if w.contest_id != Some(step.contest_id)
            || w.step_id != Some(step.step_id)
            || judge != (w.target == Target::Judge)
        {
            return Err(invalid());
        }
        let boundary: i64 = tx.query_row(
            "SELECT MAX(pos) FROM history WHERE context_id=?",
            [&w.context_id.0],
            |r| r.get(0),
        )?;
        w.history_through = HistoryPosition::new(boundary).map_err(|_| invalid())?;
        let b = bindings.get(&w.target.to_string()).ok_or_else(invalid)?;
        if let Some(case) = w.case_id {
            tx.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![case.to_string(),w.context_id.0,step.contest_id.to_string(),b.model.as_str(),serde_json::to_string(&material)?,seq.get()])?;
        }
        let mut binding = b.clone();
        if judge {
            binding.effort = ReasoningEffort::Medium;
        }
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step.step_id, &w)?;
        tx.execute(
            "UPDATE work SET execution_deadline=? WHERE work_id=?",
            params![
                (group.accepted_at + chrono::Duration::seconds(if judge { 300 } else { 180 }))
                    .to_rfc3339(),
                w.work_id.to_string()
            ],
        )?;
        group.push(
            tx,
            event(EnvelopePayload::WorkState(WorkStateEvent {
                work_id: w.work_id,
                trigger_event_id: w.trigger_event_id,
                target: w.target,
                context_id: w.context_id,
                attempt: 0,
                status: WorkStatus::Queued,
                error: None,
                debate_id: None,
                case_id: w.case_id,
                contest_id: w.contest_id,
            })),
        )?;
    }
    squad_store::contests::associate(tx, step.contest_id, ContestKind::Crosscheck, seq)?;
    save(tx, group, &state, Some(version))
}
pub fn validate_artifact(tx: &Connection, work: &Work, artifact: &ContestArtifact) -> Result<()> {
    let state = squad_store::crosscheck::state(tx, artifact.contest_id)?;
    if state.status.is_terminal() || state.step_id != Some(artifact.step_id) {
        return Err(invalid());
    }
    let material: CrosscheckCaseMaterial = map(
        tx,
        artifact.contest_id,
        &format!("crosscheck.input:{}", work.work_id),
    )?;
    if artifact.artifact_kind.as_str() != material.composition()
        || work.kind != material.work_kind()
    {
        return Err(invalid());
    }
    squad_agents::agents::crosscheck::validate_response(&material, artifact.data.clone())
        .map_err(|_| invalid())?;
    if let CrosscheckCaseMaterial::NormalizeInventory { .. } = material {
        let setup: CrosscheckSetup = map(tx, artifact.contest_id, "crosscheck.setup")?;
        let inventories =
            super::super::orchestrator::crosscheck::load_inventories(tx, artifact.contest_id)?;
        let routing: PrivateRouting = map(tx, artifact.contest_id, "crosscheck.routing")?;
        script::build_grid(
            &setup,
            serde_json::from_value(artifact.data.clone())?,
            &inventories,
            &routing,
        )
        .map_err(|_| invalid())?;
    }
    Ok(())
}
impl Router {
    pub fn crosscheck_state(&self, id: ContestId) -> Result<CrosscheckState> {
        self.store
            .call(move |db| squad_store::crosscheck::state(db, id))
    }
}
pub fn barrier(tx: &Transaction<'_>, group: &mut CommitGroup, commit: BarrierCommit) -> Result<()> {
    let state = squad_store::crosscheck::state(tx, commit.contest_id)?;
    if state.state_version != commit.expected_state_version
        || state.step_id != Some(commit.step_id)
        || state.status.is_terminal()
        || !commit.next_work.is_empty()
        || !commit.projections.is_empty()
    {
        return Err(invalid());
    }
    let EnvelopePayload::ContestReport(report) = &commit.barrier_event.content else {
        return Err(invalid());
    };
    if report.contest_id != state.contest_id || report.step_id != commit.step_id {
        return Err(invalid());
    }
    squad_store::contest_steps::validate_report(tx, report)?;
    let outputs = script::step_outputs(tx, commit.step_id)?;
    let skipped = outputs
        .iter()
        .filter(|(_, a)| a.is_none())
        .map(|(w, _)| w.work_id)
        .collect::<std::collections::BTreeSet<_>>();
    if skipped != report.skipped_work_ids.iter().copied().collect()
        || skipped.len() != report.skipped_work_ids.len()
    {
        return Err(invalid());
    }
    for (work, output) in &outputs {
        if output.is_none() {
            let record = squad_store::work::record(tx, work.work_id)?.ok_or_else(invalid)?;
            if matches!(record.status, WorkStatus::Queued | WorkStatus::Running) {
                // A restart may leave a consumed attempt with no accepted result.
                let exhausted = record.attempt
                    >= if work.kind.is_crosscheck_judge() {
                        3
                    } else {
                        2
                    };
                let expired = record
                    .deadline
                    .is_some_and(|deadline| deadline <= group.accepted_at);
                if !exhausted && !expired {
                    return Err(invalid());
                }
                tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1 WHERE work_id=?",[work.work_id.to_string()])?;
            }
            squad_store::contest_steps::settle(
                tx,
                commit.step_id,
                &work.target,
                Some(&super::error(
                    ErrorCode::PROVIDER_ERROR,
                    "Crosscheck assignment failed",
                )),
            )?;
        }
    }
    let mut setup: CrosscheckSetup = map(tx, state.contest_id, "crosscheck.setup")?;
    let single = || {
        outputs
            .first()
            .and_then(|(_, a)| a.as_ref())
            .map(|a| a.data.clone())
            .ok_or_else(invalid)
    };
    let signal = match state.status {
        CrosscheckStatus::Extracting => {
            if let Ok(data) = single() {
                let goal: CrosscheckGoal = serde_json::from_value(data)?;
                setup.goal = Some(goal.clone());
                put(tx, state.contest_id, "crosscheck.setup", &setup)?;
                let seq = group.push(
                    tx,
                    event(EnvelopePayload::CrosscheckGoal(CrosscheckGoalRecord {
                        contest_id: state.contest_id,
                        work_id: outputs[0].0.work_id,
                        goal,
                    })),
                )?;
                squad_store::contests::associate(
                    tx,
                    state.contest_id,
                    ContestKind::Crosscheck,
                    seq,
                )?;
                CrosscheckTransition::ExtractionCommitted
            } else {
                CrosscheckTransition::Fail
            }
        }
        CrosscheckStatus::Inventorying => {
            let routing: PrivateRouting = map(tx, state.contest_id, "crosscheck.routing")?;
            for (w, a) in &outputs {
                if let Some(a) = a {
                    let Target::Member(member) = &w.target else {
                        return Err(invalid());
                    };
                    let index = setup
                        .participants
                        .iter()
                        .position(|n| n == member)
                        .ok_or_else(invalid)?;
                    let inventory = CrosscheckInventory {
                        contest_id: state.contest_id,
                        work_id: w.work_id,
                        member: member.clone(),
                        lens: routing.lens(index),
                        response: serde_json::from_value(a.data.clone())?,
                    };
                    let seq =
                        group.push(tx, event(EnvelopePayload::CrosscheckInventory(inventory)))?;
                    squad_store::contests::associate(
                        tx,
                        state.contest_id,
                        ContestKind::Crosscheck,
                        seq,
                    )?;
                }
            }
            CrosscheckTransition::InventoriesCommitted
        }
        CrosscheckStatus::BuildingGrid => {
            if let Ok(data) = single() {
                let inventories = script::load_inventories(tx, state.contest_id)?;
                let routing: PrivateRouting = map(tx, state.contest_id, "crosscheck.routing")?;
                let grid = script::build_grid(
                    &setup,
                    serde_json::from_value(data)?,
                    &inventories,
                    &routing,
                )
                .map_err(|_| invalid())?;
                let seq = group.push(tx, event(EnvelopePayload::CrosscheckGrid(grid.clone())))?;
                squad_store::crosscheck::save_grid(tx, &grid, seq)?;
                put(tx, state.contest_id, "crosscheck.grid", &grid)?;
                CrosscheckTransition::GridCommitted
            } else {
                CrosscheckTransition::Fail
            }
        }
        CrosscheckStatus::Reviewing => {
            let mut reviews = vec![];
            for (w, a) in &outputs {
                if let Some(a) = a {
                    let Target::Member(reviewer) = &w.target else {
                        return Err(invalid());
                    };
                    let review = CrosscheckReview {
                        contest_id: state.contest_id,
                        work_id: w.work_id,
                        reviewer: reviewer.clone(),
                        response: serde_json::from_value(a.data.clone())?,
                    };
                    squad_store::crosscheck::validate_review(tx, &review, setup.plan.as_str())?;
                    let seq =
                        group.push(tx, event(EnvelopePayload::CrosscheckReview(review.clone())))?;
                    squad_store::crosscheck::save_review(tx, &review, seq)?;
                    squad_store::contests::associate(
                        tx,
                        state.contest_id,
                        ContestKind::Crosscheck,
                        seq,
                    )?;
                    reviews.push(review);
                }
            }
            put(tx, state.contest_id, "crosscheck.reviews", &reviews)?;
            CrosscheckTransition::ReviewsCommitted
        }
        CrosscheckStatus::Auditing => {
            let selections: Vec<script::AuditSelection> =
                map(tx, state.contest_id, "crosscheck.samples")?;
            let mut audits = vec![];
            for (w, a) in &outputs {
                if let Some(a) = a {
                    let Target::Member(auditor) = &w.target else {
                        return Err(invalid());
                    };
                    let sample = selections
                        .iter()
                        .find(|s| &s.auditor == auditor)
                        .ok_or_else(invalid)?
                        .sample
                        .clone();
                    let audit = CrosscheckAudit {
                        contest_id: state.contest_id,
                        work_id: w.work_id,
                        auditor: auditor.clone(),
                        sample,
                        response: serde_json::from_value(a.data.clone())?,
                    };
                    let seq =
                        group.push(tx, event(EnvelopePayload::CrosscheckAudit(audit.clone())))?;
                    tx.execute("UPDATE crosscheck_audit_samples SET audit_event_seq=? WHERE step_id=? AND auditor=? AND audit_event_seq IS NULL",params![seq.get(),commit.step_id.to_string(),auditor.as_str()])?;
                    squad_store::contests::associate(
                        tx,
                        state.contest_id,
                        ContestKind::Crosscheck,
                        seq,
                    )?;
                    audits.push(audit);
                }
            }
            put(tx, state.contest_id, "crosscheck.audits", &audits)?;
            CrosscheckTransition::AuditsCommitted
        }
        CrosscheckStatus::Ruling => {
            if let Ok(data) = single() {
                let ruling: CrosscheckRuling = serde_json::from_value(data)?;
                let grid: CrosscheckGrid = map(tx, state.contest_id, "crosscheck.grid")?;
                let audits: Vec<CrosscheckAudit> = map(tx, state.contest_id, "crosscheck.audits")?;
                ruling
                    .validate(&grid, &audits, setup.plan.as_str())
                    .map_err(|_| invalid())?;
                put(tx, state.contest_id, "crosscheck.ruling", &ruling)?;
                let seq = group.push(
                    tx,
                    event(EnvelopePayload::CrosscheckRuling(CrosscheckRulingRecord {
                        contest_id: state.contest_id,
                        work_id: outputs[0].0.work_id,
                        ruling: ruling.clone(),
                    })),
                )?;
                squad_store::contests::associate(
                    tx,
                    state.contest_id,
                    ContestKind::Crosscheck,
                    seq,
                )?;
                for cell in &ruling.cells {
                    let status = serde_json::to_value(cell.status)?;
                    tx.execute("UPDATE crosscheck_cells SET status=?,ruling_event_seq=?,evidence_json=? WHERE contest_id=? AND cell_id=? AND review_event_seq IS NOT NULL",params![status.as_str(),seq.get(),serde_json::to_string(cell)?,state.contest_id.to_string(),cell.cell_id.to_string()])?;
                }
                CrosscheckTransition::RulingCommitted
            } else {
                CrosscheckTransition::Fail
            }
        }
        CrosscheckStatus::Reporting => {
            let grid: CrosscheckGrid = map(tx, state.contest_id, "crosscheck.grid")?;
            let reviews: Vec<CrosscheckReview> = map(tx, state.contest_id, "crosscheck.reviews")?;
            let audits: Vec<CrosscheckAudit> = map(tx, state.contest_id, "crosscheck.audits")?;
            let ruling: CrosscheckRuling = map(tx, state.contest_id, "crosscheck.ruling")?;
            let selections: Vec<script::AuditSelection> =
                map(tx, state.contest_id, "crosscheck.samples")?;
            let report = script::report(&setup, &grid, &reviews, &audits, &ruling, &selections);
            let seq = group.push(tx, event(EnvelopePayload::CrosscheckReport(report.clone())))?;
            // Audit additions have ruling origin, never counterfeit grid origin.
            let ruling_seq:i64=tx.query_row("SELECT MAX(event_seq) FROM contest_events WHERE contest_id=? AND event_seq IN (SELECT seq FROM events WHERE kind='crosscheck.ruling')",[state.contest_id.to_string()],|r|r.get(0))?;
            for cell in report
                .ledger
                .iter()
                .filter(|c| !grid.cells.iter().any(|g| g.cell_id == c.cell.cell_id))
            {
                tx.execute("INSERT INTO crosscheck_cells(contest_id,cell_id,origin_event_seq,items_json,originators_json,status,evidence_json) VALUES (?,?,?,?,?,'gap',?)",params![state.contest_id.to_string(),cell.cell.cell_id.to_string(),ruling_seq,serde_json::to_string(&cell.cell.items)?,serde_json::to_string(&cell.cell.originators)?,serde_json::to_string(&cell.ruling)?])?;
            }
            put(tx, state.contest_id, "crosscheck.report", &report)?;
            squad_store::contests::associate(tx, state.contest_id, ContestKind::Crosscheck, seq)?;
            CrosscheckTransition::ReportCommitted
        }
        _ => return Err(invalid()),
    };
    let seq = group.push(tx, commit.barrier_event)?;
    squad_store::contest_steps::barrier(tx, commit.step_id, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Crosscheck, seq)?;
    let next = script::advance(&state, &signal, setup.goal.is_none()).map_err(|_| invalid())?;
    save(tx, group, &next, Some(state.state_version))
}
#[allow(clippy::too_many_arguments)]
pub fn accept(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    version: u64,
    plan_event: EventId,
    grid_event: EventId,
    cell_ids: Vec<CellId>,
    reason: Text,
) -> Result<()> {
    let state = squad_store::crosscheck::state(tx, contest)?;
    let reference = |kind: &str| -> Result<EventId> {
        let raw:String=tx.query_row("SELECT e.id FROM events e JOIN contest_events c ON c.event_seq=e.seq WHERE c.contest_id=? AND e.kind=? ORDER BY e.seq DESC LIMIT 1",params![contest.to_string(),kind],|r|r.get(0))?;
        raw.parse().map_err(|_| invalid())
    };
    if reference("crosscheck.setup")? != plan_event || reference("crosscheck.grid")? != grid_event {
        return Err(invalid());
    }
    let mut report: CrosscheckReport = map(tx, contest, "crosscheck.report")?;
    let residual_ids = cell_ids
        .iter()
        .filter(|id| {
            report
                .residual_risks
                .iter()
                .any(|r| r.id.as_str() == id.to_string())
        })
        .map(|id| Text::new(id.to_string()).expect("UUID"))
        .collect::<Vec<_>>();
    let retained_ids = cell_ids
        .iter()
        .filter(|id| !residual_ids.iter().any(|r| r.as_str() == id.to_string()))
        .copied()
        .collect::<Vec<_>>();
    let acceptance = CrosscheckAcceptance {
        contest_id: contest,
        plan_id: state.plan_id,
        plan_version: state.plan_version,
        grid_version: state.grid_version,
        expected_state_version: version,
        cell_ids: retained_ids,
        residual_risk_ids: residual_ids.clone(),
        reason,
    };
    let mut carrier = event(EnvelopePayload::CrosscheckAcceptance(acceptance.clone()));
    carrier.base.sender = Actor::Me;
    carrier.base.id = id;
    let seq = group.push(tx, carrier)?;
    squad_store::crosscheck::accept(tx, &Actor::Me, &acceptance, seq)?;
    for risk in &mut report.residual_risks {
        if residual_ids.contains(&risk.id) {
            risk.acceptance_event_id = Some(id);
        }
    }
    for cell in &mut report.ledger {
        if cell_ids.contains(&cell.cell.cell_id) {
            cell.acceptance_event_id = Some(id);
        }
    }
    script::recompute_stop(&mut report, &[]);
    put(tx, contest, "crosscheck.report", &report)?;
    let seq = group.push(tx, event(EnvelopePayload::CrosscheckReport(report)))?;
    squad_store::contests::associate(tx, contest, ContestKind::Crosscheck, seq)?;
    let mut next = state.clone();
    next.state_version += 1;
    save(tx, group, &next, Some(state.state_version))
}
