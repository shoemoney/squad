//! Forecast admission and state changes run on the shared serialized writer.
use super::{bounds, envelope};
use crate::{orchestrator::forecast as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::{forecast::*, *};
pub use squad_store::forecast::invalid;
use squad_store::{Result, StoreError, commit::CommitGroup};
use std::collections::BTreeMap;
const NAMES: [&str; 4] = [
    "forecast.frame",
    "forecast.estimate",
    "forecast.revise",
    "forecast.cruxes",
];
pub fn load_state(tx: &Connection, contest: ContestId) -> Result<ForecastState> {
    let raw: String = tx.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='forecast'",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
fn push(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    payload: EnvelopePayload,
) -> Result<Cursor> {
    let seq = g.push(
        tx,
        envelope(id, Actor::System, vec![Recipient::Me], payload),
    )?;
    squad_store::contests::associate(tx, contest, ContestKind::Forecast, seq)?;
    Ok(seq)
}
fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ForecastState,
    expected: Option<u64>,
) -> Result<()> {
    s.validate().map_err(|_| invalid())?;
    if let Some(v) = expected {
        if load_state(tx, s.contest_id)?.state_version != v {
            return Err(invalid());
        }
    }
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ForecastState(s.clone()),
        ),
    )?;
    tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'forecast',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get()])?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Forecast, seq)?;
    if s.state.is_terminal() {
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[s.contest_id.to_string()])?;
        squad_store::history::release(tx, s.contest_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'",[s.contest_id.to_string()])?;
        let mut q=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
        let rows = q
            .query_map([s.contest_id.to_string()], |r| {
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
/// Exact D79 bounds; each member is priced individually against its frozen binding.
pub fn steps(setup: &ForecastSetup, pack: &PromptPack) -> Result<Vec<bounds::Step>> {
    use bounds::*;
    let m = setup.participants.len() as u64;
    let room = limited(2048, setup.room_words);
    let bg = serde_json::to_string(&setup.background)?.len() as u64;
    let data = [
        serde_json::to_string(&setup.question)?.len() as u64 + bg,
        artifact(4096) + bg,
        artifact(4096) + bg + room + (m - 1) * project(room),
        artifact(4096) + bg + 2 * m * project(room) + 1024,
    ];
    let kinds = [
        WorkKind::ForecastFrame,
        WorkKind::ForecastEstimate,
        WorkKind::ForecastRevise,
        WorkKind::ForecastCruxes,
    ];
    let mut out = vec![];
    for (i, kind) in kinds.into_iter().enumerate() {
        let targets = if kind.new_mode_judge() {
            vec!["judge".to_string()]
        } else {
            setup.participants.iter().map(ToString::to_string).collect()
        };
        for target in targets {
            out.push(Step {
                kind: kind.clone(),
                target,
                alternatives: vec![],
                template: template(pack, NAMES[i])?,
                schema: schema_bytes(output_schema(&kind).ok_or_else(invalid)?),
                data: data[i],
                visible: kind.new_mode_visible_tokens().ok_or_else(invalid)?,
                calls: 1,
                attempts: if kind.new_mode_judge() { 3 } else { 2 },
            })
        }
    }
    Ok(out)
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::Forecast {
        participants,
        question,
        background,
        input: conversation,
    } = parsed
    else {
        return Err(invalid());
    };
    let config = a.config.ok_or_else(invalid)?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let contest = ContestId::new();
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let mut models = a.bindings.clone();
    models.get_mut("judge").ok_or_else(invalid)?.effort =
        constants::NEW_MODE_JUDGE_REASONING_EFFORT;
    let mut compositions = serde_json::Map::new();
    let mut refs = BTreeMap::new();
    for name in NAMES {
        let (system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        refs.insert(
            name.into(),
            PromptRef {
                pack_version: Text::new(pack.version()).map_err(|_| invalid())?,
                template_id: Text::new(name).expect("name"),
                template_hash: Text::new(squad_store::commit::digest(&(
                    system.clone(),
                    user.clone(),
                ))?)
                .map_err(|_| invalid())?,
            },
        );
        compositions.insert(
            name.into(),
            serde_json::json!({"system":system,"user":user}),
        );
    }
    let setup = ForecastSetup {
        contest_id: contest,
        question: Text::new(question.clone()).map_err(|_| invalid())?,
        background: background.clone().unwrap_or_default(),
        participants: participants.clone(),
        models,
        prompt_refs: refs,
        room_words: config.prompts.limits.room_words,
        plan_words: config.prompts.limits.plan_words,
        redaction_tokens: config
            .agents
            .iter()
            .flat_map(|a| [a.nick.to_string(), a.model.clone(), a.persona.clone()])
            .collect(),
    };
    setup.validate().map_err(|_| invalid())?;
    let priced = bounds::price(tx, &setup.models, &steps(&setup, &pack)?)?;
    let s = ForecastState {
        contest_id: contest,
        state_version: 1,
        state: ForecastStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &s, None)?;
    tx.execute(
        "INSERT INTO forecast_runs(contest_id,setup_json,case_id) VALUES(?,?,?)",
        params![
            contest.to_string(),
            serde_json::to_string(&setup)?,
            CaseId::new().to_string()
        ],
    )?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "forecast.prompts",
        &serde_json::json!({"compositions":compositions,"repair":pack.template("mode.repair").map_err(|_|invalid())?,"personas":config.agents.iter().map(|a|(a.nick.to_string(),a.persona.clone())).collect::<BTreeMap<_,_>>(),"userName":config.user.name}),
    )?;
    push(
        tx,
        g,
        id,
        contest,
        EnvelopePayload::ForecastSetup(ContestSetup {
            contest_id: contest,
            kind: ContestKind::Forecast,
            script_version: Text::new("forecast-v1").expect("script"),
            participants: participants.clone(),
            source_event_ids: vec![],
            settings: serde_json::to_value(&setup)?,
        }),
    )?;
    super::variables::apply(tx, g, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    settings["reservedAttempts"] = serde_json::json!(4 * participants.len() + 6);
    settings["memberCalls"] = serde_json::json!(2 * participants.len());
    settings["judgeCalls"] = serde_json::json!(2);
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Forecast,
        plan_id: PlanId::new(),
        expires_at: g.accepted_at
            + chrono::Duration::milliseconds(constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: priced.estimated_usd,
            reserved_usd: priced.reserved_usd(),
            remaining_usd: a
                .daily_max
                .saturating_sub(daily.spent_micros + daily.reserved_micros)
                as f64
                / 1e6,
            model_calls: 2 * participants.len() as u32 + 2,
        },
        participants: participants.clone(),
        confirmation_required: priced.estimated_usd > a.confirmation_threshold,
        warning: None,
        settings,
    };
    let seq = push(
        tx,
        g,
        id,
        contest,
        EnvelopePayload::ContestPreflight(plan.clone()),
    )?;
    tx.execute("UPDATE contests SET plan_id=?,plan_json=?,expires_at=?,pending_request_json=? WHERE contest_id=?",params![plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),serde_json::json!({"input":input,"preconfirmed":a.preconfirmed,"max":a.daily_max}).to_string(),contest.to_string()])?;
    if priced.reserved_micros
        > a.daily_max
            .saturating_sub(daily.spent_micros + daily.reserved_micros)
    {
        let mut failed = s;
        failed.state = ForecastStatus::Failed;
        failed.state_version += 1;
        failed.terminal_reason = Some(Text::new("daily_max_reached").expect("reason"));
        save(tx, g, id, &failed, Some(1))?;
        let mut error = super::daily_cap_error(tx, &day, priced.reserved_micros, a.daily_max);
        error.contest_id = Some(contest);
        push(
            tx,
            g,
            id,
            contest,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: contest,
                plan_id: plan.plan_id,
                status: PreflightStatus::Rejected,
                confirmation: None,
                error: Some(error.clone()),
            }),
        )?;
        return Ok(RequestResult::Rejected {
            request_id: id,
            error,
        });
    }
    if !plan.confirmation_required || a.preconfirmed {
        resolve(tx, g, id, contest, plan.plan_id, false)
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
    let mut state = load_state(tx, contest)?;
    let (raw, pending): (String, String) = tx.query_row(
        "SELECT plan_json,pending_request_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    let pending: serde_json::Value = serde_json::from_str(&pending)?;
    if state.state != ForecastStatus::Preflight || plan.plan_id != plan_id {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let version = state.state_version;
    state.state_version += 1;
    if cancel || expired {
        state.state = ForecastStatus::Stopped;
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
        state.state = ForecastStatus::WaitingForParticipants;
    }
    push(
        tx,
        group,
        id,
        contest,
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
fn record<T: serde::de::DeserializeOwned>(tx: &Connection, id: ContestId, key: &str) -> Result<T> {
    squad_store::forecast::get(tx, id, key)?.ok_or_else(invalid)
}
fn frame(tx: &Connection, id: ContestId) -> Result<ForecastFrame> {
    record(tx, id, "frame")
}
fn firsts(
    tx: &Connection,
    setup: &ForecastSetup,
) -> Result<BTreeMap<MemberNick, ForecastEstimate>> {
    let mut out = BTreeMap::new();
    for n in &setup.participants {
        if let Some(v) = squad_store::forecast::get(tx, setup.contest_id, &format!("first:{n}"))? {
            out.insert(n.clone(), v);
        }
    }
    Ok(out)
}
fn results(tx: &Connection, setup: &ForecastSetup) -> Result<Vec<ForecastMemberResult>> {
    let first = firsts(tx, setup)?;
    let labels: BTreeMap<MemberNick, String> = record(tx, setup.contest_id, "labels")?;
    let aggregate: ForecastAggregate = record(tx, setup.contest_id, "firstAggregate")?;
    first
        .into_iter()
        .map(|(n, e)| {
            let revision: Option<ForecastRevision> =
                squad_store::forecast::get(tx, setup.contest_id, &format!("revision:{n}"))?;
            let pct = revision
                .as_ref()
                .map(|r| r.probability_pct)
                .unwrap_or(e.probability_pct);
            let uncited = revision.as_ref().is_some_and(|r| {
                script::uncited_move(e.probability_pct, pct, aggregate.l, &r.cited_labels)
            });
            Ok(ForecastMemberResult {
                label: labels[&n].clone(),
                member: n,
                first: e,
                not_revised: revision.is_none(),
                revision,
                final_probability_pct: pct,
                uncited_move: uncited,
            })
        })
        .collect()
}
fn terminal(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut state: ForecastState,
    status: ForecastStatus,
    reason: &str,
) -> Result<RequestResult> {
    let version = state.state_version;
    state.state = status;
    state.state_version += 1;
    state.terminal_reason = Some(Text::new(reason).map_err(|_| invalid())?);
    save(tx, g, id, &state, Some(version))?;
    Ok(g.accepted(id))
}
fn artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ForecastState,
    kind: &str,
    body: serde_json::Value,
) -> Result<Cursor> {
    let a = ContestArtifact {
        contest_id: s.contest_id,
        step_id: s.step_id.ok_or_else(invalid)?,
        artifact_kind: Text::new(kind).map_err(|_| invalid())?,
        work_id: None,
        source_refs: vec![],
        data: body.clone(),
        kind: Some(ContestKind::Forecast),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
    };
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestArtifact(a.clone()),
    )?;
    squad_store::contest_steps::artifact(tx, &a, seq)?;
    Ok(seq)
}
pub fn drive_move(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut state: ForecastState,
) -> Result<RequestResult> {
    if state != load_state(tx, state.contest_id)? {
        return Err(invalid());
    }
    let setup = squad_store::forecast::setup(tx, state.contest_id)?;
    if state.state == ForecastStatus::Preflight {
        let raw: String = tx.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [state.contest_id.to_string()],
            |r| r.get(0),
        )?;
        let plan: ContestPreflight = serde_json::from_str(&raw)?;
        if g.accepted_at >= plan.expires_at {
            return resolve(tx, g, id, state.contest_id, plan.plan_id, false);
        }
        return Ok(g.accepted(id));
    }
    if state.state == ForecastStatus::WaitingForParticipants {
        for n in &setup.participants {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get::<_,bool>(0))?{return Ok(g.accepted(id))}
        }
        let v = state.state_version;
        state.state = ForecastStatus::Framing;
        state.state_version += 1;
        save(tx, g, id, &state, Some(v))?;
        return Ok(g.accepted(id));
    }
    if state.state == ForecastStatus::Aggregating {
        let members = results(tx, &setup)?;
        let aggregate = script::aggregate(
            &members
                .iter()
                .map(|m| m.final_probability_pct)
                .collect::<Vec<_>>(),
        )
        .map_err(|_| invalid())?;
        artifact(
            tx,
            g,
            id,
            &state,
            "forecast.aggregate",
            serde_json::to_value(&aggregate)?,
        )?;
        squad_store::forecast::put(tx, state.contest_id, "finalAggregate", &aggregate, None)?;
        tx.execute(
            "UPDATE forecast_runs SET final_aggregate_json=? WHERE contest_id=?",
            params![
                serde_json::to_string(&aggregate)?,
                state.contest_id.to_string()
            ],
        )?;
        let v = state.state_version;
        state.state = ForecastStatus::Explaining;
        state.step_id = None;
        state.state_version += 1;
        save(tx, g, id, &state, Some(v))?;
        return Ok(g.accepted(id));
    }
    if state.state == ForecastStatus::Reporting {
        let accepted = firsts(tx, &setup)?;
        let report = ForecastReport {
            contest_id: state.contest_id,
            frame: frame(tx, state.contest_id)?,
            first_aggregate: record(tx, state.contest_id, "firstAggregate")?,
            final_aggregate: record(tx, state.contest_id, "finalAggregate")?,
            members: results(tx, &setup)?,
            cruxes: squad_store::forecast::get(tx, state.contest_id, "cruxes")?,
            skipped: setup
                .participants
                .iter()
                .filter(|n| !accepted.contains_key(*n))
                .cloned()
                .collect(),
        };
        artifact(
            tx,
            g,
            id,
            &state,
            "forecast.report",
            serde_json::to_value(&report)?,
        )?;
        return terminal(tx, g, id, state, ForecastStatus::Complete, "complete");
    }
    if state.step_id.is_none() {
        allocate(tx, g, id, &state, &setup)?;
        return Ok(g.accepted(id));
    }
    let step = state.step_id.ok_or_else(invalid)?;
    let mut q =
        tx.prepare("SELECT work_id FROM contest_assignments WHERE step_id=? ORDER BY target")?;
    let work_ids = q
        .query_map([step.to_string()], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut accepted = vec![];
    let mut skipped = vec![];
    for raw in work_ids {
        let wid: WorkId = raw.parse().map_err(|_| invalid())?;
        let mut r = squad_store::work::record(tx, wid)?.ok_or_else(invalid)?;
        let w = squad_store::work::template(tx, wid)?.ok_or_else(invalid)?.0;
        if r.status == WorkStatus::Queued
            && r.attempt >= if w.kind.new_mode_judge() { 3 } else { 2 }
        {
            tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1 WHERE work_id=?",[wid.to_string()])?;
            r.status = WorkStatus::Failed;
        }
        if matches!(r.status, WorkStatus::Queued | WorkStatus::Running) {
            return Ok(g.accepted(id));
        }
        if r.status == WorkStatus::Completed {
            let seq: i64 = tx.query_row(
                "SELECT completed_event_seq FROM work WHERE work_id=?",
                [wid.to_string()],
                |r| r.get(0),
            )?;
            let e = squad_store::events::at(tx, seq)?;
            let EnvelopePayload::ContestArtifact(a) = e.content else {
                return Err(invalid());
            };
            accepted.push((w, a, e.base.id));
        } else {
            let failure = super::error(
                ErrorCode::MODE_INVALID_OUTPUT,
                "Skipped after exhausted attempts",
            );
            squad_store::contest_steps::settle_if_pending(tx, step, &w.target, Some(&failure))?;
            skipped.push(w);
        }
    }
    let seq = push(
        tx,
        g,
        id,
        state.contest_id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: state.contest_id,
            step_id: step,
            artifact_event_ids: accepted.iter().map(|a| a.2).collect(),
            skipped_work_ids: skipped.iter().map(|w| w.work_id).collect(),
            data: serde_json::json!({"step":state.state}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    if skipped.iter().any(|w| w.kind.new_mode_judge()) {
        return terminal(tx, g, id, state, ForecastStatus::Failed, "judge_failed");
    }
    let prior = state.state;
    let v = state.state_version;
    match prior {
        ForecastStatus::Framing => {
            let f: ForecastFrame = serde_json::from_value(
                accepted
                    .first()
                    .ok_or_else(invalid)?
                    .1
                    .body
                    .clone()
                    .ok_or_else(invalid)?,
            )?;
            squad_store::forecast::put(tx, state.contest_id, "frame", &f, Some(seq))?;
            tx.execute(
                "UPDATE forecast_runs SET frame_json=? WHERE contest_id=? AND frame_json IS NULL",
                params![serde_json::to_string(&f)?, state.contest_id.to_string()],
            )?;
            if !f.forecastable {
                return terminal(tx, g, id, state, ForecastStatus::Failed, "not_forecastable");
            }
            state.state = ForecastStatus::Forecasting;
        }
        ForecastStatus::Forecasting => {
            for (w, a, _) in &accepted {
                let Target::Member(n) = &w.target else {
                    return Err(invalid());
                };
                let e: ForecastEstimate =
                    serde_json::from_value(a.body.clone().ok_or_else(invalid)?)?;
                squad_store::forecast::put(
                    tx,
                    state.contest_id,
                    &format!("first:{n}"),
                    &e,
                    Some(seq),
                )?;
            }
            let first = firsts(tx, &setup)?;
            if first.len() < 3 {
                return terminal(
                    tx,
                    g,
                    id,
                    state,
                    ForecastStatus::Failed,
                    "insufficient_forecasts",
                );
            }
            let aggregate = script::aggregate(
                &first
                    .values()
                    .map(|e| e.probability_pct)
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| invalid())?;
            squad_store::forecast::put(
                tx,
                state.contest_id,
                "firstAggregate",
                &aggregate,
                Some(seq),
            )?;
            tx.execute(
                "UPDATE forecast_runs SET first_aggregate_json=? WHERE contest_id=?",
                params![
                    serde_json::to_string(&aggregate)?,
                    state.contest_id.to_string()
                ],
            )?;
            let mut shuffled = first
                .keys()
                .map(|n| (CellId::new().to_string(), n.clone()))
                .collect::<Vec<_>>();
            shuffled.sort();
            let labels = shuffled
                .into_iter()
                .enumerate()
                .map(|(i, (_, n))| (n, format!("F{}", i + 1)))
                .collect::<BTreeMap<_, _>>();
            squad_store::forecast::put(tx, state.contest_id, "labels", &labels, Some(seq))?;
            squad_store::contest_steps::private_map(
                tx,
                state.contest_id,
                "forecast.labels",
                &serde_json::to_value(&labels)?,
            )?;
            for n in first.keys() {
                let b = script::bundle(n, &first, &labels, &setup);
                artifact(
                    tx,
                    g,
                    id,
                    &state,
                    "forecast.bundle",
                    serde_json::to_value(&b)?,
                )?;
                squad_store::forecast::put(
                    tx,
                    state.contest_id,
                    &format!("bundle:{n}"),
                    &b,
                    Some(seq),
                )?;
            }
            state.state = ForecastStatus::Revising;
        }
        ForecastStatus::Revising => {
            for (w, a, _) in &accepted {
                let Target::Member(n) = &w.target else {
                    return Err(invalid());
                };
                let r: ForecastRevision =
                    serde_json::from_value(a.body.clone().ok_or_else(invalid)?)?;
                squad_store::forecast::put(
                    tx,
                    state.contest_id,
                    &format!("revision:{n}"),
                    &r,
                    Some(seq),
                )?;
            }
            state.state = ForecastStatus::Aggregating;
        }
        ForecastStatus::Explaining => {
            let c: ForecastCruxes = serde_json::from_value(
                accepted
                    .first()
                    .ok_or_else(invalid)?
                    .1
                    .body
                    .clone()
                    .ok_or_else(invalid)?,
            )?;
            squad_store::forecast::put(tx, state.contest_id, "cruxes", &c, Some(seq))?;
            state.state = ForecastStatus::Reporting;
        }
        _ => return Err(invalid()),
    }
    state
        .artifact_event_ids
        .extend(accepted.iter().map(|a| a.2));
    if !matches!(
        state.state,
        ForecastStatus::Aggregating | ForecastStatus::Reporting
    ) {
        state.step_id = None
    }
    state.state_version += 1;
    save(tx, g, id, &state, Some(v))?;
    Ok(g.accepted(id))
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ForecastState,
    setup: &ForecastSetup,
) -> Result<()> {
    let kind = match s.state {
        ForecastStatus::Framing => WorkKind::ForecastFrame,
        ForecastStatus::Forecasting => WorkKind::ForecastEstimate,
        ForecastStatus::Revising => WorkKind::ForecastRevise,
        ForecastStatus::Explaining => WorkKind::ForecastCruxes,
        _ => return Err(invalid()),
    };
    let judge = kind.new_mode_judge();
    let name = kind.new_mode_composition().ok_or_else(invalid)?;
    let targets = if judge {
        vec![Target::Judge]
    } else if kind == WorkKind::ForecastRevise {
        firsts(tx, setup)?
            .keys()
            .cloned()
            .map(Target::Member)
            .collect()
    } else {
        setup
            .participants
            .iter()
            .cloned()
            .map(Target::Member)
            .collect()
    };
    let step_id = StepId::new();
    let case_raw: String = tx.query_row(
        "SELECT case_id FROM forecast_runs WHERE contest_id=?",
        [s.contest_id.to_string()],
        |r| r.get(0),
    )?;
    let case: CaseId = case_raw.parse().map_err(|_| invalid())?;
    let frozen_raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='forecast.prompts'",[s.contest_id.to_string()],|r|r.get(0))?;
    let frozen: serde_json::Value = serde_json::from_str(&frozen_raw)?;
    let pack = PromptPack::embedded().map_err(|_| invalid())?;
    let mut assignments = vec![];
    let mut pending = vec![];
    for target in targets {
        let context = if judge {
            ContextId(format!("judge:{case}"))
        } else {
            ContextId(format!("contest:{}:{}:{}", s.contest_id, step_id, target))
        };
        let mut data = match kind {
            WorkKind::ForecastFrame => {
                serde_json::json!({"question":setup.question,"background":setup.background,"asOf":g.accepted_at.format("%Y-%m-%d").to_string()})
            }
            WorkKind::ForecastEstimate => {
                serde_json::json!({"frame":frame(tx,s.contest_id)?,"background":script::blind(&setup.background,setup)})
            }
            WorkKind::ForecastRevise => {
                let Target::Member(n) = &target else {
                    return Err(invalid());
                };
                let bundle: ForecastBundle = record(tx, s.contest_id, &format!("bundle:{n}"))?;
                let own: ForecastEstimate = record(tx, s.contest_id, &format!("first:{n}"))?;
                serde_json::json!({"frame":frame(tx,s.contest_id)?,"background":script::blind(&setup.background,setup),"ownFirstForecast":own,"others":bundle.entries,"labels":bundle.entries.iter().map(|e|&e.label).collect::<Vec<_>>()})
            }
            WorkKind::ForecastCruxes => {
                let members = results(tx, setup)?;
                let labelled=members.iter().map(|m|serde_json::json!({"label":m.label,"first":m.first,"revision":m.revision,"finalProbabilityPct":m.final_probability_pct,"notRevised":m.not_revised})).collect::<Vec<_>>();
                serde_json::json!({"frame":frame(tx,s.contest_id)?,"background":script::blind(&setup.background,setup),"forecasts":labelled,"labels":members.iter().map(|m|&m.label).collect::<Vec<_>>(),"firstAggregate":record::<ForecastAggregate>(tx,s.contest_id,"firstAggregate")?,"finalAggregate":record::<ForecastAggregate>(tx,s.contest_id,"finalAggregate")?})
            }
            _ => return Err(invalid()),
        };
        // Scrub all strings while preserving ids, enums and numbers in authorized data.
        fn redact(v: &mut serde_json::Value, setup: &ForecastSetup) {
            match v {
                serde_json::Value::String(s) => *s = script::blind(s, setup),
                serde_json::Value::Array(a) => {
                    for x in a {
                        redact(x, setup)
                    }
                }
                serde_json::Value::Object(o) => {
                    for (key, x) in o {
                        if !["label", "labels", "citedLabels", "direction"].contains(&key.as_str())
                        {
                            redact(x, setup)
                        }
                    }
                }
                _ => {}
            }
        }
        redact(&mut data, setup);
        data["wordLimit"] = serde_json::json!(match kind {
            WorkKind::ForecastFrame => 300,
            WorkKind::ForecastRevise => 200,
            WorkKind::ForecastCruxes => setup.plan_words,
            _ => setup.room_words,
        });
        let binding = setup
            .models
            .get(&target.to_string())
            .ok_or_else(invalid)?
            .clone();
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
            case_id: judge.then_some(case),
            prompt_ref: Some(setup.prompt_refs[name].clone()),
            max_output_tokens: kind.new_mode_visible_tokens(),
            contest_id: Some(s.contest_id),
            step_id: Some(step_id),
        };
        let mut values = crate::prompts::inputs_forecast::inputs(&pack, &item, &data, None)
            .map_err(|_| invalid())?;
        values.extend(BTreeMap::from([
            ("nick".into(), serde_json::json!(target.to_string())),
            (
                "model_slug".into(),
                serde_json::json!(binding.model.as_str()),
            ),
            (
                "judge_model_slug".into(),
                serde_json::json!(binding.model.as_str()),
            ),
            ("member_nicks_json".into(), serde_json::json!([])),
            ("user_name_json".into(), serde_json::json!("the user")),
            ("persona_json".into(), serde_json::json!("")),
            (
                "room_word_limit".into(),
                serde_json::json!(setup.room_words),
            ),
        ]));
        let source = &frozen["compositions"][name];
        let sys = source["system"].as_str().ok_or_else(invalid)?;
        let user = source["user"].as_str().ok_or_else(invalid)?;
        let combined = format!("{sys}\n{user}");
        values.retain(|k, _| combined.contains(&format!("{{{{{k}}}}}")));
        let mut rendered = pack
            .render_frozen_composition(name, sys, user, &values)
            .map_err(|_| invalid())?;
        if matches!(kind, WorkKind::ForecastCruxes | WorkKind::ForecastRevise) {
            rendered.system = script::blind(&rendered.system, setup)
        }
        let (context_length, output_limit, params_json): (u32, Option<u32>, String) = tx
            .query_row(
                "SELECT context_length,max_output,supported_params_json FROM catalog WHERE slug=?",
                [binding.model.as_str()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
        let model = squad_agents::resolver::Model {
            id: binding.model.to_string(),
            context_length,
            output_limit: output_limit.unwrap_or(8192),
            supported_parameters: serde_json::from_str(&params_json)?,
            reasoning_efforts: vec![],
            pricing: serde_json::json!({}),
        };
        let input = serde_json::json!({"data":data,"rendered":rendered,"model":model,"repairTemplate":frozen["repair"]});
        assignments.push(ContestAssignment {
            target,
            context_id: context,
            payload_refs: vec![],
            work_id: Some(item.work_id),
        });
        pending.push((item, binding, input));
    }
    let step = ContestStep {
        contest_id: s.contest_id,
        step_id,
        ordinal: match kind {
            WorkKind::ForecastFrame => 1,
            WorkKind::ForecastEstimate => 2,
            WorkKind::ForecastRevise => 3,
            _ => 4,
        },
        script_version: Text::new("forecast-v1").expect("script"),
        move_name: if judge {
            ContestMove::Judge
        } else {
            ContestMove::Fanout
        },
        inputs: vec![],
        assignments,
    };
    let mut event = envelope(
        id,
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestStep(step.clone()),
    );
    event.base.id = id;
    let seq = g.push(tx, event)?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Forecast, seq)?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (mut item, binding, input) in pending {
        if judge {
            tx.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES(?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}') ON CONFLICT(case_id) DO UPDATE SET material_json=excluded.material_json",params![case.to_string(),item.context_id.0,s.contest_id.to_string(),binding.model.as_str(),input["data"].to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: item.context_id.clone(),
            origin_event_id: id,
            projection_key: Text::new(format!("forecast.input:{step_id}"))
                .map_err(|_| invalid())?,
            role: HistoryRole::User,
            name: Some("squad".into()),
            content: input.to_string(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        item.history_through =
            squad_store::history::project(tx, &projection)?.ok_or_else(invalid)?;
        squad_store::work::insert(tx, &item, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step_id, &item)?;
        tx.execute(
            "INSERT INTO forecast_work_inputs VALUES(?,?)",
            params![item.work_id.to_string(), input.to_string()],
        )?;
        g.push(
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
    let mut next = s.clone();
    next.step_id = Some(step_id);
    next.state_version += 1;
    save(tx, g, id, &next, Some(s.state_version))
}
pub fn accepted_work(tx: &Connection, w: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    squad_store::forecast::accept(tx, w, a, seq)
}
pub fn human_answer(
    _tx: &Connection,
    _pending: &parser::PendingHuman,
    _complete: bool,
) -> Result<()> {
    Err(invalid())
}
pub fn stop(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
) -> Result<()> {
    terminal(
        tx,
        g,
        id,
        load_state(tx, contest)?,
        ForecastStatus::Stopped,
        "stopped",
    )?;
    Ok(())
}
pub fn recover(tx: &Connection, contest: ContestId) -> Result<()> {
    load_state(tx, contest)?.validate().map_err(|_| invalid())
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let state = load_state(tx, submission.contest_id)?;
    if submission.kind != ContestKind::Forecast
        || state.state_version != submission.expected_state_version
        || state.state.is_terminal()
        || !matches!(
            submission.transition,
            ContestTransition::Forecast(ForecastTransition::Advance)
        )
    {
        return Err(invalid());
    }
    drive_move(tx, g, id, state)?;
    Ok(())
}
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
pub fn barrier(tx: &Transaction<'_>, g: &mut CommitGroup, commit: BarrierCommit) -> Result<()> {
    let state = load_state(tx, commit.contest_id)?;
    if state.step_id != Some(commit.step_id)
        || state.state_version != commit.expected_state_version
        || !commit.projections.is_empty()
        || !commit.next_work.is_empty()
    {
        return Err(invalid());
    }
    drive_move(tx, g, commit.barrier_event.base.id, state)?;
    Ok(())
}
