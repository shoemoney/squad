//! Transactional trace admission, assignments, barriers and human continuation.
use super::envelope;
use crate::{orchestrator::trace as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::trace::*;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup, trace::Run};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid trace transition",
    )
}
pub fn load_state(db: &Connection, id: ContestId) -> Result<TraceState> {
    let s: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='trace'",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
fn push(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    p: EnvelopePayload,
) -> Result<Cursor> {
    let contest = match &p {
        EnvelopePayload::TraceSetup(s) => s.contest_id,
        EnvelopePayload::ContestStep(s) => s.contest_id,
        EnvelopePayload::ContestReport(s) => s.contest_id,
        EnvelopePayload::ContestPreflight(s) => s.contest_id,
        EnvelopePayload::ContestPreflightResult(s) => s.contest_id,
        _ => return Err(invalid()),
    };
    let seq = g.push(tx, envelope(id, Actor::System, vec![Recipient::Me], p))?;
    squad_store::contests::associate(tx, contest, ContestKind::Trace, seq)?;
    Ok(seq)
}
pub fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &TraceState,
    expected: Option<u64>,
) -> Result<()> {
    let mut canonical = s.clone();
    let mut q=tx.prepare("SELECT e.id FROM contest_artifacts a JOIN events e ON e.seq=a.event_seq WHERE a.contest_id=? ORDER BY a.event_seq")?;
    canonical.artifact_event_ids = q
        .query_map([s.contest_id.to_string()], |r| r.get::<_, String>(0))?
        .map(|raw| raw?.parse().map_err(|_| rusqlite::Error::InvalidQuery))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let s = &canonical;
    s.validate().map_err(|_| invalid())?;
    if let Some(v) = expected {
        let current: i64 = tx.query_row(
            "SELECT state_version FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )?;
        if current as u64 != v {
            return Err(invalid());
        }
    }
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::TraceState(s.clone()),
        ),
    )?;
    if expected.is_none() {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'trace',(SELECT epoch FROM room_meta),?,?,1,?)",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,seq.get()])?;
    } else {
        tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=?,expires_at=CASE WHEN ? THEN NULL ELSE expires_at END WHERE contest_id=?",params![s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get(),false,s.contest_id.to_string()])?;
    }
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Trace, seq)?;
    if s.state.is_terminal() {
        tx.execute(
            "UPDATE contest_human_steps SET closed=1 WHERE contest_id=?",
            [s.contest_id.to_string()],
        )?;
        // /stop's shared transaction emits WorkState cancellations and releases
        // each unstarted work reservation. Keep those rows runnable until that
        // handler sees them; pre-cancelling here would bypass both operations.
        if s.state != TraceStatus::Stopped {
            tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[s.contest_id.to_string()])?;
        }
        squad_store::history::release(tx, s.contest_id)?;
        let rows = {
            let mut q=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
            q.query_map([s.contest_id.to_string()], |r| {
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
    }
    Ok(())
}
pub fn steps(
    participants: &[MemberNick],
    question: &str,
    sources: &[trace::TraceSource],
    templates: &[u64],
    words: u32,
) -> Vec<super::bounds::Step> {
    use super::bounds::*;
    let m = participants.len() as u64;
    let packet = sources
        .iter()
        .map(|s| json_text(&s.text) + 128)
        .sum::<u64>();
    let base = json_text(question) + packet + 512;
    let make = |kind: WorkKind, target: String, t: usize, data: u64, attempts| Step {
        schema: schema_bytes(trace::output_schema(&kind).expect("schema")),
        kind,
        target,
        alternatives: vec![],
        template: templates[t],
        data,
        visible: if t == 0 || t == 2 { 2048 } else { 4096 },
        calls: 1,
        attempts,
    };
    let mut out = vec![];
    for n in participants {
        out.push(make(WorkKind::TraceExtract, n.to_string(), 0, base, 2));
        out.push(make(
            WorkKind::TraceVerify,
            n.to_string(),
            2,
            base + project(artifact(4096)) + m * project(limited(4096, 900)),
            2,
        ));
    }
    out.push(make(
        WorkKind::TraceMerge,
        "judge".into(),
        1,
        blinded(question) + packet + m * project(limited(4096, 900)) + 512,
        3,
    ));
    out.push(make(
        WorkKind::TraceAssemble,
        "judge".into(),
        3,
        json_text(question)
            + project(artifact(4096))
            + m * project(limited(4096, 900))
            + m * project(limited(2048, words))
            + 1024,
        3,
    ));
    out
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::Trace {
        participants,
        question,
        sources: packet,
        input: conversation,
    } = parsed
    else {
        return Err(invalid());
    };
    let busy: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get(0),
    )?;
    if busy {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let config = a.config.ok_or_else(invalid)?;
    let mut sources = vec![];
    let mut source_events = vec![];
    for (i, s) in packet.iter().enumerate() {
        let source = match s {
            parser::TraceSource::Pasted { text } => {
                parser::PlanSource::Pasted { text: text.clone() }
            }
            parser::TraceSource::Quoted { event_id } => parser::PlanSource::Quoted {
                event_id: *event_id,
            },
        };
        let (text, events) = super::census::source(tx, &source)?;
        sources.push(trace::TraceSource {
            source_id: format!("S{}", i + 1),
            text,
        });
        source_events.extend(events);
    }
    if question.trim().is_empty() || participants.len() < 3 {
        return Err(invalid());
    }
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let names = [
        "trace.extract",
        "trace.merge",
        "trace.verify",
        "trace.assemble",
    ];
    let mut frozen = serde_json::Map::new();
    let mut refs = BTreeMap::new();
    for name in names {
        let (system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        refs.insert(
            name.into(),
            PromptRef {
                pack_version: pack.version().parse().map_err(|_| invalid())?,
                template_id: name.parse().expect("template"),
                template_hash: squad_store::commit::digest(&(system.clone(), user.clone()))?
                    .parse()
                    .map_err(|_| invalid())?,
            },
        );
        frozen.insert(
            name.into(),
            serde_json::json!({"system":system,"user":user}),
        );
    }
    let mut models = a.bindings.clone();
    models.get_mut("judge").ok_or_else(invalid)?.effort =
        constants::NEW_MODE_JUDGE_REASONING_EFFORT;
    let setup = TraceSetup {
        question: question.clone(),
        sources,
        participants: participants.clone(),
        room_words: config.prompts.limits.room_words,
        plan_words: config.prompts.limits.plan_words,
        models,
        prompt_refs: refs,
    };
    setup.validate().map_err(|_| invalid())?;
    let templates = names
        .into_iter()
        .map(|n| super::bounds::template(&pack, n))
        .collect::<Result<Vec<_>>>()?;
    let priced = super::bounds::price(
        tx,
        &setup.models,
        &steps(
            participants,
            &setup.question,
            &setup.sources,
            &templates,
            setup.room_words,
        ),
    )?;
    let contest = ContestId::new();
    let state = TraceState {
        contest_id: contest,
        state_version: 1,
        state: TraceStatus::Preflight,
        round: 0,
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &state, None)?;
    squad_store::trace::insert(tx, contest, &setup)?;
    let run = squad_store::trace::run(tx, contest)?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "trace.seed",
        &serde_json::json!({"seed":run.seed}),
    )?;
    let tokens = config
        .agents
        .iter()
        .flat_map(|a| [a.nick.to_string(), a.model.clone(), a.persona.clone()])
        .chain(std::iter::once(config.judge.model.clone()))
        .chain(
            setup
                .models
                .values()
                .map(|binding| binding.model.to_string()),
        )
        .collect::<Vec<_>>();
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "trace.prompts",
        &serde_json::json!({"repairTemplate":pack.template("mode.repair").map_err(|_|invalid())?,"compositions":frozen,"tokens":tokens,"personas":config.agents.iter().map(|a|(a.nick.to_string(),a.persona.clone())).collect::<BTreeMap<_,_>>(),"userName":config.user.name,"roster":participants}),
    )?;
    push(
        tx,
        g,
        id,
        EnvelopePayload::TraceSetup(ContestSetup {
            contest_id: contest,
            kind: ContestKind::Trace,
            script_version: "trace-v1".parse().expect("version"),
            participants: participants.clone(),
            source_event_ids: source_events,
            settings: serde_json::json!({"question":setup.question,"sources":setup.sources}),
        }),
    )?;
    super::variables::apply(tx, g, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    let (calls, attempts) = script::call_bounds(participants.len() as u32);
    settings["reservedAttempts"] = attempts.into();
    settings["memberCalls"] = (2 * participants.len() as u32).into();
    settings["judgeCalls"] = 2u32.into();
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Trace,
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
            model_calls: calls,
        },
        participants: participants.clone(),
        confirmation_required: priced.estimated_usd > a.confirmation_threshold,
        warning: None,
        settings,
    };
    let seq = push(tx, g, id, EnvelopePayload::ContestPreflight(plan.clone()))?;
    tx.execute("UPDATE contests SET plan_id=?,plan_json=?,expires_at=?,pending_request_json=? WHERE contest_id=?",params![plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),serde_json::json!({"input":input,"preconfirmed":a.preconfirmed,"max":a.daily_max,"threshold":a.confirmation_threshold}).to_string(),contest.to_string()])?;
    if priced.reserved_micros
        > a.daily_max
            .saturating_sub(daily.spent_micros + daily.reserved_micros)
    {
        let mut s = state;
        s.state = TraceStatus::Failed;
        s.state_version += 1;
        s.terminal_reason = Some("daily_max_reached".parse().expect("reason"));
        save(tx, g, id, &s, Some(1))?;
        return Ok(RequestResult::Rejected {
            request_id: id,
            error: super::daily_cap_error(tx, &day, priced.reserved_micros, a.daily_max),
        });
    }
    if a.preconfirmed || !plan.confirmation_required {
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
    if state.state != TraceStatus::Preflight || plan.plan_id != plan_id {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let version = state.state_version;
    state.state_version += 1;
    if cancel || expired {
        state.state = TraceStatus::Stopped;
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
        state.state = TraceStatus::WaitingForParticipants;
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
fn material(db: &Connection, work: &Work) -> Result<serde_json::Value> {
    let rows = squad_store::history::read(db, &work.context_id, work.history_through)?;
    serde_json::from_str(&rows.last().ok_or_else(invalid)?.content).map_err(Into::into)
}
pub fn accepted_work(
    db: &Connection,
    work: &Work,
    artifact: &ContestArtifact,
    seq: Cursor,
) -> Result<()> {
    let state = load_state(db, artifact.contest_id)?;
    if state.state.is_terminal()
        || state.step_id != work.step_id
        || artifact.round != Some(state.round)
        || artifact.audience != Some(ArtifactAudience::Human)
        || trace::composition(&work.kind) != Some(artifact.artifact_kind.as_str())
    {
        return Err(invalid());
    }
    let data = material(db, work)?;
    let output = artifact.body.as_ref().ok_or_else(invalid)?;
    script::validate_output(&work.kind, &data["data"], output).map_err(|_| invalid())?;
    if trace::prose_words(output) > data["wordLimit"].as_u64().ok_or_else(invalid)? as usize {
        return Err(invalid());
    }
    squad_store::contest_steps::artifact(db, artifact, seq)
}

pub fn human_answer(
    _db: &Connection,
    _pending: &parser::PendingHuman,
    _complete: bool,
) -> Result<()> {
    Ok(())
}
pub fn recover(db: &Connection, contest: ContestId) -> Result<()> {
    load_state(db, contest)?.validate().map_err(|_| invalid())
}
pub fn stop(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
) -> Result<()> {
    let mut s = load_state(tx, contest)?;
    finish(tx, g, id, &mut s, "stopped", false)
}
pub fn own_artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &TraceState,
    kind: &str,
    body: serde_json::Value,
) -> Result<()> {
    let step = s.step_id.ok_or_else(invalid)?;
    let a = ContestArtifact {
        contest_id: s.contest_id,
        step_id: step,
        artifact_kind: kind.parse().map_err(|_| invalid())?,
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Trace),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
    };
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestArtifact(a.clone()),
        ),
    )?;
    squad_store::contest_steps::artifact(tx, &a, seq)?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Trace, seq)
}
pub fn finish(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut TraceState,
    reason: &str,
    failed: bool,
) -> Result<()> {
    let setup = squad_store::trace::setup(tx, s.contest_id)?;
    let run = squad_store::trace::run(tx, s.contest_id)?;
    if s.step_id.is_none() {
        let step = ContestStep {
            contest_id: s.contest_id,
            step_id: StepId::new(),
            ordinal: 5,
            script_version: "trace-v1".parse().expect("version"),
            move_name: ContestMove::Report,
            inputs: vec![],
            assignments: vec![],
        };
        let seq = push(tx, g, id, EnvelopePayload::ContestStep(step.clone()))?;
        squad_store::contest_steps::insert(tx, &step, seq)?;
        s.step_id = Some(step.step_id);
    }
    let report = TraceReport {
        question: setup.question,
        sources: setup.sources,
        extractions: run.extractions,
        provenance: run.provenance,
        quote_checks: run.quote_checks,
        claims: run.claims,
        assignments: run.assignments,
        verifications: run.verifications,
        ledger: run.ledger,
        brief: run.brief,
        skipped: run.skipped,
        reason: reason.into(),
    };
    own_artifact(tx, g, id, s, "trace.report", serde_json::to_value(report)?)?;
    let v = s.state_version;
    s.state_version += 1;
    s.state = if failed {
        TraceStatus::Failed
    } else if reason == "stopped" {
        TraceStatus::Stopped
    } else {
        TraceStatus::Complete
    };
    s.terminal_reason = Some(reason.parse().map_err(|_| invalid())?);
    save(tx, g, id, s, Some(v))
}
pub(crate) fn drive_move(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut s: TraceState,
) -> Result<RequestResult> {
    if load_state(tx, s.contest_id)? != s {
        return Err(invalid());
    }
    let setup = squad_store::trace::setup(tx, s.contest_id)?;
    let mut run = squad_store::trace::run(tx, s.contest_id)?;
    if s.state == TraceStatus::Preflight {
        let raw: String = tx.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )?;
        let p: ContestPreflight = serde_json::from_str(&raw)?;
        if g.accepted_at >= p.expires_at {
            return resolve(tx, g, id, s.contest_id, p.plan_id, true);
        }
        return Ok(g.accepted(id));
    }
    if s.state == TraceStatus::WaitingForParticipants {
        for n in &setup.participants {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL AND json_type(scheduling_meta,'$.openModeDormant') IS NULL)",[n.as_str()],|r|r.get(0))?;
            if pending {
                return Ok(g.accepted(id));
            }
        }
        let v = s.state_version;
        s.state = TraceStatus::Extracting;
        s.step_id = None;
        s.state_version += 1;
        save(tx, g, id, &s, Some(v))?;
        return Ok(g.accepted(id));
    }
    if s.state == TraceStatus::Reporting {
        finish(tx, g, id, &mut s, "complete", false)?;
        return Ok(g.accepted(id));
    }
    if s.step_id.is_none() {
        allocate(tx, g, id, &s, &setup, &mut run)?;
        return Ok(g.accepted(id));
    }
    let step = s.step_id.ok_or_else(invalid)?;
    let rows = {
        let mut q = tx.prepare("SELECT work_id FROM contest_assignments WHERE step_id=?")?;
        q.query_map([step.to_string()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut artifacts = vec![];
    let mut skipped = vec![];
    for raw in rows {
        let work_id: WorkId = raw.parse().map_err(|_| invalid())?;
        let mut record = squad_store::work::record(tx, work_id)?.ok_or_else(invalid)?;
        let work = squad_store::work::template(tx, work_id)?
            .ok_or_else(invalid)?
            .0;
        let limit = if matches!(work.kind, WorkKind::TraceMerge | WorkKind::TraceAssemble) {
            3
        } else {
            2
        };
        if record.status == WorkStatus::Queued && record.attempt >= limit {
            let seq = g.push(
                tx,
                envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::WorkState(WorkStateEvent {
                        work_id,
                        trigger_event_id: work.trigger_event_id,
                        target: work.target.clone(),
                        context_id: work.context_id.clone(),
                        attempt: record.attempt,
                        status: WorkStatus::Failed,
                        error: Some(super::error(
                            ErrorCode::PROVIDER_TIMEOUT,
                            "Trace attempt interrupted and exhausted",
                        )),
                        debate_id: None,
                        case_id: work.case_id,
                        contest_id: work.contest_id,
                    }),
                ),
            )?;
            tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1,completed_event_seq=? WHERE work_id=?",params![seq.get(),work_id.to_string()])?;
            squad_store::accounting::release_unstarted_work(tx, work_id)?;
            record.status = WorkStatus::Failed;
        }
        if matches!(record.status, WorkStatus::Queued | WorkStatus::Running) {
            return Ok(g.accepted(id));
        }
        if record.status == WorkStatus::Completed {
            let seq: i64 = tx.query_row(
                "SELECT completed_event_seq FROM work WHERE work_id=?",
                [raw],
                |r| r.get(0),
            )?;
            let e = squad_store::events::at(tx, seq)?;
            let EnvelopePayload::ContestArtifact(a) = e.content else {
                return Err(invalid());
            };
            artifacts.push((work, a, e.base.id));
        } else {
            if work.target == Target::Judge {
                finish(tx, g, id, &mut s, "judge_failed", true)?;
                return Ok(g.accepted(id));
            }
            squad_store::contest_steps::settle_if_pending(
                tx,
                step,
                &work.target,
                Some(&super::error(
                    ErrorCode::MODE_INVALID_OUTPUT,
                    "Member skipped after exhausted attempts",
                )),
            )?;
            skipped.push(work);
        }
    }
    let seq = push(
        tx,
        g,
        id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: s.contest_id,
            step_id: step,
            artifact_event_ids: artifacts.iter().map(|a| a.2).collect(),
            skipped_work_ids: skipped.iter().map(|w| w.work_id).collect(),
            data: serde_json::json!({"round":s.round,"step":s.state}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    s.artifact_event_ids.extend(artifacts.iter().map(|a| a.2));
    for w in skipped {
        run.skipped.push(format!("{}: {:?}", w.target, s.state));
    }
    match s.state {
        TraceStatus::Extracting => {
            let mut labelled = artifacts
                .into_iter()
                .map(|a| {
                    (
                        squad_store::commit::digest(&("trace.labels", &run.seed, &a.0.target))
                            .expect("label digest"),
                        a,
                    )
                })
                .collect::<Vec<_>>();
            labelled.sort_by(|a, b| a.0.cmp(&b.0));
            for (i, (_, (w, a, _))) in labelled.into_iter().enumerate() {
                let Target::Member(n) = w.target else {
                    return Err(invalid());
                };
                let label = format!("Reader {}", i + 1);
                let extraction: TraceExtraction =
                    serde_json::from_value(a.body.ok_or_else(invalid)?)?;
                run.quote_checks
                    .extend(script::check_quotes(&label, &extraction, &setup.sources));
                run.provenance.insert(label.clone(), n);
                run.extractions.insert(label, extraction);
            }
            squad_store::trace::save(tx, s.contest_id, &run)?;
            own_artifact(
                tx,
                g,
                id,
                &s,
                "trace.quote_checks",
                serde_json::json!({"checks":run.quote_checks}),
            )?;
            if run.extractions.len() < 2 {
                finish(tx, g, id, &mut s, "insufficient_extractions", true)?;
                return Ok(g.accepted(id));
            }
            squad_store::contest_steps::private_map(
                tx,
                s.contest_id,
                "trace.readers",
                &serde_json::json!({"labels":run.provenance}),
            )?;
            s.state = TraceStatus::Merging;
        }
        TraceStatus::Merging => {
            run.claims = serde_json::from_value(
                artifacts
                    .first()
                    .ok_or_else(invalid)?
                    .1
                    .body
                    .clone()
                    .ok_or_else(invalid)?,
            )?;
            run.assignments = script::assign_verifiers(
                &run.claims,
                &run.provenance,
                &setup.participants,
                &run.seed,
            );
            squad_store::contest_steps::private_map(
                tx,
                s.contest_id,
                "trace.verifiers",
                &serde_json::json!({"assignments":run.assignments}),
            )?;
            squad_store::trace::save(tx, s.contest_id, &run)?;
            if run.claims.claims.is_empty() {
                run.brief = Some(TraceBrief {
                    sections: vec![],
                    disputed: vec![],
                    gaps: vec!["No canonical claims extracted.".into()],
                });
                squad_store::trace::save(tx, s.contest_id, &run)?;
                finish(tx, g, id, &mut s, "empty_claims", false)?;
                return Ok(g.accepted(id));
            }
            s.state = TraceStatus::Verifying;
        }
        TraceStatus::Verifying => {
            for (w, a, _) in artifacts {
                let Target::Member(n) = w.target else {
                    return Err(invalid());
                };
                run.verifications
                    .insert(n, serde_json::from_value(a.body.ok_or_else(invalid)?)?);
            }
            run.ledger = script::ledger(&run.claims, &run.quote_checks, &run.verifications);
            squad_store::trace::save(tx, s.contest_id, &run)?;
            own_artifact(
                tx,
                g,
                id,
                &s,
                "trace.ledger",
                serde_json::to_value(&run.ledger)?,
            )?;
            s.state = TraceStatus::Assembling;
        }
        TraceStatus::Assembling => {
            run.brief = Some(serde_json::from_value(
                artifacts
                    .first()
                    .ok_or_else(invalid)?
                    .1
                    .body
                    .clone()
                    .ok_or_else(invalid)?,
            )?);
            squad_store::trace::save(tx, s.contest_id, &run)?;
            s.state = TraceStatus::Reporting;
        }
        _ => return Err(invalid()),
    }
    squad_store::trace::save(tx, s.contest_id, &run)?;
    let v = s.state_version;
    s.state_version += 1;
    s.step_id = None;
    save(tx, g, id, &s, Some(v))?;
    Ok(g.accepted(id))
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &TraceState,
    setup: &TraceSetup,
    run: &mut Run,
) -> Result<()> {
    let kind = match s.state {
        TraceStatus::Extracting => WorkKind::TraceExtract,
        TraceStatus::Merging => WorkKind::TraceMerge,
        TraceStatus::Verifying => WorkKind::TraceVerify,
        TraceStatus::Assembling => WorkKind::TraceAssemble,
        _ => return Err(invalid()),
    };
    let judge = matches!(kind, WorkKind::TraceMerge | WorkKind::TraceAssemble);
    let name = trace::composition(&kind).ok_or_else(invalid)?;
    let words = if kind == WorkKind::TraceExtract {
        900
    } else if judge {
        setup.plan_words
    } else {
        setup.room_words
    };
    let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='trace.prompts'",[s.contest_id.to_string()],|r|r.get(0))?;
    let frozen: serde_json::Value = serde_json::from_str(&frozen)?;
    let tokens: Vec<String> = serde_json::from_value(frozen["tokens"].clone())?;
    let step_id = StepId::new();
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
    let mut items = vec![];
    let mut assignments = vec![];
    for target in targets {
        let data = script::input(&kind, setup, run, &target)?;
        let data = if kind == WorkKind::TraceMerge {
            script::blind_input(data, &tokens)
        } else {
            data
        };
        let context = if judge {
            ContextId(format!("judge:{}", run.case_id))
        } else {
            ContextId(format!("contest:{}:{}:{}", s.contest_id, step_id, target))
        };
        let binding = setup
            .models
            .get(&target.to_string())
            .ok_or_else(invalid)?
            .clone();
        let model: (i64,Option<i64>,String,String)=tx.query_row("SELECT context_length,max_output,pricing_json,supported_params_json FROM catalog WHERE slug=?",[binding.model.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let model = squad_agents::resolver::Model {
            id: binding.model.to_string(),
            context_length: model.0 as u32,
            output_limit: model.1.unwrap_or(8192) as u32,
            pricing: serde_json::from_str(&model.2)?,
            supported_parameters: serde_json::from_str(&model.3)?,
            reasoning_efforts: vec![],
        };
        let mut values = BTreeMap::from([
            ("mode_data_json".into(), data.to_string()),
            (
                "mode_schema_json".into(),
                trace::output_schema(&kind).ok_or_else(invalid)?.to_string(),
            ),
            ("nick".into(), target.to_string()),
            ("model_slug".into(), binding.model.to_string()),
            (
                "judge_model_slug".into(),
                if kind == WorkKind::TraceMerge {
                    "anonymous judge".into()
                } else {
                    binding.model.to_string()
                },
            ),
            ("room_word_limit".into(), words.to_string()),
            (
                "member_nicks_json".into(),
                if judge {
                    "[]".into()
                } else {
                    frozen["roster"].to_string()
                },
            ),
            (
                "user_name_json".into(),
                if judge {
                    "\"the user\"".into()
                } else {
                    frozen["userName"].to_string()
                },
            ),
            (
                "persona_json".into(),
                if judge {
                    "\"\"".into()
                } else {
                    serde_json::json!(
                        frozen["personas"][target.to_string()]
                            .as_str()
                            .unwrap_or("")
                    )
                    .to_string()
                },
            ),
        ]);
        values.insert(
            "mode_data_json".into(),
            serde_json::json!({"input":script::provider_projection(&kind,&data),"proseWordLimit":words}).to_string(),
        );
        let source = &frozen["compositions"][name];
        let render = |key: &str| -> Result<String> {
            crate::prompts::inputs_census::render_source(
                source[key].as_str().ok_or_else(invalid)?,
                &values,
            )
        };
        let system = render("system")?;
        let bundle = serde_json::json!({"repairTemplate":frozen["repairTemplate"],"round":s.round,"data":data,"wordLimit":words,"model":model,"rendered":{"system":system,"user":render("user")?,"provider_user_name":"squad"}});
        let item = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: kind.clone(),
            target: target.clone(),
            context_id: context.clone(),
            trigger_event_id: id,
            history_through: HistoryPosition::new(1).expect("position"),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: judge.then_some(run.case_id),
            prompt_ref: Some(setup.prompt_refs[name].clone()),
            max_output_tokens: Some(
                if matches!(kind, WorkKind::TraceExtract | WorkKind::TraceVerify) {
                    2048
                } else {
                    4096
                },
            ),
            contest_id: Some(s.contest_id),
            step_id: Some(step_id),
        };
        assignments.push(ContestAssignment {
            target,
            context_id: context,
            payload_refs: vec![],
            work_id: Some(item.work_id),
        });
        items.push((item, binding, bundle));
    }
    let step = ContestStep {
        contest_id: s.contest_id,
        step_id,
        ordinal: match kind {
            WorkKind::TraceExtract => 1,
            WorkKind::TraceMerge => 2,
            WorkKind::TraceVerify => 3,
            _ => 4,
        },
        script_version: "trace-v1".parse().expect("version"),
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
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Trace, seq)?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (mut item, binding, bundle) in items {
        if judge {
            tx.execute("INSERT OR IGNORE INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![run.case_id.to_string(),item.context_id.0,s.contest_id.to_string(),binding.model.as_str(),bundle["data"].to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: item.context_id.clone(),
            origin_event_id: id,
            projection_key: "trace.step".parse().expect("key"),
            role: HistoryRole::User,
            name: None,
            content: bundle.to_string(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        item.history_through =
            squad_store::history::project(tx, &projection)?.ok_or_else(invalid)?;
        squad_store::work::insert(tx, &item, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step_id, &item)?;
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
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let s = load_state(tx, submission.contest_id)?;
    if submission.kind != ContestKind::Trace
        || s.state_version != submission.expected_state_version
        || !matches!(
            submission.transition,
            ContestTransition::Trace(TraceTransition::Advance)
        )
        || s.state.is_terminal()
    {
        return Err(invalid());
    }
    drive_move(tx, g, id, s)?;
    Ok(())
}
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
pub fn barrier(_tx: &Transaction<'_>, _g: &mut CommitGroup, _c: BarrierCommit) -> Result<()> {
    Err(invalid())
}
