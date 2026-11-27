//! Transactional Negotiate admission, frozen projections, barriers and recovery.
use crate::{orchestrator::negotiate as policy, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};
use squad_protocol::negotiate::*;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup, negotiate as storage};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid negotiate transition",
    )
}
fn push(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    c: ContestId,
    p: EnvelopePayload,
) -> Result<Cursor> {
    let step = matches!(&p, EnvelopePayload::ContestStep(_));
    let mut event = super::envelope(id, Actor::System, vec![Recipient::Me], p);
    if step {
        event.base.id = id;
    }
    let seq = g.push(tx, event)?;
    squad_store::contests::associate(tx, c, ContestKind::Negotiate, seq)?;
    Ok(seq)
}
fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &NegotiateState,
    expected: Option<u64>,
) -> Result<()> {
    let mut canonical = s.clone();
    canonical.artifact_event_ids = storage::events(tx, s.contest_id)?
        .into_iter()
        .filter_map(|e| {
            matches!(e.content, EnvelopePayload::ContestArtifact(_)).then_some(e.base.id)
        })
        .collect();
    let s = &canonical;
    let seq = g.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::NegotiateState(s.clone()),
        ),
    )?;
    storage::save(tx, s, expected, seq)?;
    if s.state.is_terminal() {
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
        for (key, day, amount) in rows {
            tx.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![amount, day],
            )?;
            tx.execute("DELETE FROM reservations WHERE reservation_id=?", [key])?;
        }
    }
    Ok(())
}
pub fn load_state(db: &Connection, c: ContestId) -> Result<NegotiateState> {
    storage::state(db, c)
}
pub fn steps(
    participants: &[MemberNick],
    our_side: &str,
    counterpart: &str,
    templates: &[u64],
    room_words: u32,
    rounds: u32,
) -> Vec<super::bounds::Step> {
    use super::bounds::*;
    let m = participants.len() as u64;
    let p = table_count(participants.len()) as u32;
    let ours = json_text(our_side);
    let other = json_text(counterpart);
    let maps = m * project(limited(2048, room_words));
    let transcript = 2 * u64::from(rounds) * limited(2048, 200);
    let bounds = [
        ours + other,
        ours + other + maps,
        ours + limited(4096, 600),
        ours + limited(4096, 600) + limited(2048, 200) + transcript,
        2 * limited(4096, 300) + transcript,
        ours + other
            + maps
            + artifact(4096)
            + (m - u64::from(p)) * limited(2048, 200)
            + u64::from(p) * transcript,
    ];
    let kinds = [
        WorkKind::NegotiateMap,
        WorkKind::NegotiateBrief,
        WorkKind::NegotiatePackage,
        WorkKind::NegotiateMove,
        WorkKind::NegotiateCounter,
        WorkKind::NegotiatePlaybook,
    ];
    let counts = [m as u32, 1, m as u32 - p, p * rounds, p * rounds, 1];
    kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| {
            let judge = matches!(kind, WorkKind::NegotiateBrief | WorkKind::NegotiatePlaybook);
            Step {
                schema: schema_bytes(output_schema(&kind).expect("schema")),
                kind,
                target: if judge {
                    "judge".into()
                } else {
                    participants[0].to_string()
                },
                alternatives: if judge {
                    vec![]
                } else {
                    participants
                        .iter()
                        .skip(1)
                        .map(ToString::to_string)
                        .collect()
                },
                template: templates[i],
                data: bounds[i] + 1024,
                visible: if judge { 4096 } else { 2048 },
                calls: counts[i],
                attempts: if judge { 3 } else { 2 },
            }
        })
        .collect()
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    _input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::Negotiate {
        input,
        participants,
        our_side,
        counterpart,
        rounds,
    } = parsed
    else {
        return Err(invalid());
    };
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running.",
        ));
    }
    let config = a.config.ok_or_else(invalid)?;
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let c = ContestId::new();
    let names = [
        "negotiate.map",
        "negotiate.brief",
        "negotiate.package",
        "negotiate.move",
        "negotiate.counter",
        "negotiate.playbook",
    ];
    let mut prompts = serde_json::Map::new();
    for name in names {
        let (system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        let (repair_system, repair_user) =
            pack.composition_source(name, true).map_err(|_| invalid())?;
        let repair_user = repair_user.replace("{{mode_step}}", name);
        prompts.insert(name.into(),json!({"system":system,"user":user,"repairSystem":repair_system,"repairUser":repair_user}));
    }
    let mut models = a.bindings.clone();
    models.get_mut("judge").ok_or_else(invalid)?.effort =
        constants::NEW_MODE_JUDGE_REASONING_EFFORT;
    let setup = NegotiateSetup {
        pack_version: pack.version().to_owned(),
        our_side: our_side.clone(),
        counterpart: counterpart.clone().unwrap_or_default(),
        participants: participants.clone(),
        rounds: u32::from(*rounds),
        tables: policy::draw(participants),
        models,
        prompts: Value::Object(prompts),
        room_words: config.prompts.limits.room_words,
        plan_words: config.prompts.limits.plan_words,
        redaction_tokens: config
            .agents
            .iter()
            .flat_map(|m| [m.nick.to_string(), m.model.clone(), m.persona.clone()])
            .chain(std::iter::once(config.judge.model.clone()))
            .collect(),
    };
    if participants.len() < 4 {
        return Err(invalid());
    }
    let templates = names
        .map(|n| super::bounds::template(&pack, n))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let priced = super::bounds::price(
        tx,
        &setup.models,
        &steps(
            participants,
            our_side,
            &setup.counterpart,
            &templates,
            setup.room_words,
            setup.rounds,
        ),
    )?;
    setup.validate().map_err(|_| invalid())?;
    storage::ensure_holds(tx)?;
    let state = NegotiateState {
        contest_id: c,
        state_version: 1,
        state: NegotiateStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &state, None)?;
    storage::put(tx, c, "setup", &serde_json::to_value(&setup)?)?;
    storage::journal(tx, c, "tables", &json!(setup.tables))?;
    storage::journal(tx, c, "counterTurn", &json!(false))?;
    storage::journal(tx, c, "case", &json!(CaseId::new()))?;
    push(
        tx,
        g,
        id,
        c,
        EnvelopePayload::NegotiateSetup(ContestSetup {
            contest_id: c,
            kind: ContestKind::Negotiate,
            script_version: "negotiate-v1".parse().expect("literal"),
            participants: participants.clone(),
            source_event_ids: vec![],
            settings: json!({"ourSide":our_side,"counterpart":setup.counterpart,"tables":setup.tables,"rounds":setup.rounds}),
        }),
    )?;
    super::variables::apply(tx, g, id, &input.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    settings["reservedAttempts"] = json!(attempts(participants.len(), setup.rounds));
    settings["memberCalls"] = json!(calls(participants.len(), setup.rounds) - 2);
    settings["judgeCalls"] = json!(2);
    let plan = ContestPreflight {
        contest_id: c,
        kind: ContestKind::Negotiate,
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
            model_calls: calls(participants.len(), setup.rounds),
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
        c,
        EnvelopePayload::ContestPreflight(plan.clone()),
    )?;
    tx.execute("UPDATE contests SET plan_id=?,plan_json=?,expires_at=?,pending_request_json=? WHERE contest_id=?",params![plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),json!({"max":a.daily_max,"preconfirmed":a.preconfirmed}).to_string(),c.to_string()])?;
    if priced.reserved_micros
        > a.daily_max
            .saturating_sub(daily.spent_micros + daily.reserved_micros)
    {
        let mut s = state;
        s.state = NegotiateStatus::Failed;
        s.state_version += 1;
        s.terminal_reason = Some("daily_max_reached".parse().expect("literal"));
        save(tx, g, id, &s, Some(1))?;
        let mut e = super::daily_cap_error(tx, &day, priced.reserved_micros, a.daily_max);
        e.contest_id = Some(c);
        push(
            tx,
            g,
            id,
            c,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: c,
                plan_id: plan.plan_id,
                status: PreflightStatus::Rejected,
                confirmation: None,
                error: Some(e.clone()),
            }),
        )?;
        return Ok(RequestResult::Rejected {
            request_id: id,
            error: e,
        });
    }
    if !plan.confirmation_required || a.preconfirmed {
        resolve(tx, g, id, c, plan.plan_id, false)
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
    g: &mut CommitGroup,
    id: EventId,
    c: ContestId,
    p: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let mut s = load_state(tx, c)?;
    let (raw, pending): (String, String) = tx.query_row(
        "SELECT plan_json,pending_request_json FROM contests WHERE contest_id=?",
        [c.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    let pending: Value = serde_json::from_str(&pending)?;
    if s.state != NegotiateStatus::Preflight || p != plan.plan_id {
        return Err(invalid());
    }
    let expired = g.accepted_at >= plan.expires_at;
    let old = s.state_version;
    s.state_version += 1;
    if cancel || expired {
        s.state = NegotiateStatus::Stopped;
        s.terminal_reason = Some(
            if expired { "expired" } else { "cancelled" }
                .parse()
                .expect("literal"),
        );
    } else {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        squad_store::accounting::reserve(
            tx,
            &format!("plan:{p}"),
            None,
            &day,
            squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
                .map_err(|_| invalid())?,
            pending["max"].as_i64().ok_or_else(invalid)?,
        )?;
        tx.execute(
            "UPDATE reservations SET plan_id=?,contest_id=? WHERE reservation_id=?",
            params![p.to_string(), c.to_string(), format!("plan:{p}")],
        )?;
        s.state = NegotiateStatus::WaitingForParticipants;
    }
    push(
        tx,
        g,
        id,
        c,
        EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
            contest_id: c,
            plan_id: p,
            status: if expired {
                PreflightStatus::Expired
            } else if cancel {
                PreflightStatus::Cancelled
            } else {
                PreflightStatus::Confirmed
            },
            confirmation: if cancel || expired {
                None
            } else {
                Some(if pending["preconfirmed"] == true {
                    Confirmation::Preconfirmed
                } else if plan.confirmation_required {
                    Confirmation::Explicit
                } else {
                    Confirmation::Automatic
                })
            },
            error: None,
        }),
    )?;
    save(tx, g, id, &s, Some(old))?;
    Ok(g.accepted(id))
}
pub fn accepted_work(db: &Connection, w: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    storage::accept(db, w, a, seq)
}
pub fn human_answer(_db: &Connection, _p: &parser::PendingHuman, _complete: bool) -> Result<()> {
    Err(invalid())
}
pub fn stop(tx: &Transaction<'_>, g: &mut CommitGroup, id: EventId, c: ContestId) -> Result<()> {
    let mut s = load_state(tx, c)?;
    if s.state.is_terminal() {
        return Ok(());
    }
    let v = s.state_version;
    if s.step_id.is_some() {
        report(tx, g, id, &s)?;
    }
    s.state = NegotiateStatus::Stopped;
    s.state_version += 1;
    s.terminal_reason = Some("stopped".parse().expect("literal"));
    save(tx, g, id, &s, Some(v))
}
pub fn recover(db: &Connection, c: ContestId) -> Result<()> {
    storage::ensure_holds(db)?;
    storage::setup(db, c)?.validate().map_err(|_| invalid())?;
    load_state(db, c)?.validate().map_err(|_| invalid())
}
fn maps(db: &Connection, c: ContestId) -> Result<BTreeMap<MemberNick, NegotiateMap>> {
    storage::artifacts(db, c, "negotiate.map")?
        .into_iter()
        .map(|(n, v)| Ok((n, serde_json::from_value(v)?)))
        .collect()
}
fn packages(db: &Connection, c: ContestId) -> Result<BTreeMap<MemberNick, NegotiatePackage>> {
    storage::artifacts(db, c, "negotiate.package")?
        .into_iter()
        .map(|(n, v)| Ok((n, serde_json::from_value(v)?)))
        .collect()
}
fn judge_output<T: serde::de::DeserializeOwned>(
    db: &Connection,
    c: ContestId,
    kind: &str,
) -> Result<Option<T>> {
    for e in storage::events(db, c)? {
        if let EnvelopePayload::ContestArtifact(a) = e.content
            && a.artifact_kind.as_str() == kind
        {
            return Ok(Some(serde_json::from_value(a.body.ok_or_else(invalid)?)?));
        }
    }
    Ok(None)
}
fn tables(db: &Connection, c: ContestId) -> Result<Vec<NegotiateTable>> {
    Ok(serde_json::from_value(storage::read_journal(
        db, c, "tables",
    )?)?)
}
fn own_artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &NegotiateState,
    kind: &str,
    body: Value,
) -> Result<()> {
    let a = ContestArtifact {
        kind: Some(ContestKind::Negotiate),
        round: Some(s.round),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        contest_id: s.contest_id,
        step_id: s.step_id.ok_or_else(invalid)?,
        artifact_kind: kind.parse().map_err(|_| invalid())?,
        work_id: None,
        source_refs: vec![],
        data: body,
    };
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestArtifact(a.clone()),
    )?;
    squad_store::contest_steps::artifact(tx, &a, seq)
}
fn finish(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut NegotiateState,
    reason: &str,
) -> Result<()> {
    report(tx, g, id, s)?;
    s.state = NegotiateStatus::Failed;
    s.terminal_reason = Some(reason.parse().map_err(|_| invalid())?);
    let old = s.state_version;
    s.state_version += 1;
    save(tx, g, id, s, Some(old))
}
/// Advance at most one durable move. No accepted work is allocated twice.
pub(crate) fn advance(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut s: NegotiateState,
) -> Result<()> {
    if load_state(tx, s.contest_id)? != s {
        return Err(invalid());
    }
    let setup = storage::setup(tx, s.contest_id)?;
    let old = s.state_version;
    if s.state.is_terminal() {
        return Ok(());
    }
    if s.state == NegotiateStatus::Preflight {
        let raw: String = tx.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )?;
        let plan: ContestPreflight = serde_json::from_str(&raw)?;
        if g.accepted_at >= plan.expires_at {
            resolve(tx, g, id, s.contest_id, plan.plan_id, true)?;
        }
        return Ok(());
    }
    if s.state == NegotiateStatus::WaitingForParticipants {
        for n in &setup.participants {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get::<_,bool>(0))?{return Ok(());}
        }
        s.state = NegotiateStatus::Mapping;
        s.state_version += 1;
        return save(tx, g, id, &s, Some(old));
    }
    if s.state == NegotiateStatus::Reporting {
        report(tx, g, id, &s)?;
        s.state = NegotiateStatus::Complete;
        s.terminal_reason = Some("rehearsed".parse().expect("literal"));
        s.state_version += 1;
        return save(tx, g, id, &s, Some(old));
    }
    if s.step_id.is_none() {
        return allocate(tx, g, id, &s, &setup);
    }
    let step = s.step_id.ok_or_else(invalid)?;
    let mut q =
        tx.prepare("SELECT work_id FROM contest_assignments WHERE step_id=? ORDER BY target")?;
    let ids = q
        .query_map([step.to_string()], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for wid in &ids {
        let w: WorkId = wid.parse().map_err(|_| invalid())?;
        let mut record = squad_store::work::record(tx, w)?.ok_or_else(invalid)?;
        let work = squad_store::work::template(tx, w)?.ok_or_else(invalid)?.0;
        let limit = if matches!(
            work.kind,
            WorkKind::NegotiateBrief | WorkKind::NegotiatePlaybook
        ) {
            3
        } else {
            2
        };
        // Recovery preserves consumed provider attempts. A queued last attempt
        // has no dispatch capacity left and must settle instead of waiting forever.
        if record.status == WorkStatus::Queued && record.attempt >= limit {
            let failure = super::error(
                ErrorCode::PROVIDER_TIMEOUT,
                "Negotiate attempt interrupted and exhausted",
            );
            let seq = push(
                tx,
                g,
                id,
                s.contest_id,
                EnvelopePayload::WorkState(WorkStateEvent {
                    work_id: w,
                    trigger_event_id: work.trigger_event_id,
                    target: work.target,
                    context_id: work.context_id,
                    attempt: record.attempt,
                    status: WorkStatus::Failed,
                    error: Some(failure),
                    debate_id: None,
                    case_id: work.case_id,
                    contest_id: work.contest_id,
                }),
            )?;
            tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1,completed_event_seq=? WHERE work_id=? AND status='queued'",params![seq.get(),w.to_string()])?;
            squad_store::accounting::release_unstarted_work(tx, w)?;
            record.status = WorkStatus::Failed;
        }
        if matches!(record.status, WorkStatus::Queued | WorkStatus::Running) {
            return Ok(());
        }
    }
    let mut skipped_work_ids = vec![];
    for wid in ids {
        let w: WorkId = wid.parse().map_err(|_| invalid())?;
        let record = squad_store::work::record(tx, w)?.ok_or_else(invalid)?;
        let work = squad_store::work::template(tx, w)?.ok_or_else(invalid)?.0;
        if record.status != WorkStatus::Completed {
            skipped_work_ids.push(w);
            let e = super::error(
                ErrorCode::MODE_INVALID_OUTPUT,
                "Negotiate slot skipped after exhausted attempts",
            );
            squad_store::contest_steps::settle_if_pending(tx, step, &work.target, Some(&e))?;
        }
    }
    let artifact_event_ids = storage::events(tx, s.contest_id)?
        .into_iter()
        .filter_map(|e| match e.content {
            EnvelopePayload::ContestArtifact(a) if a.step_id == step => Some(e.base.id),
            _ => None,
        })
        .collect();
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: s.contest_id,
            step_id: step,
            artifact_event_ids,
            skipped_work_ids,
            data: json!({"barrier":s.state}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    match s.state {
        NegotiateStatus::Mapping => {
            if maps(tx, s.contest_id)?.len() < 2 {
                return finish(tx, g, id, &mut s, "insufficient_maps");
            }
            s.state = NegotiateStatus::Briefing;
        }
        NegotiateStatus::Briefing => {
            if judge_output::<NegotiateBrief>(tx, s.contest_id, "negotiate.brief")?.is_none() {
                return finish(tx, g, id, &mut s, "judge_failed");
            }
            s.state = NegotiateStatus::Packaging;
        }
        NegotiateStatus::Packaging => {
            s.state = NegotiateStatus::Rehearsing;
            s.round = 1;
        }
        NegotiateStatus::Rehearsing => {
            let mut ts = tables(tx, s.contest_id)?;
            let counter = storage::read_journal(tx, s.contest_id, "counterTurn")?
                .as_bool()
                .ok_or_else(invalid)?;
            let mut ended = vec![];
            for t in ts.iter_mut().filter(|t| t.outcome.is_none()) {
                let who = if counter {
                    &t.counterpart
                } else {
                    &t.negotiator
                };
                let row = storage::events(tx, s.contest_id)?
                    .into_iter()
                    .find_map(|e| match e.content {
                        EnvelopePayload::ContestArtifact(a)
                            if a.step_id == step && a.actor == Some(Actor::Member(who.clone())) =>
                        {
                            a.body
                        }
                        _ => None,
                    });
                t.rounds_used = s.round;
                if let Some(body) = row {
                    t.turns.push(NegotiateTurn {
                        table: t.table,
                        round: s.round,
                        actor: who.clone(),
                        counterpart: counter,
                        body: body.clone(),
                    });
                    if counter {
                        let reply: NegotiateCounter = serde_json::from_value(body)?;
                        t.outcome = policy::outcome(reply.response, s.round, setup.rounds);
                    }
                } else {
                    t.outcome = Some(NegotiateOutcome::Abandoned);
                }
                if t.outcome.is_some() {
                    ended.push(t.clone());
                }
                if let Some(turn) = t
                    .turns
                    .last()
                    .filter(|turn| turn.round == s.round && turn.counterpart == counter)
                {
                    own_artifact(tx, g, id, &s, "negotiate.turn", json!(turn))?;
                }
            }
            storage::journal(tx, s.contest_id, "tables", &json!(ts))?;
            if ts.iter().all(|t| t.outcome.is_some()) {
                s.state = NegotiateStatus::WritingPlaybook;
                s.round = 0;
            } else {
                if counter {
                    s.round += 1;
                }
                storage::journal(tx, s.contest_id, "counterTurn", &json!(!counter))?;
            }
            release_future(tx, s.contest_id, &ended, setup.rounds, counter)?;
        }
        NegotiateStatus::WritingPlaybook => {
            if judge_output::<NegotiatePlaybook>(tx, s.contest_id, "negotiate.playbook")?.is_none()
            {
                return finish(tx, g, id, &mut s, "judge_failed");
            }
            s.state = NegotiateStatus::Reporting;
        }
        _ => return Err(invalid()),
    }
    // Reporting retains the last committed step as the report's anchor.
    if s.state != NegotiateStatus::Reporting {
        s.step_id = None;
    }
    s.artifact_event_ids = storage::events(tx, s.contest_id)?
        .into_iter()
        .filter_map(|e| {
            matches!(e.content, EnvelopePayload::ContestArtifact(_)).then_some(e.base.id)
        })
        .collect();
    s.state_version += 1;
    save(tx, g, id, &s, Some(old))
}
/// Role-specific projections are assembled in the owned runtime module.
fn data(
    db: &Connection,
    s: &NegotiateState,
    setup: &NegotiateSetup,
    target: &Target,
    kind: &WorkKind,
) -> Result<Value> {
    let brief = judge_output::<NegotiateBrief>(db, s.contest_id, "negotiate.brief")?;
    let ms = maps(db, s.contest_id)?;
    let ps = packages(db, s.contest_id)?;
    let ts = tables(db, s.contest_id)?;
    crate::runtime::negotiate::projection(
        setup,
        s.state,
        s.round,
        target,
        kind,
        &ms,
        brief.as_ref(),
        &ps,
        &ts,
    )
    .map_err(|_| invalid())
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &NegotiateState,
    setup: &NegotiateSetup,
) -> Result<()> {
    let counter = storage::read_journal(tx, s.contest_id, "counterTurn")?
        .as_bool()
        .ok_or_else(invalid)?;
    let kind = match s.state {
        NegotiateStatus::Mapping => WorkKind::NegotiateMap,
        NegotiateStatus::Briefing => WorkKind::NegotiateBrief,
        NegotiateStatus::Packaging => WorkKind::NegotiatePackage,
        NegotiateStatus::Rehearsing => {
            if counter {
                WorkKind::NegotiateCounter
            } else {
                WorkKind::NegotiateMove
            }
        }
        NegotiateStatus::WritingPlaybook => WorkKind::NegotiatePlaybook,
        _ => return Err(invalid()),
    };
    let judge = matches!(kind, WorkKind::NegotiateBrief | WorkKind::NegotiatePlaybook);
    let targets: Vec<Target> = match s.state {
        NegotiateStatus::Mapping => setup
            .participants
            .iter()
            .cloned()
            .map(Target::Member)
            .collect(),
        NegotiateStatus::Packaging => setup
            .participants
            .iter()
            .filter(|n| !setup.tables.iter().any(|t| &t.counterpart == *n))
            .cloned()
            .map(Target::Member)
            .collect(),
        NegotiateStatus::Rehearsing => tables(tx, s.contest_id)?
            .into_iter()
            .filter(|t| t.outcome.is_none())
            .map(|t| Target::Member(if counter { t.counterpart } else { t.negotiator }))
            .collect(),
        _ => vec![Target::Judge],
    };
    let step_id = StepId::new();
    let name = storage::composition(&kind).ok_or_else(invalid)?;
    let mut items = vec![];
    let mut assignments = vec![];
    for target in targets {
        let case = if judge {
            Some(serde_json::from_value::<CaseId>(storage::read_journal(
                tx,
                s.contest_id,
                "case",
            )?)?)
        } else {
            None
        };
        let context = ContextId(if let Some(case) = case {
            format!("judge:{case}")
        } else {
            format!("contest:{}:{}:{}", s.contest_id, step_id, target)
        });
        let binding = setup
            .models
            .get(&target.to_string())
            .ok_or_else(invalid)?
            .clone();
        let source = &setup.prompts[name];
        let prompt = PromptRef {
            pack_version: setup.pack_version.parse().map_err(|_| invalid())?,
            template_id: name.parse().expect("literal"),
            template_hash: squad_store::commit::digest(source)?
                .parse()
                .map_err(|_| invalid())?,
        };
        let d = data(tx, s, setup, &target, &kind)?;
        let (length,output,pricing,params):(u32,Option<u32>,String,String)=tx.query_row("SELECT context_length,max_output,pricing_json,supported_params_json FROM catalog WHERE slug=?",[binding.model.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let model = squad_agents::resolver::Model {
            id: binding.model.to_string(),
            context_length: length,
            output_limit: output.ok_or_else(invalid)?,
            supported_parameters: serde_json::from_str(&params)?,
            reasoning_efforts: vec![],
            pricing: serde_json::from_str(&pricing)?,
        };
        let invocation = json!({"data":d,"prompt":source,"model":model});
        let w = Work {
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
            prompt_ref: Some(prompt),
            max_output_tokens: Some(if judge { 4096 } else { 2048 }),
            contest_id: Some(s.contest_id),
            step_id: Some(step_id),
        };
        assignments.push(ContestAssignment {
            target,
            context_id: context,
            payload_refs: vec![],
            work_id: Some(w.work_id),
        });
        items.push((w, binding, invocation));
    }
    let ordinal = match s.state {
        NegotiateStatus::Mapping => 1,
        NegotiateStatus::Briefing => 2,
        NegotiateStatus::Packaging => 3,
        NegotiateStatus::Rehearsing => 4 + 2 * (s.round - 1) + u32::from(counter),
        _ => 4 + 2 * setup.rounds,
    };
    let step = ContestStep {
        contest_id: s.contest_id,
        step_id,
        ordinal,
        script_version: "negotiate-v1".parse().expect("literal"),
        move_name: if judge {
            ContestMove::Judge
        } else if s.state == NegotiateStatus::Rehearsing {
            ContestMove::Turns
        } else {
            ContestMove::Fanout
        },
        inputs: vec![],
        assignments,
    };
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestStep(step.clone()),
    )?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    if s.state == NegotiateStatus::Mapping {
        let mut anchor = s.clone();
        anchor.step_id = Some(step_id);
        own_artifact(tx, g, id, &anchor, "negotiate.roles", json!(setup))?;
    }
    for (mut w, binding, input) in items {
        if let Some(case) = w.case_id {
            tx.execute("INSERT OR IGNORE INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![case.to_string(),w.context_id.0,s.contest_id.to_string(),binding.model.as_str(),input.to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: w.context_id.clone(),
            origin_event_id: id,
            projection_key: "negotiate.step".parse().expect("literal"),
            role: HistoryRole::User,
            name: Some("squad".into()),
            content: input.to_string(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        w.history_through = squad_store::history::project(tx, &projection)?.ok_or_else(invalid)?;
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step_id, &w)?;
        tx.execute(
            "INSERT INTO negotiate_inputs VALUES (?,?)",
            params![w.work_id.to_string(), input.to_string()],
        )?;
        push(
            tx,
            g,
            id,
            s.contest_id,
            EnvelopePayload::WorkState(WorkStateEvent {
                work_id: w.work_id,
                trigger_event_id: id,
                target: w.target,
                context_id: w.context_id,
                attempt: 0,
                status: WorkStatus::Queued,
                error: None,
                debate_id: None,
                case_id: w.case_id,
                contest_id: w.contest_id,
            }),
        )?;
    }
    let mut next = s.clone();
    next.step_id = Some(step_id);
    next.state_version += 1;
    save(tx, g, id, &next, Some(s.state_version))
}
fn report(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &NegotiateState,
) -> Result<()> {
    let c = s.contest_id;
    let ts = tables(tx, c)?;
    let mut q=tx.prepare("SELECT a.target,s.ordinal FROM contest_assignments a JOIN contest_steps s USING(step_id) WHERE s.contest_id=? AND a.outcome='skipped' ORDER BY s.ordinal,a.target")?;
    let skipped = q
        .query_map([c.to_string()], |r| {
            Ok(format!(
                "{}: step {}",
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let r = NegotiateReport {
        setup: storage::setup(tx, c)?,
        maps: maps(tx, c)?,
        brief: judge_output(tx, c, "negotiate.brief")?,
        packages: packages(tx, c)?,
        counts: policy::tally(&ts),
        no_rehearsal_evidence: ts
            .iter()
            .all(|t| t.outcome == Some(NegotiateOutcome::Abandoned) || t.turns.is_empty()),
        tables: ts,
        playbook: judge_output(tx, c, "negotiate.playbook")?,
        skipped,
    };
    own_artifact(tx, g, id, s, "negotiate.report", json!(r))
}
/// Release only impossible future turns; started attempt holds belong to shared accounting.
fn release_future(
    db: &Connection,
    c: ContestId,
    ended: &[NegotiateTable],
    rounds: u32,
    counter: bool,
) -> Result<()> {
    use rusqlite::OptionalExtension;
    let raw: String = db.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [c.to_string()],
        |r| r.get(0),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    let (moves, counters) = policy::released_calls(ended, rounds, counter);
    let share = |kind: &str| {
        plan.settings["attemptShares"][kind]
            .as_i64()
            .ok_or_else(invalid)
    };
    let amount = 2
        * (i64::from(moves) * share("negotiate.move")?
            + i64::from(counters) * share("negotiate.counter")?);
    if amount == 0 {
        return Ok(());
    }
    let key = format!("plan:{}", plan.plan_id);
    let row: Option<(String, i64)> = db
        .query_row(
            "SELECT day,amount_micros FROM reservations WHERE reservation_id=?",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((day, remaining)) = row {
        if amount > remaining {
            return Err(invalid());
        }
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![amount, day],
        )?;
        db.execute(
            "UPDATE reservations SET amount_micros=amount_micros-? WHERE reservation_id=?",
            params![amount, key],
        )?;
    }
    Ok(())
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    sub: ContestTransitionSubmission,
) -> Result<()> {
    let s = load_state(tx, sub.contest_id)?;
    if sub.kind != ContestKind::Negotiate || sub.expected_state_version != s.state_version {
        return Err(invalid());
    }
    match sub.transition {
        ContestTransition::Negotiate(NegotiateTransition::Advance) => advance(tx, g, id, s),
        ContestTransition::Negotiate(NegotiateTransition::Stop) => stop(tx, g, id, s.contest_id),
        _ => Err(invalid()),
    }
}
// Negotiate allocates and commits its barriers inside advance, with no caller-supplied projections.
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
pub fn barrier(_tx: &Transaction<'_>, _g: &mut CommitGroup, _c: BarrierCommit) -> Result<()> {
    Err(invalid())
}
