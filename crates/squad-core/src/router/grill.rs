//! Transactional shared-floor admission, assignments and barriers.
use super::{envelope, error};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION, "Invalid grill move")
}
pub fn recipients(setup: &GrillSetup) -> Vec<Recipient> {
    std::iter::once(Recipient::Me)
        .chain(setup.participants().into_iter().map(Recipient::Member))
        .collect()
}
pub fn floor(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    payload: EnvelopePayload,
) -> Result<EventId> {
    let contest = match &payload {
        EnvelopePayload::GrillSetup(v) => v.contest_id,
        EnvelopePayload::GrillPlan(v) => v.contest_id,
        EnvelopePayload::GrillQuestion(v) => v.contest_id,
        EnvelopePayload::GrillAnswer(v) => v.contest_id,
        EnvelopePayload::GrillPass(v) => v.contest_id,
        EnvelopePayload::GrillComplete(v) => v.contest_id,
        _ => return Err(invalid()),
    };
    let setup = squad_store::grill::setup(tx, contest)?.setup;
    let sender = match &payload {
        EnvelopePayload::GrillQuestion(v) => Actor::Member(v.griller.clone()),
        EnvelopePayload::GrillAnswer(v) => Actor::Member(v.leader.clone()),
        EnvelopePayload::GrillPlan(v) if v.version > 1 => Actor::Member(v.leader.clone()),
        EnvelopePayload::GrillPass(v) if v.reason == GrillPassReason::Pass => {
            Actor::Member(v.griller.clone())
        }
        _ => Actor::System,
    };
    let seq = g.push(tx, envelope(id, sender, recipients(&setup), payload))?;
    let event = squad_store::events::at(tx, seq.get())?;
    squad_store::grill::retain(tx, &event)?;
    Ok(event.base.id)
}
pub fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    state: &GrillState,
) -> Result<()> {
    state.validate().map_err(|_| invalid())?;
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::GrillState(state.clone()),
        ),
    )?;
    if tx.execute("UPDATE contests SET state_json=?,state_version=?,active=?,updated_seq=? WHERE contest_id=? AND kind='grill' AND state_version=?",params![serde_json::to_string(state)?,state.state_version as i64,!state.state.is_terminal(),seq.get(),state.contest_id.to_string(),state.state_version as i64-1])?!=1{return Err(invalid());}
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Grill, seq)?;
    if state.state.is_terminal() {
        squad_store::accounting::release_contest(tx, state.contest_id)?;
        squad_store::history::release(tx, state.contest_id)?;
    }
    Ok(())
}
pub fn work(
    state: &GrillState,
    step: StepId,
    trigger: EventId,
    pack: &str,
    hash: &str,
) -> Result<Work> {
    let (kind, nick) = match state.state {
        GrillStatus::Questioning => (
            WorkKind::GrillQuestion,
            state.griller_order[state.turn as usize].clone(),
        ),
        GrillStatus::Answering => (WorkKind::GrillAnswer, state.leader.clone()),
        GrillStatus::Rebuilding => (WorkKind::GrillRebuild, state.leader.clone()),
        _ => return Err(invalid()),
    };
    Ok(Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        target: Target::Member(nick.clone()),
        context_id: ContextId(format!("contest:{}:{step}:{nick}", state.contest_id)),
        trigger_event_id: trigger,
        history_through: HistoryPosition::new(1).expect("positive"),
        attempt: 1,
        debate_id: None,
        round: Some(state.round),
        replay: None,
        case_id: None,
        prompt_ref: Some(PromptRef {
            pack_version: Text::new(pack).map_err(|_| invalid())?,
            template_id: Text::new(kind.grill_composition().expect("grill")).expect("literal"),
            template_hash: Text::new(hash).map_err(|_| invalid())?,
        }),
        max_output_tokens: Some(if kind == WorkKind::GrillRebuild {
            constants::PLAN_VISIBLE_TOKENS
        } else {
            constants::DEBATE_TURN_VISIBLE_TOKENS
        }),
        kind,
        contest_id: Some(state.contest_id),
        step_id: Some(step),
    })
}
pub fn prompt_key(state: &GrillState) -> String {
    format!(
        "{}:{}:{}:{}",
        state.round,
        state.turn,
        state.plan_version,
        state.state.as_str()
    )
}
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    setup: GrillSetup,
    bindings: &BTreeMap<String, ExecutorBinding>,
    max: i64,
    threshold: f64,
    preconfirmed: bool,
    config: &SquadConfig,
    replacing: bool,
) -> Result<RequestResult> {
    setup.validate().map_err(|_| invalid())?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1 AND contest_id<>?)",
        [setup.contest_id.to_string()],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(StoreError::Domain(
            ErrorCode::DEBATE_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let frozen = GrillPreflight {
        setup: setup.clone(),
        answer_words: config.prompts.limits.turn_words,
        plan_words: config.prompts.limits.plan_words,
    };
    let pack =
        crate::prompts::PromptPack::from_override_dir(config.prompts.directory.as_deref())
            .map_err(|_| StoreError::Domain(ErrorCode::CONFIG_INVALID, "Invalid grill prompts"))?;
    let mut prompts = serde_json::Map::new();
    let mut state = crate::orchestrator::grill::initial_state(&setup).map_err(|_| invalid())?;
    let mut nominal = 0.;
    let mut reserved = 0.;
    let mut frozen_bindings = BTreeMap::new();
    let mut models = serde_json::Map::new();
    for nick in setup.participants() {
        frozen_bindings.insert(
            nick.to_string(),
            bindings
                .get(nick.as_str())
                .ok_or(StoreError::Domain(
                    ErrorCode::MODEL_UNAVAILABLE,
                    "Missing grill model",
                ))?
                .clone(),
        );
    }
    let bound = setup.initial_plan.len() as u64
        + setup.focus.len() as u64
        + 32768
        + u64::from(setup.total_rounds) * setup.grillers.len() as u64 * 32768;
    for round in 1..=setup.total_rounds {
        state.round = round;
        for version in 1..=round {
            state.plan_version = version;
            state.griller_order = setup.grillers.clone();
            let len = state.griller_order.len();
            state.griller_order.rotate_left((round as usize - 1) % len);
            for turn in 0..len as u32 {
                state.turn = turn;
                for status in [
                    GrillStatus::Questioning,
                    GrillStatus::Answering,
                    GrillStatus::Rebuilding,
                ] {
                    if status == GrillStatus::Rebuilding && turn + 1 != len as u32 {
                        continue;
                    }
                    state.state = status;
                    let w = work(&state, StepId::new(), id, pack.version(), "pending")?;
                    let binding = &frozen_bindings[&w.target.to_string()];
                    let (composition, inputs) = pack
                        .grill_inputs(config, &w, binding, &frozen, &state)
                        .map_err(|_| invalid())?;
                    let rendered = pack
                        .render_composition(composition, &inputs, false)
                        .map_err(|_| invalid())?;
                    let hash = squad_store::commit::digest(&rendered)?;
                    prompts.insert(
                        prompt_key(&state),
                        serde_json::json!({"rendered":rendered,"hash":hash}),
                    );
                    let (length, pricing): (i64, String) = tx.query_row(
                        "SELECT context_length,pricing_json FROM catalog WHERE slug=?",
                        [binding.model.as_str()],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    models.insert(w.target.to_string(),serde_json::json!({"contextLength":length,"pricing":serde_json::from_str::<serde_json::Value>(&pricing)?,"binding":binding}));
                    let reasoning = match binding.effort {
                        ReasoningEffort::Low => constants::REASONING_LOW_TOKENS,
                        ReasoningEffort::Medium => constants::REASONING_MEDIUM_TOKENS,
                        ReasoningEffort::High => constants::REASONING_HIGH_TOKENS,
                    };
                    let output = w.max_output_tokens.unwrap_or(2048) + reasoning;
                    let input_bound = bound + serde_json::to_string(&rendered)?.len() as u64;
                    if input_bound + u64::from(output) + 4096 > length as u64 {
                        return Err(StoreError::Domain(
                            ErrorCode::CONTEXT_TOO_LARGE,
                            "Grill floor does not fit the model context",
                        ));
                    }
                    let prices: serde_json::Value = serde_json::from_str(&pricing)?;
                    let price = |key: &str| -> Result<f64> {
                        prices[key]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .or_else(|| prices[key].as_f64())
                            .filter(|v| v.is_finite() && *v >= 0.)
                            .ok_or(StoreError::Domain(
                                ErrorCode::MODEL_UNAVAILABLE,
                                "Grill pricing is unavailable",
                            ))
                    };
                    let cost = input_bound as f64 * price("prompt")?
                        + f64::from(output) * price("completion")?;
                    if version == round {
                        nominal += cost;
                        reserved += cost * 2.;
                    }
                }
            }
        }
    }
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let amount = squad_store::accounting::reservation_micros(reserved).map_err(|_| invalid())?;
    match squad_store::accounting::admit(tx, &day, amount, max) {
        Ok(()) => {}
        Err(StoreError::Domain(ErrorCode::DAILY_MAX_REACHED, _)) => {
            return Ok(RequestResult::Rejected {
                request_id: id,
                error: super::daily_cap_error(tx, &day, amount, max),
            });
        }
        Err(e) => return Err(e),
    }
    let spend = squad_store::accounting::daily(tx, &day)?;
    let plan = ContestPreflight {
        contest_id: setup.contest_id,
        kind: ContestKind::Grill,
        plan_id: PlanId::new(),
        expires_at: g.accepted_at
            + chrono::Duration::milliseconds(squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: nominal,
            reserved_usd: reserved,
            remaining_usd: (max - spend.spent_micros - spend.reserved_micros) as f64 / 1e6,
            model_calls: setup.normal_calls(),
        },
        participants: setup.participants(),
        confirmation_required: nominal > threshold && !preconfirmed,
        warning: None,
        settings: serde_json::to_value(&frozen)?,
    };
    state = crate::orchestrator::grill::initial_state(&setup).map_err(|_| invalid())?;
    if plan.confirmation_required {
        state.state = GrillStatus::Preflight;
    }
    if replacing {
        state.state_version = tx.query_row(
            "SELECT state_version+1 FROM contests WHERE contest_id=?",
            [setup.contest_id.to_string()],
            |r| r.get::<_, i64>(0),
        )? as u64;
        tx.execute("UPDATE contests SET state_version=?,state_json=?,plan_id=?,plan_json=?,expires_at=? WHERE contest_id=?",params![state.state_version as i64,serde_json::to_string(&state)?,plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),setup.contest_id.to_string()])?;
    } else {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at) VALUES (?,'grill',(SELECT epoch FROM room_meta),1,?,1,?,?,?)",params![setup.contest_id.to_string(),serde_json::to_string(&state)?,plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339()])?;
    }
    squad_store::contest_steps::private_map(
        tx,
        setup.contest_id,
        "grill.settings",
        &serde_json::to_value(&frozen)?,
    )?;
    squad_store::contest_steps::private_map(
        tx,
        setup.contest_id,
        "grill.provenance",
        &serde_json::json!({"bindings":frozen_bindings,"models":models,"prompts":prompts,"packVersion":pack.version()}),
    )?;
    if !replacing {
        let quotes = super::references::quotes(tx, input.quote.as_deref().unwrap_or_default())?;
        let mut event = envelope(
            id,
            Actor::Me,
            vec![Recipient::Me],
            EnvelopePayload::UserMessage(UserMessage {
                input,
                body: setup.focus.clone(),
                resolved_quotes: quotes,
                debate_id: None,
                case_id: None,
                contest_id: Some(setup.contest_id),
            }),
        );
        event.base.id = id;
        let seq = g.push(tx, event)?;
        squad_store::contests::associate(tx, setup.contest_id, ContestKind::Grill, seq)?;
    }
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::GrillState(state),
        ),
    )?;
    squad_store::contests::associate(tx, setup.contest_id, ContestKind::Grill, seq)?;
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPreflight(plan.clone()),
        ),
    )?;
    squad_store::contests::associate(tx, setup.contest_id, ContestKind::Grill, seq)?;
    if plan.confirmation_required {
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(plan),
            cursor: seq,
        });
    }
    reserve(tx, &plan, max)?;
    preflight_result(
        tx,
        g,
        id,
        &plan,
        PreflightStatus::Confirmed,
        Some(if preconfirmed {
            Confirmation::Preconfirmed
        } else {
            Confirmation::Automatic
        }),
        None,
    )?;
    Ok(g.accepted(id))
}
fn reserve(tx: &Connection, plan: &ContestPreflight, max: i64) -> Result<()> {
    let key = format!("plan:{}", plan.plan_id);
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    squad_store::accounting::reserve(
        tx,
        &key,
        None,
        &day,
        squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
            .map_err(|_| invalid())?,
        max,
    )?;
    tx.execute(
        "UPDATE reservations SET contest_id=?,plan_id=? WHERE reservation_id=?",
        params![plan.contest_id.to_string(), plan.plan_id.to_string(), key],
    )?;
    Ok(())
}
fn preflight_result(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    plan: &ContestPreflight,
    status: PreflightStatus,
    confirmation: Option<Confirmation>,
    failure: Option<ErrorPayload>,
) -> Result<()> {
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: plan.contest_id,
                plan_id: plan.plan_id,
                status,
                confirmation,
                error: failure,
            }),
        ),
    )?;
    squad_store::contests::associate(tx, plan.contest_id, ContestKind::Grill, seq)
}
pub fn resolve(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan: PlanId,
    cancel: bool,
    max: i64,
) -> Result<RequestResult> {
    let mut state = squad_store::grill::state(tx, contest)?;
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=? AND plan_id=?",
        params![contest.to_string(), plan.to_string()],
        |r| r.get(0),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    if state.state != GrillStatus::Preflight {
        return Err(invalid());
    }
    let mut failure = None;
    let status = if cancel {
        PreflightStatus::Cancelled
    } else if g.accepted_at >= plan.expires_at {
        PreflightStatus::Expired
    } else {
        match reserve(tx, &plan, max) {
            Ok(()) => PreflightStatus::Confirmed,
            Err(StoreError::Domain(ErrorCode::DAILY_MAX_REACHED, _)) => {
                let day = chrono::Local::now().format("%Y-%m-%d").to_string();
                let required =
                    squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
                        .map_err(|_| invalid())?;
                failure = Some(super::daily_cap_error(tx, &day, required, max));
                PreflightStatus::Rejected
            }
            Err(e) => return Err(e),
        }
    };
    state.state = if status == PreflightStatus::Confirmed {
        GrillStatus::WaitingForParticipants
    } else if status == PreflightStatus::Rejected {
        GrillStatus::Failed
    } else {
        GrillStatus::Stopped
    };
    state.state_version += 1;
    save(tx, g, id, &state)?;
    preflight_result(
        tx,
        g,
        id,
        &plan,
        status.clone(),
        (status == PreflightStatus::Confirmed).then_some(Confirmation::Explicit),
        failure.clone(),
    )?;
    if let Some(error) = failure {
        return Ok(RequestResult::Rejected {
            request_id: id,
            error,
        });
    }
    Ok(g.accepted(id))
}
pub fn provenance(db: &Connection, id: ContestId) -> Result<serde_json::Value> {
    let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='grill.provenance'",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn current_step(db: &Connection, id: ContestId) -> Result<Option<StepId>> {
    let raw:Option<String>=db.query_row("SELECT step_id FROM contest_steps WHERE contest_id=? AND committed=0 ORDER BY ordinal DESC LIMIT 1",[id.to_string()],|r|r.get(0)).optional()?;
    raw.map(|s| s.parse().map_err(|_| invalid())).transpose()
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: ContestTransitionSubmission,
) -> Result<()> {
    let state = squad_store::grill::state(tx, s.contest_id)?;
    if state.state_version != s.expected_state_version {
        return Err(invalid());
    }
    let ContestTransition::Grill(signal) = s.transition else {
        return Err(invalid());
    };
    if signal != GrillTransition::ParticipantsReady {
        return Err(invalid());
    }
    for nick in squad_store::grill::setup(tx, s.contest_id)?
        .setup
        .participants()
    {
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL AND json_type(scheduling_meta,'$.openModeDormant') IS NULL)",[nick.as_str()],|r|r.get(0))?;
        if pending {
            return Err(invalid());
        }
    }
    let setup = squad_store::grill::setup(tx, s.contest_id)?.setup;
    floor(tx, g, id, EnvelopePayload::GrillSetup(setup.clone()))?;
    floor(
        tx,
        g,
        id,
        EnvelopePayload::GrillPlan(GrillPlan {
            contest_id: setup.contest_id,
            round: 0,
            version: 1,
            leader: setup.leader,
            text: setup.initial_plan,
        }),
    )?;
    let next =
        crate::orchestrator::grill::transition(&state, &signal, 0, 0).map_err(|_| invalid())?;
    save(tx, g, id, &next)
}
pub fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    step: ContestStep,
    works: Vec<Work>,
) -> Result<RequestResult> {
    let state = squad_store::grill::state(tx, step.contest_id)?;
    if current_step(tx, state.contest_id)?.is_some()
        || works.len() != 1
        || step.assignments.len() != 1
        || step.script_version.as_str() != "grill-v1"
        || !step.inputs.is_empty()
        || !step.assignments[0].payload_refs.is_empty()
        || step.move_name != ContestMove::Turns
    {
        return Err(invalid());
    }
    let ordinal: u32 = tx.query_row(
        "SELECT COALESCE(MAX(ordinal),0)+1 FROM contest_steps WHERE contest_id=?",
        [state.contest_id.to_string()],
        |r| r.get(0),
    )?;
    let trigger:String=tx.query_row("SELECT e.id FROM contest_events c JOIN events e ON e.seq=c.event_seq WHERE c.contest_id=? ORDER BY e.seq DESC LIMIT 1",[state.contest_id.to_string()],|r|r.get(0))?;
    if step.ordinal != ordinal || works[0].trigger_event_id.to_string() != trigger {
        return Err(invalid());
    }
    let frozen = provenance(tx, state.contest_id)?;
    let prompt = &frozen["prompts"][prompt_key(&state)];
    let expected = work(
        &state,
        step.step_id,
        works[0].trigger_event_id,
        frozen["packVersion"].as_str().ok_or_else(invalid)?,
        prompt["hash"].as_str().ok_or_else(invalid)?,
    )?;
    let mut w = works[0].clone();
    if w.kind != expected.kind
        || w.target != expected.target
        || w.context_id != expected.context_id
        || w.round != expected.round
        || w.contest_id != expected.contest_id
        || w.step_id != expected.step_id
        || w.prompt_ref != expected.prompt_ref
        || w.max_output_tokens != expected.max_output_tokens
        || w.attempt != 1
    {
        return Err(invalid());
    }
    if step.assignments[0].target != w.target
        || step.assignments[0].work_id != Some(w.work_id)
        || step.assignments[0].context_id != w.context_id
    {
        return Err(invalid());
    }
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestStep(step.clone()),
        ),
    )?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Grill, seq)?;
    for projection in squad_store::grill::projection(tx, state.contest_id, w.context_id.clone())? {
        squad_store::history::project(tx, &projection)?;
    }
    let boundary: i64 = tx.query_row(
        "SELECT MAX(pos) FROM history WHERE context_id=?",
        [&w.context_id.0],
        |r| r.get(0),
    )?;
    w.history_through = HistoryPosition::new(boundary).map_err(|_| invalid())?;
    let binding: ExecutorBinding =
        serde_json::from_value(frozen["bindings"][w.target.to_string()].clone())?;
    squad_store::work::insert(tx, &w, &binding, None)?;
    squad_store::contest_steps::attach_work(tx, step.step_id, &w)?;
    squad_store::contest_steps::private_map(
        tx,
        state.contest_id,
        &format!("grill.work:{}", w.work_id),
        &serde_json::json!({"rendered":prompt["rendered"],"input":squad_store::grill::work_input(tx,&state)?}),
    )?;
    super::contests::queued(tx, g, &w)?;
    Ok(g.accepted(id))
}

/// Accept only the canonical generation and the output derived from its frozen input.
pub fn canonical(db: &Connection, w: &Work, payload: &EnvelopePayload) -> Result<EnvelopePayload> {
    let EnvelopePayload::ContestArtifact(a) = payload else {
        return Err(invalid());
    };
    let state = squad_store::grill::state(db, a.contest_id)?;
    if state.state.is_terminal()
        || current_step(db, a.contest_id)? != w.step_id
        || a.artifact_kind.as_str() != w.kind.grill_composition().ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    if a.data["finishReason"] != serde_json::json!(FinishReason::Stop) {
        return Err(invalid());
    }
    let generation: GenerationIdentity = serde_json::from_value(a.data["generation"].clone())?;
    if generation.work_id != w.work_id
        || generation.message_id != w.message_id
        || generation.trigger_event_id != w.trigger_event_id
        || generation.attempt != w.attempt
    {
        return Err(super::authorization::forbidden());
    }
    let input = squad_store::grill::work_input(db, &state)?;
    input
        .parse_output(&w.kind, a.data["raw"].as_str().ok_or_else(invalid)?)
        .map_err(|_| invalid())
}
pub fn barrier(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    commit: BarrierCommit,
) -> Result<()> {
    let state = squad_store::grill::state(tx, commit.contest_id)?;
    if state.state_version != commit.expected_state_version
        || state.state.is_terminal()
        || current_step(tx, state.contest_id)? != Some(commit.step_id)
        || !commit.projections.is_empty()
        || !commit.next_work.is_empty()
    {
        return Err(invalid());
    }
    let raw: String = tx.query_row(
        "SELECT work_id FROM contest_assignments WHERE step_id=?",
        [commit.step_id.to_string()],
        |r| r.get(0),
    )?;
    let wid: WorkId = raw.parse().map_err(|_| invalid())?;
    let record = squad_store::work::record(tx, wid)?.ok_or_else(invalid)?;
    let w = squad_store::work::template(tx, wid)?.ok_or_else(invalid)?.0;
    let signal = if record.status == WorkStatus::Completed {
        let seq: i64 = tx.query_row(
            "SELECT completed_event_seq FROM work WHERE work_id=?",
            [wid.to_string()],
            |r| r.get(0),
        )?;
        let event = squad_store::events::at(tx, seq)?;
        match event.content {
            EnvelopePayload::GrillQuestion(_) => GrillTransition::QuestionsCommitted {
                question_event_id: event.base.id,
            },
            EnvelopePayload::GrillAnswer(_) => GrillTransition::AnswerCommitted {
                answer_event_id: event.base.id,
            },
            EnvelopePayload::GrillPass(_) => GrillTransition::PassCommitted {
                pass_event_id: event.base.id,
            },
            EnvelopePayload::GrillPlan(_) => GrillTransition::PlanCommitted {
                plan_event_id: event.base.id,
            },
            _ => return Err(invalid()),
        }
    } else if matches!(record.status, WorkStatus::Failed | WorkStatus::Cancelled) {
        let failure = record
            .error
            .unwrap_or_else(|| error(ErrorCode::PROVIDER_ERROR, "Grill assignment exhausted"));
        squad_store::contest_steps::settle(tx, commit.step_id, &w.target, Some(&failure))?;
        if state.state == GrillStatus::Questioning {
            let pass = floor(
                tx,
                g,
                id,
                EnvelopePayload::GrillPass(GrillPass {
                    contest_id: state.contest_id,
                    round: state.round,
                    turn: state.turn,
                    griller: state.griller_order[state.turn as usize].clone(),
                    reason: if failure.code == ErrorCode::PROVIDER_TIMEOUT {
                        GrillPassReason::Timeout
                    } else {
                        GrillPassReason::Error
                    },
                }),
            )?;
            GrillTransition::PassCommitted {
                pass_event_id: pass,
            }
        } else {
            GrillTransition::LeaderFailed
        }
    } else {
        return Err(invalid());
    };
    let mut questions = 0;
    let mut passes = 0;
    for e in squad_store::grill::events(tx, state.contest_id)? {
        match e.content {
            EnvelopePayload::GrillQuestion(q) if q.round == state.round => questions += 1,
            EnvelopePayload::GrillPass(p)
                if p.round == state.round && p.reason == GrillPassReason::Pass =>
            {
                passes += 1
            }
            _ => {}
        }
    }
    let mut next = crate::orchestrator::grill::transition(&state, &signal, questions, passes)
        .map_err(|_| invalid())?;
    next.questions_asked = state.questions_asked
        + u32::from(matches!(signal, GrillTransition::QuestionsCommitted { .. }));
    next.passed = state.passed
        + u32::from(
            matches!(signal, GrillTransition::PassCommitted { .. })
                && record.status == WorkStatus::Completed,
        );
    let report = EnvelopePayload::ContestReport(ContestReport {
        contest_id: state.contest_id,
        step_id: commit.step_id,
        artifact_event_ids: vec![],
        skipped_work_ids: if record.status == WorkStatus::Completed {
            vec![]
        } else {
            vec![wid]
        },
        data: serde_json::json!({"grillBarrier":true}),
    });
    let seq = g.push(tx, envelope(id, Actor::System, vec![Recipient::Me], report))?;
    squad_store::contest_steps::barrier(tx, commit.step_id, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Grill, seq)?;
    if next.state.is_terminal() {
        let events = squad_store::grill::events(tx, state.contest_id)?;
        let plan = events
            .iter()
            .rev()
            .find_map(|e| {
                if let EnvelopePayload::GrillPlan(p) = &e.content {
                    Some((e.base.id, p.version))
                } else {
                    None
                }
            })
            .ok_or_else(invalid)?;
        floor(
            tx,
            g,
            id,
            EnvelopePayload::GrillComplete(GrillComplete {
                contest_id: state.contest_id,
                final_plan_event_id: plan.0,
                final_plan_version: plan.1,
                reason: next.terminal_reason.clone(),
                appendix: crate::orchestrator::grill::appendix(&events).map_err(|_| invalid())?,
            }),
        )?;
    }
    save(tx, g, id, &next)
}
#[allow(clippy::too_many_arguments)]
pub fn pick(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    nicks: Vec<MemberNick>,
    bindings: &BTreeMap<String, ExecutorBinding>,
    max: i64,
    threshold: f64,
    config: &SquadConfig,
) -> Result<RequestResult> {
    let raw: String = tx.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND active=1",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let state: ContestSelectionState = serde_json::from_str(&raw)?;
    if state.state != ContestSelectionStatus::AwaitingPick
        || nicks.len() != 1
        || !state.answers.iter().any(|a| a.nick == nicks[0])
    {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_PICK_INVALID,
            "Pick one eligible leader",
        ));
    }
    let answer = state
        .answers
        .iter()
        .find(|a| a.nick == nicks[0])
        .expect("checked");
    let picked = ContestPicked {
        contest_id: contest,
        kind: ContestKind::Grill,
        nicks: nicks.clone(),
        source_message_ids: vec![answer.message_id],
    };
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPicked(picked.clone()),
        ),
    )?;
    squad_store::contests::picks::accept(tx, &picked, seq)?;
    squad_store::contests::associate(tx, contest, ContestKind::Grill, seq)?;
    let raw: String = tx.query_row(
        "SELECT pending_request_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let rounds = serde_json::from_str::<serde_json::Value>(&raw)?["rounds"]
        .as_u64()
        .ok_or_else(invalid)? as u32;
    let seq:i64=tx.query_row("SELECT seq FROM events WHERE kind='user.message' AND json_extract(payload,'$.messageId')=?",[state.question_message_id.to_string()],|r|r.get(0))?;
    let event = squad_store::events::at(tx, seq)?;
    let EnvelopePayload::UserMessage(question) = event.content else {
        return Err(invalid());
    };
    let setup = GrillSetup {
        contest_id: contest,
        leader: nicks[0].clone(),
        grillers: state
            .answers
            .iter()
            .filter(|a| a.nick != nicks[0])
            .map(|a| a.nick.clone())
            .collect(),
        source_message_id: answer.message_id,
        initial_plan: answer.text.clone(),
        focus: question.body,
        total_rounds: rounds,
    };
    prepare(
        tx,
        g,
        id,
        question.input,
        setup,
        bindings,
        max,
        threshold,
        false,
        config,
        true,
    )
}
