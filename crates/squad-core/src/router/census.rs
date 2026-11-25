//! Transactional census admission and lifecycle gates, shared with the driver.
use super::{Router, envelope};
use crate::{orchestrator::census as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::*;
use squad_store::{
    Result, StoreError,
    commit::{CommitGroup, CommitOutcome},
};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid census transition",
    )
}
pub fn push(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    payload: EnvelopePayload,
) -> Result<Cursor> {
    let contest = match &payload {
        EnvelopePayload::CensusSetup(s) => s.contest_id,
        EnvelopePayload::CensusPlan(s) => s.contest_id,
        EnvelopePayload::CensusReport(s) => s.contest_id,
        EnvelopePayload::ContestStep(s) => s.contest_id,
        EnvelopePayload::ContestReport(s) => s.contest_id,
        EnvelopePayload::ContestPreflight(s) => s.contest_id,
        EnvelopePayload::ContestPreflightResult(s) => s.contest_id,
        _ => return Err(invalid()),
    };
    let seq = group.push(
        tx,
        envelope(id, Actor::System, vec![Recipient::Me], payload),
    )?;
    squad_store::contests::associate(tx, contest, ContestKind::Census, seq)?;
    Ok(seq)
}
pub fn save(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &CensusState,
    expected: Option<u64>,
) -> Result<()> {
    let seq = group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::CensusState(state.clone()),
        ),
    )?;
    squad_store::census::save(tx, state, expected, seq)?;
    if state.status.is_terminal() || state.status == CensusStatus::WaitingForPlan {
        squad_store::history::release(tx, state.contest_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'",[state.contest_id.to_string()])?;
    }
    if state.status.is_terminal() {
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[state.contest_id.to_string()])?;
        let mut q=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
        let rows = q
            .query_map([state.contest_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (reservation, day, amount) in rows {
            tx.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![amount, day],
            )?;
            tx.execute(
                "DELETE FROM reservations WHERE reservation_id=?",
                [reservation],
            )?;
        }
    }
    Ok(())
}
pub fn source(db: &Connection, source: &parser::PlanSource) -> Result<(String, Vec<EventId>)> {
    let event = match source {
        parser::PlanSource::Pasted { text } => return Ok((text.clone(), vec![])),
        parser::PlanSource::Quoted { event_id } => {
            squad_store::events::by_id(db, *event_id)?.ok_or_else(invalid)?
        }
        parser::PlanSource::LatestReply { nick } => {
            let seq:Option<i64>=db.query_row("SELECT MAX(seq) FROM events WHERE kind='message.final' AND sender=? AND NOT EXISTS(SELECT 1 FROM superseded WHERE superseded_event_id=events.id)",[nick.as_str()],|r|r.get(0))?;
            squad_store::events::at(
                db,
                seq.ok_or(StoreError::Domain(
                    ErrorCode::CENSUS_SOURCE_MISSING,
                    "Plan source missing",
                ))?,
            )?
        }
    };
    let text = match &event.content {
        EnvelopePayload::MessageFinal(m) => m.content.clone(),
        EnvelopePayload::UserMessage(m) => m.body.clone(),
        EnvelopePayload::CensusPlan(p) => p.text.to_string(),
        _ => script::grill_plan_text(&serde_json::to_value(&event.content)?).ok_or_else(invalid)?,
    };
    Ok((text, vec![event.base.id]))
}
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    parsed: ParsedInput,
    preconfirmed: bool,
    bindings: &BTreeMap<String, ExecutorBinding>,
    config: &SquadConfig,
    max: i64,
    threshold: f64,
) -> Result<RequestResult> {
    let ParsedInput::Census {
        participants,
        plan,
        passes,
        input: conversation,
    } = parsed
    else {
        return Err(invalid());
    };
    let conflict: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get(0),
    )?;
    if conflict {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let (text, sources) = source(tx, &plan)?;
    if text.trim().is_empty() {
        return Err(invalid());
    }
    let contest = ContestId::new();
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let mut frozen_sources = serde_json::Map::new();
    for name in ["census.hunt", "census.merge", "census.rate", "census.final"] {
        let (system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        frozen_sources.insert(
            name.into(),
            serde_json::json!({"system":system,"user":user}),
        );
    }
    let prompt_refs = ["census.hunt", "census.merge", "census.rate", "census.final"]
        .into_iter()
        .map(|name| {
            let (sys, user) = pack
                .composition_source(name, false)
                .map_err(|_| invalid())?;
            Ok((
                name.into(),
                PromptRef {
                    pack_version: Text::new(pack.version()).map_err(|_| invalid())?,
                    template_id: Text::new(name).map_err(|_| invalid())?,
                    template_hash: Text::new(squad_store::commit::digest(&(sys, user))?)
                        .map_err(|_| invalid())?,
                },
            ))
        })
        .collect::<Result<_>>()?;
    let mut models = bindings.clone();
    if let Some(j) = models.get_mut("judge") {
        j.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
    }
    let setup = CensusSetup {
        contest_id: contest,
        plan: CensusPlan {
            contest_id: contest,
            version: 1,
            text: Text::new(text).map_err(|_| invalid())?,
            source_event_ids: sources,
        },
        participants,
        passes,
        room_words: config.prompts.limits.room_words,
        plan_words: config.prompts.limits.plan_words,
        models,
        prompt_refs,
        redaction_tokens: config
            .agents
            .iter()
            .flat_map(|a| [a.nick.to_string(), a.model.clone(), a.persona.clone()])
            .chain(std::iter::once(config.judge.model.clone()))
            .collect(),
    };
    setup.validate().map_err(|_| invalid())?;
    // A conservative frozen input bound includes all possible generated material
    // for the most expensive finalization, with output and safety reservations.
    let input_bound =
        setup.plan.text.as_str().len() as u64 + 32_768 + setup.participants.len() as u64 * 16_384;
    let mut normal = 0.0_f64;
    let mut reserved = 0.0_f64;
    for key in setup
        .participants
        .iter()
        .map(|n| n.as_str())
        .chain(std::iter::once("judge"))
    {
        let binding = &setup.models[key];
        let (length, pricing): (i64, String) = tx.query_row(
            "SELECT context_length,pricing_json FROM catalog WHERE slug=?",
            [binding.model.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let output = if key == "judge" {
            constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
        } else {
            2048
        };
        let reasoning = match binding.effort {
            ReasoningEffort::Low => constants::REASONING_LOW_TOKENS,
            ReasoningEffort::Medium => constants::REASONING_MEDIUM_TOKENS,
            ReasoningEffort::High => constants::REASONING_HIGH_TOKENS,
        };
        if input_bound + u64::from(output + reasoning) + 1024 > length as u64 {
            return Err(StoreError::Domain(
                ErrorCode::CONTEXT_TOO_LARGE,
                "Census input exceeds model context",
            ));
        }
        let pricing: serde_json::Value = serde_json::from_str(&pricing)?;
        let price = |key: &str| {
            pricing[key]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| pricing[key].as_f64())
                .unwrap_or(0.0)
        };
        let cost = input_bound as f64 * price("prompt")
            + f64::from(output + reasoning) * price("completion");
        normal += 2.0 * f64::from(passes) * cost;
        reserved += if key == "judge" { 6.0 } else { 4.0 } * f64::from(passes) * cost;
    }
    let mut state = CensusState {
        contest_id: contest,
        state_version: 1,
        status: CensusStatus::Preflight,
        pass: 1,
        plan_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        unfinished_work_ids: vec![],
        terminal_reason: None,
    };
    save(tx, group, id, &state, None)?;
    let plan_seq = push(
        tx,
        group,
        id,
        EnvelopePayload::CensusPlan(setup.plan.clone()),
    )?;
    tx.execute(
        "INSERT INTO census_runs VALUES (?,?,?)",
        params![
            contest.to_string(),
            serde_json::to_string(&setup)?,
            serde_json::to_string(&setup.plan)?
        ],
    )?;
    tx.execute(
        "INSERT INTO census_passes(contest_id,pass,plan_event_seq) VALUES (?,1,?)",
        params![contest.to_string(), plan_seq.get()],
    )?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "census.prompts",
        &serde_json::json!({"compositions":frozen_sources,"personas":config.agents.iter().map(|a|(a.nick.to_string(),a.persona.clone())).collect::<BTreeMap<_,_>>(),"userName":config.user.name,"roster":setup.participants}),
    )?;
    let map = script::lenses(&setup.participants);
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "census.lenses",
        &serde_json::json!({"assignments":map,"tieBreakSeed":CellId::new().to_string()}),
    )?;
    push(tx, group, id, EnvelopePayload::CensusSetup(setup.clone()))?;
    super::variables::apply(tx, group, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Census,
        plan_id: PlanId::new(),
        expires_at: group.accepted_at
            + chrono::Duration::milliseconds(squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: normal,
            reserved_usd: reserved,
            remaining_usd: max.saturating_sub(daily.spent_micros + daily.reserved_micros) as f64
                / 1e6,
            model_calls: setup.normal_calls(),
        },
        participants: setup.participants.clone(),
        confirmation_required: normal > threshold,
        warning: None,
        settings: serde_json::json!({"passes":passes,"reservedCalls":setup.reserved_calls(),"planVersion":1,"inputBound":input_bound}),
    };
    CensusPreflight {
        base: plan.clone(),
        passes,
        plan: setup.plan.clone(),
    }
    .validate()
    .map_err(|_| invalid())?;
    let seq = push(
        tx,
        group,
        id,
        EnvelopePayload::ContestPreflight(plan.clone()),
    )?;
    tx.execute("UPDATE contests SET plan_id=?,plan_json=?,expires_at=?,pending_request_json=? WHERE contest_id=?",params![plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),serde_json::json!({"input":input,"preconfirmed":preconfirmed,"max":max,"threshold":threshold}).to_string(),contest.to_string()])?;
    if max <= daily.spent_micros + daily.reserved_micros
        || reserved * 1e6 > max.saturating_sub(daily.spent_micros + daily.reserved_micros) as f64
    {
        state.status = CensusStatus::Failed;
        state.terminal_reason = Some(Text::new("daily_max_reached").expect("reason"));
        state.state_version += 1;
        let mut error = super::daily_cap_error(
            tx,
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
            (reserved * 1e6).ceil() as i64,
            max,
        );
        error.contest_id = Some(contest);
        push(
            tx,
            group,
            id,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: contest,
                plan_id: plan.plan_id,
                status: PreflightStatus::Rejected,
                confirmation: None,
                error: Some(error.clone()),
            }),
        )?;
        save(tx, group, id, &state, Some(1))?;
        return Ok(RequestResult::Rejected {
            request_id: id,
            error,
        });
    }
    if !plan.confirmation_required || preconfirmed {
        resolve(tx, group, id, contest, plan.plan_id, false)
    } else {
        Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(plan),
            cursor: seq,
        })
    }
}
pub fn resolve(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan_id: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let mut state = squad_store::census::state(tx, contest)?;
    let (raw, pending): (String, String) = tx.query_row(
        "SELECT plan_json,pending_request_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    let pending: serde_json::Value = serde_json::from_str(&pending)?;
    if state.status != CensusStatus::Preflight || plan.plan_id != plan_id {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let version = state.state_version;
    state.state_version += 1;
    if cancel || expired {
        state.status = CensusStatus::Stopped;
        state.terminal_reason =
            Some(Text::new(if expired { "expired" } else { "cancelled" }).expect("reason"));
    } else {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let amount = squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
            .map_err(|_| invalid())?;
        squad_store::accounting::reserve(
            tx,
            &format!("plan:{plan_id}"),
            None,
            &day,
            amount,
            pending["max"].as_i64().ok_or_else(invalid)?,
        )?;
        tx.execute(
            "UPDATE reservations SET plan_id=?,contest_id=? WHERE reservation_id=?",
            params![
                plan_id.to_string(),
                contest.to_string(),
                format!("plan:{plan_id}")
            ],
        )?;
        state.status = CensusStatus::WaitingForParticipants;
    }
    push(
        tx,
        group,
        id,
        EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
            contest_id: contest,
            plan_id,
            status: if expired {
                PreflightStatus::Expired
            } else if cancel {
                PreflightStatus::Cancelled
            } else {
                PreflightStatus::Confirmed
            },
            confirmation: (!cancel && !expired).then_some(if pending["preconfirmed"] == true {
                Confirmation::Preconfirmed
            } else if plan.confirmation_required {
                Confirmation::Explicit
            } else {
                Confirmation::Automatic
            }),
            error: None,
        }),
    )?;
    save(tx, group, id, &state, Some(version))?;
    Ok(group.accepted(id))
}
pub fn continue_plan(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    version: u64,
    plan_event_id: Option<EventId>,
    replacement: Option<Text>,
) -> Result<RequestResult> {
    let mut state = squad_store::census::state(tx, contest)?;
    if state.state_version != version
        || state.status != CensusStatus::WaitingForPlan
        || plan_event_id.is_some() && replacement.is_some()
    {
        return Err(invalid());
    }
    let mut plan = squad_store::census::plan(tx, contest)?;
    if let Some(text) = replacement {
        plan.version += 1;
        plan.text = text;
        plan.source_event_ids = vec![];
    }
    if let Some(event) = plan_event_id {
        let (text, refs) = source(tx, &parser::PlanSource::Quoted { event_id: event })?;
        plan.version += 1;
        plan.text = Text::new(text).map_err(|_| invalid())?;
        plan.source_event_ids = refs;
    }
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let mut preflight: ContestPreflight = serde_json::from_str(&raw)?;
    let original = squad_store::census::setup(tx, contest)?
        .plan
        .text
        .as_str()
        .len();
    let revised = plan.text.as_str().len() > original;
    let seq = push(tx, group, id, EnvelopePayload::CensusPlan(plan.clone()))?;
    state.pass += 1;
    state.plan_version = plan.version;
    state.status = CensusStatus::WaitingForParticipants;
    state.step_id = None;
    state.unfinished_work_ids.clear();
    state.state_version += 1;
    tx.execute(
        "UPDATE census_runs SET current_plan_json=? WHERE contest_id=?",
        params![serde_json::to_string(&plan)?, contest.to_string()],
    )?;
    tx.execute(
        "INSERT INTO census_passes(contest_id,pass,plan_event_seq) VALUES (?,?,?)",
        params![contest.to_string(), state.pass, seq.get()],
    )?;
    if revised {
        let setup = squad_store::census::setup(tx, contest)?;
        let bound =
            plan.text.as_str().len() as u64 + 32_768 + setup.participants.len() as u64 * 16_384;
        for binding in setup.models.values() {
            let length: i64 = tx.query_row(
                "SELECT context_length FROM catalog WHERE slug=?",
                [binding.model.as_str()],
                |r| r.get(0),
            )?;
            if bound + u64::from(constants::REASONING_HIGH_TOKENS) + 8192 > length as u64 {
                return Err(StoreError::Domain(
                    ErrorCode::CONTEXT_TOO_LARGE,
                    "Replacement census plan exceeds model context",
                ));
            }
        }
        let old_bound = preflight.settings["inputBound"]
            .as_f64()
            .ok_or_else(invalid)?;
        let remaining = u32::from(setup.passes - state.pass + 1);
        let ratio = bound as f64 / old_bound;
        preflight.plan_id = PlanId::new();
        preflight.expires_at = group.accepted_at
            + chrono::Duration::milliseconds(squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64);
        preflight.estimate.estimated_usd *= ratio * f64::from(remaining) / f64::from(setup.passes);
        preflight.estimate.reserved_usd *= ratio * f64::from(remaining) / f64::from(setup.passes);
        preflight.estimate.model_calls = remaining * (2 * setup.participants.len() as u32 + 2);
        preflight.settings = serde_json::json!({"passes":remaining,"reservedCalls":remaining*(4*setup.participants.len() as u32+6),"inputBound":bound,"planVersion":plan.version});
        let pending: String = tx.query_row(
            "SELECT pending_request_json FROM contests WHERE contest_id=?",
            [contest.to_string()],
            |r| r.get(0),
        )?;
        let pending: serde_json::Value = serde_json::from_str(&pending)?;
        preflight.confirmation_required =
            preflight.estimate.estimated_usd > pending["threshold"].as_f64().unwrap_or(0.0);
        let rows = {
            let mut q=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
            q.query_map([contest.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (reservation, day, amount) in rows {
            tx.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![amount, day],
            )?;
            tx.execute(
                "DELETE FROM reservations WHERE reservation_id=?",
                [reservation],
            )?;
        }
        tx.execute(
            "UPDATE contests SET plan_id=?,plan_json=?,expires_at=? WHERE contest_id=?",
            params![
                preflight.plan_id.to_string(),
                serde_json::to_string(&preflight)?,
                preflight.expires_at.to_rfc3339(),
                contest.to_string()
            ],
        )?;
        state.status = CensusStatus::Preflight;
        save(tx, group, id, &state, Some(version))?;
        let seq = push(
            tx,
            group,
            id,
            EnvelopePayload::ContestPreflight(preflight.clone()),
        )?;
        if !preflight.confirmation_required || pending["preconfirmed"] == true {
            return resolve(tx, group, id, contest, preflight.plan_id, false);
        }
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(preflight),
            cursor: seq,
        });
    }
    save(tx, group, id, &state, Some(version))?;
    Ok(group.accepted(id))
}

pub(crate) fn drive_move(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    mut state: CensusState,
) -> Result<RequestResult> {
    let current = squad_store::census::state(tx, state.contest_id)?;
    if current != state {
        return Err(invalid());
    }
    let setup = squad_store::census::setup(tx, state.contest_id)?;
    if state.status == CensusStatus::Preflight {
        let raw: String = tx.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [state.contest_id.to_string()],
            |r| r.get(0),
        )?;
        let plan: ContestPreflight = serde_json::from_str(&raw)?;
        if group.accepted_at >= plan.expires_at {
            return resolve(tx, group, id, state.contest_id, plan.plan_id, true);
        }
        return Ok(group.accepted(id));
    }
    if state.status == CensusStatus::WaitingForParticipants {
        for member in &setup.participants {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[member.as_str()],|r|r.get(0))?;
            if pending {
                return Ok(group.accepted(id));
            }
        }
        let version = state.state_version;
        state.status = CensusStatus::Hunting;
        state.state_version += 1;
        save(tx, group, id, &state, Some(version))?;
        return Ok(group.accepted(id));
    }
    if state.status == CensusStatus::Reporting {
        let version = state.state_version;
        state.status = CensusStatus::Complete;
        state.terminal_reason = Some(Text::new("Pass limit reached").expect("reason"));
        state.state_version += 1;
        save(tx, group, id, &state, Some(version))?;
        return Ok(group.accepted(id));
    }
    if state.step_id.is_none() {
        allocate(tx, group, id, &state, &setup)?;
        return Ok(group.accepted(id));
    }
    let step = state.step_id.ok_or_else(invalid)?;
    let mut artifacts = vec![];
    let mut missing = vec![];
    for work_id in &state.unfinished_work_ids {
        let mut record = squad_store::work::record(tx, *work_id)?.ok_or_else(invalid)?;
        let work = squad_store::work::template(tx, *work_id)?
            .ok_or_else(invalid)?
            .0;
        if record.status == WorkStatus::Queued
            && record.attempt >= if work.kind.census_judge() { 3 } else { 2 }
        {
            tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1 WHERE work_id=?",[work_id.to_string()])?;
            record.status = WorkStatus::Failed;
        }
        if matches!(record.status, WorkStatus::Queued | WorkStatus::Running) {
            return Ok(group.accepted(id));
        }
        if record.status == WorkStatus::Completed {
            let seq: i64 = tx.query_row(
                "SELECT completed_event_seq FROM work WHERE work_id=?",
                [work_id.to_string()],
                |r| r.get(0),
            )?;
            let event = squad_store::events::at(tx, seq)?;
            let EnvelopePayload::ContestArtifact(artifact) = event.content else {
                return Err(invalid());
            };
            artifacts.push((work, artifact, event.base.id, seq));
        } else {
            if work.kind.census_judge() {
                state.status = CensusStatus::Failed;
                state.terminal_reason =
                    Some(Text::new("judge_attempts_exhausted").expect("reason"));
                break;
            }
            missing.push(CensusMissing {
                member: match &work.target {
                    Target::Member(n) => n.clone(),
                    _ => return Err(invalid()),
                },
                step: Text::new(work.kind.census_composition().ok_or_else(invalid)?)
                    .map_err(|_| invalid())?,
            });
            let failure = super::error(
                ErrorCode::MODE_INVALID_OUTPUT,
                "Member skipped after exhausted attempts",
            );
            squad_store::contest_steps::settle(tx, step, &work.target, Some(&failure))?;
        }
    }
    let version = state.state_version;
    if state.status == CensusStatus::Failed {
        state.state_version += 1;
        save(tx, group, id, &state, Some(version))?;
        return Ok(group.accepted(id));
    }
    let artifact_ids = artifacts.iter().map(|a| a.2).collect::<Vec<_>>();
    // One barrier freezes all accepted artifacts and attributed failures before
    // any next-step input is constructed. It is also the anonymous source event.
    let seq = push(
        tx,
        group,
        id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: state.contest_id,
            step_id: step,
            artifact_event_ids: artifact_ids.clone(),
            skipped_work_ids: state
                .unfinished_work_ids
                .iter()
                .filter(|w| !artifacts.iter().any(|a| a.0.work_id == **w))
                .copied()
                .collect(),
            data: serde_json::json!({"pass":state.pass,"step":state.status,"missing":missing}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    let plan = squad_store::census::plan(tx, state.contest_id)?;
    match state.status {
        CensusStatus::Hunting => {
            if artifacts.is_empty() {
                state.status = CensusStatus::Failed;
                state.terminal_reason = Some(Text::new("no_hunts").expect("reason"));
            } else {
                let mut finds = vec![];
                let mut provenance = serde_json::Map::new();
                let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='census.lenses'",[state.contest_id.to_string()],|r|r.get(0))?;
                let maps: serde_json::Value = serde_json::from_str(&raw)?;
                for (work, artifact, _, _) in &artifacts {
                    let Target::Member(member) = &work.target else {
                        return Err(invalid());
                    };
                    let output: CensusHuntOutput = serde_json::from_value(artifact.data.clone())?;
                    let lens = script::lens(
                        maps["assignments"][member.as_str()]
                            .as_u64()
                            .ok_or_else(invalid)? as usize,
                        state.pass,
                    );
                    let hunt = CensusHunt {
                        contest_id: state.contest_id,
                        pass: state.pass,
                        plan_version: state.plan_version,
                        work_id: work.work_id,
                        member: member.clone(),
                        lens: Text::new(lens).expect("lens"),
                        output: output.clone(),
                    };
                    let hunt_seq = group.push(
                        tx,
                        envelope(
                            id,
                            Actor::Member(member.clone()),
                            vec![Recipient::Me],
                            EnvelopePayload::CensusHunt(hunt),
                        ),
                    )?;
                    squad_store::contests::associate(
                        tx,
                        state.contest_id,
                        ContestKind::Census,
                        hunt_seq,
                    )?;
                    for find in output.gaps {
                        let handle = CellId::new().to_string();
                        provenance.insert(
                            handle.clone(),
                            serde_json::json!({"member":member,"huntSeq":hunt_seq.get()}),
                        );
                        finds.push(serde_json::json!({"id":handle,"omission":script::blind_text(find.omission.as_str(),&setup),"consequence":script::blind_text(find.consequence.as_str(),&setup),"lens":lens}));
                    }
                }
                finds.sort_by_key(|f| f["id"].as_str().unwrap_or_default().to_owned());
                squad_store::contest_steps::private_map(
                    tx,
                    state.contest_id,
                    &format!("census.finds.{}", state.pass),
                    &serde_json::json!({"sources":provenance,"finds":finds,"missing":missing}),
                )?;
                state.status = CensusStatus::Merging;
            }
        }
        CensusStatus::Merging => {
            let (work, artifact, _, _) = artifacts.first().ok_or_else(invalid)?;
            let mut output: CensusMergeOutput = serde_json::from_value(artifact.data.clone())?;
            let raw: String = tx.query_row(
                "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",
                params![
                    state.contest_id.to_string(),
                    format!("census.finds.{}", state.pass)
                ],
                |r| r.get(0),
            )?;
            let sources: serde_json::Value = serde_json::from_str(&raw)?;
            for gap in &mut output.ledger {
                gap.id = Text::new(CellId::new().to_string()).map_err(|_| invalid())?;
                let mut lenses = gap
                    .source_find_ids
                    .iter()
                    .map(|source| {
                        let lens = sources["finds"]
                            .as_array()
                            .ok_or_else(invalid)?
                            .iter()
                            .find(|f| f["id"].as_str() == Some(source.as_str()))
                            .and_then(|f| f["lens"].as_str())
                            .ok_or_else(invalid)?;
                        Text::new(lens).map_err(|_| invalid())
                    })
                    .collect::<Result<Vec<_>>>()?;
                lenses.sort_by(|a, b| a.as_str().cmp(b.as_str()));
                lenses.dedup();
                gap.lenses = lenses;

                for source in &gap.source_find_ids {
                    let finder = &sources["sources"][source.as_str()];
                    tx.execute(
                        "INSERT OR IGNORE INTO census_finders VALUES (?,?,?,?,?)",
                        params![
                            state.contest_id.to_string(),
                            state.pass,
                            gap.id.as_str(),
                            finder["member"].as_str().ok_or_else(invalid)?,
                            finder["huntSeq"].as_i64().ok_or_else(invalid)?
                        ],
                    )?;
                }
            }
            let seq = group.push(
                tx,
                envelope(
                    id,
                    Actor::Judge,
                    vec![Recipient::Me],
                    EnvelopePayload::CensusLedger(CensusLedger {
                        stage: CensusLedgerStage::Merged,
                        final_output: None,
                        contest_id: state.contest_id,
                        pass: state.pass,
                        plan_version: state.plan_version,
                        work_id: work.work_id,
                        output,
                    }),
                ),
            )?;
            squad_store::contests::associate(tx, state.contest_id, ContestKind::Census, seq)?;
            tx.execute(
                "UPDATE census_passes SET merged_ledger_event_seq=? WHERE contest_id=? AND pass=?",
                params![seq.get(), state.contest_id.to_string(), state.pass],
            )?;
            state.status = CensusStatus::Rating;
        }
        CensusStatus::Rating => {
            let mut additions = vec![];
            let mut origins = serde_json::Map::new();
            for (work, artifact, _, _) in &artifacts {
                let Target::Member(member) = &work.target else {
                    return Err(invalid());
                };
                let output: CensusRatingOutput = serde_json::from_value(artifact.data.clone())?;
                for a in &output.additions {
                    let handle = CellId::new().to_string();
                    additions.push(serde_json::json!({"id":handle,"omission":script::blind_text(a.omission.as_str(),&setup),"consequence":script::blind_text(a.consequence.as_str(),&setup),"lens":script::blind_text(a.lens.as_str(),&setup)}));
                    origins.insert(handle,serde_json::json!({"member":member,"workId":work.work_id,"localId":a.local_id}));
                }
                let rating = CensusRating {
                    contest_id: state.contest_id,
                    pass: state.pass,
                    plan_version: state.plan_version,
                    work_id: work.work_id,
                    member: member.clone(),
                    output: serde_json::from_value(artifact.data.clone())?,
                };
                let seq = group.push(
                    tx,
                    envelope(
                        id,
                        Actor::Member(member.clone()),
                        vec![Recipient::Me],
                        EnvelopePayload::CensusRating(rating),
                    ),
                )?;
                squad_store::contests::associate(tx, state.contest_id, ContestKind::Census, seq)?;
            }
            additions.sort_by_key(|a| a["id"].as_str().unwrap_or_default().to_owned());
            squad_store::contest_steps::private_map(
                tx,
                state.contest_id,
                &format!("census.additions.{}", state.pass),
                &serde_json::json!({"origins":origins,"additions":additions}),
            )?;
            if artifacts.is_empty() {
                state.status = CensusStatus::Failed;
                state.terminal_reason = Some(Text::new("no_ratings").expect("reason"));
            } else {
                state.status = CensusStatus::Finalizing;
            }
        }
        CensusStatus::Finalizing => {
            let final_output: CensusFinalOutput =
                serde_json::from_value(artifacts.first().ok_or_else(invalid)?.1.data.clone())?;
            let data = final_data(tx, &state, &setup, &plan)?;
            let mut outcomes: Vec<CensusGapOutcome> =
                serde_json::from_value(data["outcomes"].clone())?;
            let order = final_output
                .confirmed
                .iter()
                .enumerate()
                .map(|(i, g)| (g.gap_id.to_string(), i))
                .collect::<BTreeMap<_, _>>();
            outcomes.sort_by_key(|g| order.get(g.gap.id.as_str()).copied().unwrap_or(usize::MAX));
            for gap in &mut outcomes {
                gap.priority = final_output
                    .confirmed
                    .iter()
                    .find(|r| r.gap_id == gap.gap.id)
                    .map(|r| r.priority.clone());
            }
            let prior = squad_store::census::events(tx, state.contest_id)?
                .into_iter()
                .filter_map(|e| {
                    if let EnvelopePayload::CensusReport(r) = e.content {
                        Some(r)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let missing: Vec<CensusMissing> = serde_json::from_value(data["missing"].clone())?;
            let final_seq = group.push(
                tx,
                envelope(
                    id,
                    Actor::Judge,
                    vec![Recipient::Me],
                    EnvelopePayload::CensusLedger(CensusLedger {
                        contest_id: state.contest_id,
                        pass: state.pass,
                        plan_version: state.plan_version,
                        work_id: artifacts.first().ok_or_else(invalid)?.0.work_id,
                        stage: CensusLedgerStage::Finalized,
                        final_output: Some(final_output.clone()),
                        output: ledger(tx, &state)?,
                    }),
                ),
            )?;
            squad_store::contests::associate(tx, state.contest_id, ContestKind::Census, final_seq)?;
            let report = script::report(
                state.contest_id,
                state.pass,
                state.plan_version,
                outcomes,
                final_output.additions,
                missing,
                &prior,
            );
            let seq = push(tx, group, id, EnvelopePayload::CensusReport(report.clone()))?;
            tx.execute("UPDATE census_passes SET final_ledger_event_seq=?,report_event_seq=?,tally_json=? WHERE contest_id=? AND pass=?",params![final_seq.get(),seq.get(),serde_json::to_string(&report)?,state.contest_id.to_string(),state.pass])?;
            state.status = if state.pass < setup.passes {
                CensusStatus::WaitingForPlan
            } else {
                CensusStatus::Reporting
            };
        }
        _ => return Err(invalid()),
    }
    state.artifact_event_ids.extend(artifact_ids);
    state.unfinished_work_ids.clear();
    state.step_id = None;
    state.state_version += 1;
    save(tx, group, id, &state, Some(version))?;
    Ok(group.accepted(id))
}
fn ledger(db: &Connection, state: &CensusState) -> Result<CensusMergeOutput> {
    squad_store::census::events(db, state.contest_id)?
        .into_iter()
        .find_map(|e| match e.content {
            EnvelopePayload::CensusLedger(l) if l.pass == state.pass => Some(l.output),
            _ => None,
        })
        .ok_or_else(invalid)
}
fn final_data(
    db: &Connection,
    state: &CensusState,
    setup: &CensusSetup,
    plan: &CensusPlan,
) -> Result<serde_json::Value> {
    let ledger = ledger(db, state)?;
    let events = squad_store::census::events(db, state.contest_id)?;
    let ratings = events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::CensusRating(r) if r.pass == state.pass => Some(r),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut finders: BTreeMap<String, Vec<MemberNick>> = BTreeMap::new();
    let mut q = db.prepare(
        "SELECT gap_id,nick FROM census_finders WHERE contest_id=? AND pass=? ORDER BY gap_id,nick",
    )?;
    for row in q.query_map(params![state.contest_id.to_string(), state.pass], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (gap, nick) = row?;
        finders
            .entry(gap)
            .or_default()
            .push(nick.parse().map_err(|_| invalid())?);
    }
    let outcomes = script::outcomes(
        &ledger,
        &ratings.iter().map(|r| r.output.clone()).collect::<Vec<_>>(),
        setup.participants.len(),
        &finders,
    );
    let raw: String = db.query_row(
        "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",
        params![
            state.contest_id.to_string(),
            format!("census.additions.{}", state.pass)
        ],
        |r| r.get(0),
    )?;
    let addition_map: serde_json::Value = serde_json::from_str(&raw)?;
    let additions = addition_map["additions"]
        .as_array()
        .ok_or_else(invalid)?
        .clone();
    let addition_ids = additions
        .iter()
        .map(|a| a["id"].clone())
        .collect::<Vec<_>>();
    let mut missing = vec![];
    for member in &setup.participants {
        for (step,accepted) in [("census.hunt",events.iter().any(|e|matches!(&e.content,EnvelopePayload::CensusHunt(h) if h.pass==state.pass && &h.member==member))),("census.rate",ratings.iter().any(|r|&r.member==member))] {if !accepted{missing.push(CensusMissing{member:member.clone(),step:Text::new(step).expect("step")});}}
    }
    Ok(
        serde_json::json!({"plan":script::blind_text(plan.text.as_str(),setup),"planVersion":plan.version,"outcomes":outcomes,"additions":additions,"additionIds":addition_ids,"missing":missing}),
    )
}
fn allocate(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &CensusState,
    setup: &CensusSetup,
) -> Result<()> {
    let plan = squad_store::census::plan(tx, state.contest_id)?;
    let kind = match state.status {
        CensusStatus::Hunting => WorkKind::CensusHunt,
        CensusStatus::Merging => WorkKind::CensusMerge,
        CensusStatus::Rating => WorkKind::CensusRate,
        CensusStatus::Finalizing => WorkKind::CensusFinal,
        _ => return Err(invalid()),
    };
    let step_id = StepId::new();
    let judge = kind.census_judge();
    let targets = if judge {
        vec![Target::Judge]
    } else {
        setup
            .participants
            .iter()
            .cloned()
            .map(Target::Member)
            .collect()
    };
    let mut work = vec![];
    let mut values = vec![];
    let mut assignments = vec![];
    for target in targets {
        let case = judge.then(CaseId::new);
        let context = if let Some(case) = case {
            ContextId(format!("judge:{case}"))
        } else {
            ContextId(format!(
                "contest:{}:{}:{}",
                state.contest_id, step_id, target
            ))
        };
        let mut data = match kind {
            WorkKind::CensusHunt => {
                let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='census.lenses'",[state.contest_id.to_string()],|r|r.get(0))?;
                let map: serde_json::Value = serde_json::from_str(&raw)?;
                let lens = script::lens(
                    map["assignments"][target.to_string()]
                        .as_u64()
                        .ok_or_else(invalid)? as usize,
                    state.pass,
                );
                serde_json::json!({"plan":script::blind_text(plan.text.as_str(),setup),"planVersion":plan.version,"lens":lens})
            }
            WorkKind::CensusMerge => {
                let raw: String = tx.query_row(
                    "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",
                    params![
                        state.contest_id.to_string(),
                        format!("census.finds.{}", state.pass)
                    ],
                    |r| r.get(0),
                )?;
                let map: serde_json::Value = serde_json::from_str(&raw)?;
                serde_json::json!({"plan":script::blind_text(plan.text.as_str(),setup),"planVersion":plan.version,"finds":map["finds"],"sourceIds":map["finds"].as_array().ok_or_else(invalid)?.iter().map(|f|f["id"].clone()).collect::<Vec<_>>()})
            }
            WorkKind::CensusRate => script::rating_data(&plan, &ledger(tx, state)?, setup),
            WorkKind::CensusFinal => final_data(tx, state, setup, &plan)?,
            _ => return Err(invalid()),
        };
        if kind == WorkKind::CensusFinal {
            if let Some(outcomes) = data["outcomes"].as_array_mut() {
                for outcome in outcomes {
                    let outcome = outcome.as_object_mut().expect("typed outcome");
                    outcome.remove("finders");
                    outcome.remove("finderCount");
                }
            }
            data.as_object_mut().expect("object").remove("missing");
        }
        script::redact_prose(&mut data, setup);
        let name = kind.census_composition().ok_or_else(invalid)?;
        let mut binding = setup.models[&target.to_string()].clone();
        if judge {
            binding.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
        }
        let item = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: kind.clone(),
            target: target.clone(),
            context_id: context.clone(),
            trigger_event_id: id,
            history_through: HistoryPosition::new(1).expect("positive"),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: case,
            prompt_ref: Some(setup.prompt_refs[name].clone()),
            max_output_tokens: Some(if judge {
                constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            } else {
                2048
            }),
            contest_id: Some(state.contest_id),
            step_id: Some(step_id),
        };
        let mut input = crate::prompts::inputs_census::values(
            state,
            &kind,
            &data,
            if judge {
                setup.plan_words
            } else {
                setup.room_words
            },
        )?;
        let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='census.prompts'",[state.contest_id.to_string()],|r|r.get(0))?;
        let frozen: serde_json::Value = serde_json::from_str(&frozen)?;
        let mut render_values = input.clone();
        render_values.insert("nick".into(), target.to_string());
        render_values.insert("model_slug".into(), binding.model.to_string());
        render_values.insert("judge_model_slug".into(), binding.model.to_string());
        render_values.insert("member_nicks_json".into(), frozen["roster"].to_string());
        render_values.insert("user_name_json".into(), frozen["userName"].to_string());
        render_values.insert(
            "persona_json".into(),
            frozen["personas"][target.to_string()]
                .as_str()
                .map(|v| serde_json::json!(v).to_string())
                .unwrap_or_else(|| "\"\"".into()),
        );
        let source = &frozen["compositions"][name];
        let system = crate::prompts::inputs_census::render_source(
            source["system"].as_str().ok_or_else(invalid)?,
            &render_values,
        )?;
        let user = crate::prompts::inputs_census::render_source(
            source["user"].as_str().ok_or_else(invalid)?,
            &render_values,
        )?;
        input.insert(
            "census_rendered_prompt".into(),
            serde_json::json!({"system":system,"user":user,"provider_user_name":"squad"})
                .to_string(),
        );
        assignments.push(ContestAssignment {
            target,
            context_id: context,
            payload_refs: vec![],
            work_id: Some(item.work_id),
        });
        values.push((input, binding));
        work.push(item);
    }
    let step = ContestStep {
        contest_id: state.contest_id,
        step_id,
        ordinal: (u32::from(state.pass) - 1) * 4
            + match kind {
                WorkKind::CensusHunt => 1,
                WorkKind::CensusMerge => 2,
                WorkKind::CensusRate => 3,
                _ => 4,
            },
        script_version: Text::new("census-v1").expect("script"),
        move_name: if judge {
            ContestMove::Judge
        } else {
            ContestMove::Fanout
        },
        inputs: vec![],
        assignments,
    };
    // The source identity is a shared step event, never an individual contributor.
    let mut event = envelope(
        id,
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestStep(step.clone()),
    );
    event.base.id = id;
    let seq = group.push(tx, event)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Census, seq)?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (mut item, (input, binding)) in work.iter().cloned().zip(values) {
        if item.case_id.is_some() {
            let data: serde_json::Value = serde_json::from_str(&input["mode_data_json"])?;
            let seq = group.push(
                tx,
                envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::CensusCaseMaterial(CensusCaseMaterial {
                        contest_id: state.contest_id,
                        pass: state.pass,
                        plan_version: state.plan_version,
                        step: Text::new(kind.census_composition().ok_or_else(invalid)?)
                            .map_err(|_| invalid())?,
                        data,
                    }),
                ),
            )?;
            squad_store::contests::associate(tx, state.contest_id, ContestKind::Census, seq)?;
        }
        if let Some(case) = item.case_id {
            tx.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![case.to_string(),item.context_id.0,state.contest_id.to_string(),binding.model.as_str(),input["mode_data_json"],seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: item.context_id.clone(),
            origin_event_id: id,
            projection_key: Text::new("census.step").expect("key"),
            role: HistoryRole::User,
            name: Some("squad".into()),
            content: input["mode_data_json"].clone(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        let position = squad_store::history::project(tx, &projection)?.ok_or_else(invalid)?;
        item.history_through = position;
        squad_store::work::insert(tx, &item, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step_id, &item)?;
        tx.execute(
            "INSERT INTO census_work_inputs VALUES (?,?)",
            params![item.work_id.to_string(), serde_json::to_string(&input)?],
        )?;
        group.push(
            tx,
            envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::WorkState(WorkStateEvent {
                    work_id: item.work_id,
                    trigger_event_id: id,
                    target: item.target,
                    context_id: item.context_id,
                    attempt: 0,
                    status: WorkStatus::Queued,
                    error: None,
                    debate_id: None,
                    case_id: item.case_id,
                    contest_id: item.contest_id,
                }),
            ),
        )?;
    }
    let mut next = state.clone();
    next.step_id = Some(step_id);
    next.unfinished_work_ids = work.iter().map(|w| w.work_id).collect();
    next.state_version += 1;
    save(tx, group, id, &next, Some(state.state_version))
}

impl Router {
    /// The common ContestRouter transition hook delegates to the census script;
    /// durable artifacts, barriers and pending slots remain the authority.
    pub(crate) fn census_transition(
        &mut self,
        id: EventId,
        submission: ContestTransitionSubmission,
    ) -> Result<CommitOutcome> {
        let digest = squad_store::commit::digest(&submission)?;
        self.store
            .commit(id, "contest-engine".into(), digest, move |tx, group| {
                let mut state = squad_store::census::state(tx, submission.contest_id)?;
                if submission.kind != ContestKind::Census
                    || state.state_version != submission.expected_state_version
                    || state.status.is_terminal()
                {
                    return Err(invalid());
                }
                let ContestTransition::Census(signal) = submission.transition else {
                    return Err(invalid());
                };
                let matches = match signal {
                    CensusTransition::ParticipantsReady => {
                        state.status == CensusStatus::WaitingForParticipants
                    }
                    CensusTransition::HuntBarrier => {
                        state.status == CensusStatus::Hunting && state.step_id.is_some()
                    }
                    CensusTransition::MergeAccepted => {
                        state.status == CensusStatus::Merging && state.step_id.is_some()
                    }
                    CensusTransition::RatingBarrier => {
                        state.status == CensusStatus::Rating && state.step_id.is_some()
                    }
                    CensusTransition::FinalAccepted => {
                        state.status == CensusStatus::Finalizing && state.step_id.is_some()
                    }
                    CensusTransition::ReportCommitted => state.status == CensusStatus::Reporting,
                    CensusTransition::Stop | CensusTransition::Fail => true,
                    CensusTransition::Continue => state.status == CensusStatus::WaitingForPlan,
                };
                if !matches {
                    return Err(invalid());
                }
                if matches!(signal, CensusTransition::Stop | CensusTransition::Fail) {
                    let version = state.state_version;
                    state.state_version += 1;
                    state.status = if signal == CensusTransition::Stop {
                        CensusStatus::Stopped
                    } else {
                        CensusStatus::Failed
                    };
                    state.terminal_reason = Some(
                        Text::new(if signal == CensusTransition::Stop {
                            "stopped"
                        } else {
                            "failed"
                        })
                        .expect("reason"),
                    );
                    save(tx, group, id, &state, Some(version))?;
                    return Ok(group.accepted(id));
                }
                let result = drive_move(tx, group, id, state.clone())?;
                if squad_store::census::state(tx, state.contest_id)?.state_version
                    == state.state_version
                {
                    return Err(invalid());
                }
                Ok(result)
            })
    }
}
