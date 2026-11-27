//! Transactional debugging admission, assignments, barriers and human continuation.
use super::envelope;
use crate::{orchestrator::debugging as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::debugging::*;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup, debugging::Run};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid debugging transition",
    )
}
pub fn load_state(db: &Connection, id: ContestId) -> Result<DebuggingState> {
    let s: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='debug'",
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
        EnvelopePayload::DebuggingSetup(s) => s.contest_id,
        EnvelopePayload::ContestStep(s) => s.contest_id,
        EnvelopePayload::ContestReport(s) => s.contest_id,
        EnvelopePayload::ContestPreflight(s) => s.contest_id,
        EnvelopePayload::ContestPreflightResult(s) => s.contest_id,
        _ => return Err(invalid()),
    };
    let seq = g.push(tx, envelope(id, Actor::System, vec![Recipient::Me], p))?;
    squad_store::contests::associate(tx, contest, ContestKind::Debugging, seq)?;
    Ok(seq)
}
pub fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &DebuggingState,
    expected: Option<u64>,
) -> Result<()> {
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
            EnvelopePayload::DebuggingState(s.clone()),
        ),
    )?;
    if expected.is_none() {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'debug',(SELECT epoch FROM room_meta),?,?,1,?)",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,seq.get()])?;
    } else {
        tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=?,expires_at=CASE WHEN ? THEN NULL ELSE expires_at END WHERE contest_id=?",params![s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get(),s.state==DebuggingStatus::AwaitingHuman,s.contest_id.to_string()])?;
    }
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Debugging, seq)?;
    if s.state.is_terminal() {
        tx.execute(
            "UPDATE contest_human_steps SET closed=1 WHERE contest_id=?",
            [s.contest_id.to_string()],
        )?;
        // /stop's shared transaction emits WorkState cancellations and releases
        // each unstarted work reservation. Keep those rows runnable until that
        // handler sees them; pre-cancelling here would bypass both operations.
        if s.state != DebuggingStatus::Stopped {
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
    rounds: u32,
    brief: &str,
    limits: &str,
    templates: &[u64],
    words: u32,
) -> Vec<super::bounds::Step> {
    use super::bounds::*;
    let step =
        |kind: WorkKind, target: String, i: usize, data: u64, calls: u32, judge: bool| Step {
            schema: schema_bytes(debugging::output_schema(&kind).expect("schema")),
            kind,
            target,
            alternatives: vec![],
            template: templates[i],
            data,
            visible: if judge { 4096 } else { 2048 },
            calls,
            attempts: if judge { 3 } else { 2 },
        };
    let m = participants.len() as u64;
    let base = blinded(brief) + json_text(limits) + 1024;
    let ledger = u64::from(rounds) * limited(2048, 400);
    let mut out = vec![];
    for n in participants {
        out.push(step(
            WorkKind::DebuggingHypothesize,
            n.to_string(),
            0,
            base,
            1,
            false,
        ));
        out.push(step(
            WorkKind::DebuggingTest,
            n.to_string(),
            2,
            base + artifact(4096) + ledger,
            rounds,
            false,
        ));
    }
    out.push(step(
        WorkKind::DebuggingMerge,
        "judge".into(),
        1,
        base + m * project(limited(2048, words)),
        1,
        true,
    ));
    out.push(step(
        WorkKind::DebuggingSelect,
        "judge".into(),
        3,
        base + artifact(4096) + ledger + m * project(limited(2048, words)),
        rounds,
        true,
    ));
    out.push(step(
        WorkKind::DebuggingReport,
        "judge".into(),
        4,
        base + artifact(4096) + ledger,
        1,
        true,
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
    let ParsedInput::Debugging {
        participants,
        brief,
        limits,
        rounds,
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
    let sources = vec![];
    if brief.trim().is_empty() || participants.len() < 2 {
        return Err(invalid());
    }
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let names = [
        "debug.hypothesize",
        "debug.merge",
        "debug.test",
        "debug.select",
        "debug.report",
    ];
    let mut frozen = serde_json::Map::new();
    let mut refs = BTreeMap::new();
    for name in names {
        let (mut system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        if matches!(name, "debug.merge" | "debug.select") {
            // The scaffold prepends a blind prefix but retains the identity header.
            // Remove that known header before freezing, including its literal @fable.
            let header = pack
                .template("judge.system")
                .map_err(|_| invalid())?
                .split_once("You cannot see")
                .ok_or_else(invalid)?
                .0;
            if system.matches(header).count() != 1 {
                return Err(invalid());
            }
            system = system.replacen(header, "", 1);
        }
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
    let setup = DebuggingSetup {
        brief: brief.clone(),
        limits: limits.clone().unwrap_or_default(),
        participants: participants.clone(),
        rounds: *rounds,
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
            u32::from(*rounds),
            &setup.brief,
            &setup.limits,
            &templates,
            setup.room_words,
        ),
    )?;
    let contest = ContestId::new();
    let state = DebuggingState {
        contest_id: contest,
        state_version: 1,
        state: DebuggingStatus::Preflight,
        round: 1,
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &state, None)?;
    squad_store::debugging::insert(tx, contest, &setup)?;
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
        "debug.prompts",
        &serde_json::json!({"repairTemplate":pack.template("mode.repair").map_err(|_|invalid())?,"compositions":frozen,"tokens":tokens,"personas":config.agents.iter().map(|a|(a.nick.to_string(),a.persona.clone())).collect::<BTreeMap<_,_>>(),"userName":config.user.name,"roster":participants}),
    )?;
    push(
        tx,
        g,
        id,
        EnvelopePayload::DebuggingSetup(ContestSetup {
            contest_id: contest,
            kind: ContestKind::Debugging,
            script_version: "debug-v1".parse().expect("version"),
            participants: participants.clone(),
            source_event_ids: sources,
            settings: serde_json::json!({"brief":setup.brief,"limits":setup.limits,"rounds":rounds}),
        }),
    )?;
    super::variables::apply(tx, g, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    let (calls, attempts) = script::call_bounds(participants.len() as u32, u32::from(*rounds));
    settings["reservedAttempts"] = attempts.into();
    settings["memberCalls"] = ((u32::from(*rounds) + 1) * participants.len() as u32).into();
    settings["judgeCalls"] = (u32::from(*rounds) + 2).into();
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Debugging,
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
        s.state = DebuggingStatus::Failed;
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
    if state.state != DebuggingStatus::Preflight || plan.plan_id != plan_id {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let version = state.state_version;
    state.state_version += 1;
    if cancel || expired {
        state.state = DebuggingStatus::Stopped;
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
        state.state = DebuggingStatus::WaitingForParticipants;
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
        || debugging::composition(&work.kind) != Some(artifact.artifact_kind.as_str())
    {
        return Err(invalid());
    }
    let data = material(db, work)?;
    let output = artifact.body.as_ref().ok_or_else(invalid)?;
    script::validate_output(&work.kind, &data["data"], output).map_err(|_| invalid())?;
    if debugging::prose_words(output) > data["wordLimit"].as_u64().ok_or_else(invalid)? as usize {
        return Err(invalid());
    }
    squad_store::contest_steps::artifact(db, artifact, seq)
}

pub fn human_answer(db: &Connection, pending: &parser::PendingHuman, complete: bool) -> Result<()> {
    if !complete {
        return Ok(());
    }
    let mut run = squad_store::debugging::run(db, pending.contest_id)?;
    let round = run.rounds.last_mut().ok_or_else(invalid)?;
    let raw = pending
        .answers
        .get(&1)
        .map(String::as_str)
        .unwrap_or("unknown");
    let (outcome, evidence) = raw.split_once(char::is_whitespace).unwrap_or((raw, ""));
    let outcome = if outcome.eq_ignore_ascii_case("unknown") {
        "unknown".to_string()
    } else if outcome.eq_ignore_ascii_case("other") {
        "other".to_string()
    } else {
        outcome.to_uppercase()
    };
    round.result = Some(DebuggingResult {
        round: round.round,
        unexpected: outcome == "other",
        outcome,
        evidence: evidence.trim().into(),
    });
    run.resume = DebuggingStatus::Eliminating;
    squad_store::debugging::save(db, pending.contest_id, &run)
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
pub fn finish(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut DebuggingState,
    reason: &str,
    failed: bool,
) -> Result<()> {
    let run = squad_store::debugging::run(tx, s.contest_id)?;
    let step = if let Some(step) = s.step_id {
        step
    } else {
        let step = ContestStep {
            contest_id: s.contest_id,
            step_id: StepId::new(),
            ordinal: tx.query_row(
                "SELECT COALESCE(MAX(ordinal),0)+1 FROM contest_steps WHERE contest_id=?",
                [s.contest_id.to_string()],
                |r| r.get(0),
            )?,
            script_version: "debug-v1".parse().expect("version"),
            move_name: ContestMove::Report,
            inputs: vec![],
            assignments: vec![],
        };
        let seq = push(tx, g, id, EnvelopePayload::ContestStep(step.clone()))?;
        squad_store::contest_steps::insert(tx, &step, seq)?;
        step.step_id
    };
    let mut q=tx.prepare("SELECT payload FROM events WHERE kind='contest.human_answered' AND json_extract(payload,'$.contestId')=? ORDER BY seq")?;
    let raw_answers = q
        .query_map([s.contest_id.to_string()], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|raw| {
            serde_json::from_str::<ContestHumanAnswered>(&raw)
                .ok()
                .map(|a| a.text.unwrap_or_else(|| "/continue".into()))
        })
        .collect();
    let setup = squad_store::debugging::setup(tx, s.contest_id)?;
    let report = DebuggingReport {
        brief: setup.brief,
        limits: setup.limits,
        patch_allowed: script::patch_allowed(&run.differential),
        differential: run.differential,
        tables: run.tables,
        provenance: run.provenance,
        rounds: run.rounds,
        quotes: run.quotes,
        skipped: run.skipped,
        raw_answers,
        reason: reason.into(),
        narrative: run.narrative,
    };
    let artifact = ContestArtifact {
        contest_id: s.contest_id,
        step_id: step,
        artifact_kind: "debug.report".parse().expect("kind"),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Debugging),
        round: Some(s.round),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::to_value(report)?),
    };
    let e = envelope(
        id,
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestArtifact(artifact.clone()),
    );
    let event = e.base.id;
    let seq = g.push(tx, e)?;
    squad_store::contest_steps::artifact(tx, &artifact, seq)?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Debugging, seq)?;
    let v = s.state_version;
    s.state_version += 1;
    s.state = if failed {
        DebuggingStatus::Failed
    } else if reason == "stopped" {
        DebuggingStatus::Stopped
    } else {
        DebuggingStatus::Complete
    };
    s.terminal_reason = Some(reason.parse().map_err(|_| invalid())?);
    s.artifact_event_ids.push(event);
    save(tx, g, id, s, Some(v))
}
pub(crate) fn drive_move(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut s: DebuggingState,
) -> Result<RequestResult> {
    if load_state(tx, s.contest_id)? != s {
        return Err(invalid());
    }
    let setup = squad_store::debugging::setup(tx, s.contest_id)?;
    let mut run = squad_store::debugging::run(tx, s.contest_id)?;
    if s.state == DebuggingStatus::Preflight {
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
    if s.state == DebuggingStatus::WaitingForParticipants {
        for n in &setup.participants {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get(0))?;
            if pending {
                return Ok(g.accepted(id));
            }
        }
        let v = s.state_version;
        s.state = run.resume;
        s.step_id = None;
        s.state_version += 1;
        save(tx, g, id, &s, Some(v))?;
        return Ok(g.accepted(id));
    }
    if s.state == DebuggingStatus::Eliminating {
        let round = run.rounds.last_mut().ok_or_else(invalid)?;
        let elimination = script::eliminate(
            &run.differential,
            round.selection.as_ref().ok_or_else(invalid)?,
            round.result.as_ref().ok_or_else(invalid)?,
        );
        run.differential = elimination.hypotheses.clone();
        round.elimination = Some(elimination.clone());
        // Keep the human selection step as the result's durable owner.
        let step: String = tx.query_row(
            "SELECT step_id FROM contest_human_steps WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )?;
        let step: StepId = step.parse().map_err(|_| invalid())?;
        // The state update, result and elimination are committed in this one transaction.
        for (kind, body) in [
            (
                "debug.result",
                serde_json::to_value(round.result.as_ref().ok_or_else(invalid)?)?,
            ),
            ("debug.elimination", serde_json::to_value(elimination)?),
        ] {
            let artifact = ContestArtifact {
                contest_id: s.contest_id,
                step_id: step,
                artifact_kind: kind.parse().expect("kind"),
                work_id: None,
                source_refs: vec![],
                data: body.clone(),
                kind: Some(ContestKind::Debugging),
                round: Some(s.round),
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
                    EnvelopePayload::ContestArtifact(artifact.clone()),
                ),
            )?;
            squad_store::contest_steps::artifact(tx, &artifact, seq)?;
            squad_store::contests::associate(tx, s.contest_id, ContestKind::Debugging, seq)?;
        }
        if let Some(reason) = script::loop_end(&run.differential, s.round, u32::from(setup.rounds))
        {
            run.reason = Some(reason.into());
            s.state = DebuggingStatus::Reporting;
        } else {
            s.round += 1;
            s.state = DebuggingStatus::ProposingTests;
        }
        squad_store::debugging::save(tx, s.contest_id, &run)?;
        let v = s.state_version;
        s.state_version += 1;
        s.step_id = None;
        save(tx, g, id, &s, Some(v))?;
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
        let limit = if matches!(
            work.kind,
            WorkKind::DebuggingHypothesize | WorkKind::DebuggingTest
        ) {
            2
        } else {
            3
        };
        if record.status == WorkStatus::Queued && record.attempt >= limit {
            let failure = super::error(
                ErrorCode::PROVIDER_TIMEOUT,
                "Debug attempt interrupted and exhausted",
            );
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
                        error: Some(failure),
                        debate_id: None,
                        case_id: work.case_id,
                        contest_id: work.contest_id,
                    }),
                ),
            )?;
            tx.execute("UPDATE work SET status='failed',runnable=0,lease_fence=lease_fence+1,completed_event_seq=? WHERE work_id=? AND status='queued'",params![seq.get(),work_id.to_string()])?;
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
    let mut labelled = artifacts
        .into_iter()
        .map(|a| (CellId::new().to_string(), a))
        .collect::<Vec<_>>();
    labelled.sort_by(|a, b| a.0.cmp(&b.0));
    for w in &skipped {
        run.skipped
            .push(format!("{} · {:?} · round {}", w.target, s.state, s.round));
    }
    let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='debug.prompts'",[s.contest_id.to_string()],|r|r.get(0))?;
    let frozen: serde_json::Value = serde_json::from_str(&frozen)?;
    let tokens: Vec<String> = serde_json::from_value(frozen["tokens"].clone())?;
    let body = || {
        labelled
            .first()
            .and_then(|(_, (_, a, _))| a.body.clone())
            .ok_or_else(invalid)
    };
    match s.state {
        DebuggingStatus::Hypothesizing => {
            for (i, (_, (w, a, _))) in labelled.into_iter().enumerate() {
                let label = format!("Table {}", i + 1);
                let Target::Member(n) = w.target else {
                    return Err(invalid());
                };
                run.provenance.insert(label.clone(), n);
                let table: DebuggingHypotheses =
                    serde_json::from_value(a.body.ok_or_else(invalid)?)?;
                for h in &table.hypotheses {
                    run.quotes.extend(script::check_quotes(h, &setup.brief));
                }
                run.tables.insert(label, table);
            }
            squad_store::contest_steps::private_map(
                tx,
                s.contest_id,
                "debug.tables",
                &serde_json::json!({"labels":run.provenance}),
            )?;
            if run.tables.len() < 2 {
                squad_store::debugging::save(tx, s.contest_id, &run)?;
                finish(tx, g, id, &mut s, "insufficient_hypotheses", true)?;
                return Ok(g.accepted(id));
            }
            s.state = DebuggingStatus::Merging;
        }
        DebuggingStatus::Merging => {
            let differential: DebuggingDifferential = serde_json::from_value(body()?)?;
            run.differential = differential
                .hypotheses
                .into_iter()
                .map(|h| DebuggingLiveHypothesis {
                    hypothesis: h,
                    status: DebuggingHypothesisStatus::Live,
                    consistent_results: 0,
                    failed_prediction: None,
                })
                .collect();
            s.state = DebuggingStatus::ProposingTests;
        }
        DebuggingStatus::ProposingTests => {
            let r = run.rounds.last_mut().ok_or_else(invalid)?;
            let mut provenance = BTreeMap::new();
            for (i, (_, (w, a, _))) in labelled.into_iter().enumerate() {
                let label = format!("Proposer {}", i + 1);
                let Target::Member(n) = w.target else {
                    return Err(invalid());
                };
                provenance.insert(label.clone(), n);
                let proposal: DebuggingTestProposal =
                    serde_json::from_value(a.body.ok_or_else(invalid)?)?;
                if let Some(h) = &proposal.new_hypothesis {
                    run.quotes.extend(script::check_quotes(h, &setup.brief));
                }
                r.proposals.insert(label, proposal);
            }
            squad_store::contest_steps::private_map(
                tx,
                s.contest_id,
                &format!("debug.proposers.{}", s.round),
                &serde_json::json!({"labels":provenance}),
            )?;
            if r.proposals.values().all(|p| p.tests.is_empty()) {
                run.reason = Some("no_tests".into());
                s.state = DebuggingStatus::Reporting;
            } else {
                s.state = DebuggingStatus::Selecting;
            }
        }
        DebuggingStatus::Selecting => {
            let selection: DebuggingSelection = serde_json::from_value(body()?)?;
            for h in &selection.added_hypotheses {
                run.differential.push(DebuggingLiveHypothesis {
                    hypothesis: h.clone(),
                    status: DebuggingHypothesisStatus::Live,
                    consistent_results: 0,
                    failed_prediction: None,
                });
            }
            run.rounds.last_mut().ok_or_else(invalid)?.selection = Some(selection.clone());
            if let Some(reference) = &selection.test_ref {
                let tests = script::tests(&run.rounds.last().ok_or_else(invalid)?.proposals);
                let test = tests.get(reference).ok_or_else(invalid)?;
                let instruction = script::blind(&selection.instruction, &tokens).0;
                let pending = parser::PendingHuman {
                    contest_id: s.contest_id,
                    kind: ContestKind::Debugging,
                    state_version: s.state_version + 2,
                    step_id: step,
                    prompt: format!(
                        "RUN THE TEST\n{instruction}\nEffort: {:?} · Safety: {:?}\n/answer O1 <what you saw>",
                        test.effort, test.safety
                    ),
                    items: selection
                        .outcomes
                        .iter()
                        .map(|o| format!("{}: {}", o.label, o.description))
                        .collect(),
                    choices: vec![],
                    outcome_labels: selection.outcomes.iter().map(|o| o.label.clone()).collect(),
                    answers: BTreeMap::new(),
                };
                squad_store::debugging::save(tx, s.contest_id, &run)?;
                let v = s.state_version;
                s.state_version += 1;
                save(tx, g, id, &s, Some(v))?;
                super::human::request(tx, g, id, &pending)?;
                return Ok(g.accepted(id));
            } else {
                run.reason = Some("no_discriminating_test".into());
                s.state = DebuggingStatus::Reporting;
            }
        }
        DebuggingStatus::Reporting => {
            run.narrative = Some(serde_json::from_value(body()?)?);
            let reason = run.reason.clone().ok_or_else(invalid)?;
            squad_store::debugging::save(tx, s.contest_id, &run)?;
            finish(tx, g, id, &mut s, &reason, false)?;
            return Ok(g.accepted(id));
        }
        _ => return Err(invalid()),
    }
    squad_store::debugging::save(tx, s.contest_id, &run)?;
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
    s: &DebuggingState,
    setup: &DebuggingSetup,
    run: &mut Run,
) -> Result<()> {
    let kind = match s.state {
        DebuggingStatus::Hypothesizing => WorkKind::DebuggingHypothesize,
        DebuggingStatus::Merging => WorkKind::DebuggingMerge,
        DebuggingStatus::ProposingTests => WorkKind::DebuggingTest,
        DebuggingStatus::Selecting => WorkKind::DebuggingSelect,
        DebuggingStatus::Reporting => WorkKind::DebuggingReport,
        _ => return Err(invalid()),
    };
    let judge = !matches!(
        kind,
        WorkKind::DebuggingHypothesize | WorkKind::DebuggingTest
    );
    let name = debugging::composition(&kind).ok_or_else(invalid)?;
    if kind == WorkKind::DebuggingTest {
        run.rounds.push(DebuggingRound {
            round: s.round,
            proposals: BTreeMap::new(),
            selection: None,
            result: None,
            elimination: None,
        });
        squad_store::debugging::save(tx, s.contest_id, run)?;
    }
    let words = if kind == WorkKind::DebuggingSelect {
        600
    } else if judge {
        setup.plan_words
    } else {
        setup.room_words
    };
    let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='debug.prompts'",[s.contest_id.to_string()],|r|r.get(0))?;
    let frozen: serde_json::Value = serde_json::from_str(&frozen)?;
    let tokens: Vec<String> = serde_json::from_value(frozen["tokens"].clone())?;
    let ledger=run.rounds.iter().filter_map(|r|r.result.as_ref().map(|result|serde_json::json!({"round":r.round,"test":r.selection.as_ref().map(|s|&s.instruction),"outcome":result.outcome,"evidence":script::prompt_evidence(&result.evidence)}))).collect::<Vec<_>>();
    let allow_new = run
        .rounds
        .iter()
        .rev()
        .find_map(|r| r.result.as_ref())
        .is_some_and(|r| r.unexpected);
    // Judges and subsequent members receive only anonymous material.
    let mut data = serde_json::json!({"brief":setup.brief,"limits":setup.limits});
    match kind {
        WorkKind::DebuggingHypothesize => {}
        WorkKind::DebuggingMerge => {
            data["tables"] = serde_json::to_value(&run.tables)?;
            data["quoteChecks"] = serde_json::to_value(&run.quotes)?;
        }
        _ => {
            data["differential"] = serde_json::to_value(&run.differential)?;
            data["quoteChecks"] = serde_json::to_value(&run.quotes)?;
            data["ledger"] = serde_json::to_value(&ledger)?;
            if kind != WorkKind::DebuggingReport {
                data["allowNewHypothesis"] = allow_new.into();
            }
            if kind == WorkKind::DebuggingSelect {
                data["proposals"] =
                    serde_json::to_value(&run.rounds.last().ok_or_else(invalid)?.proposals)?;
            }
            if kind == WorkKind::DebuggingReport {
                data["patchAllowed"] = script::patch_allowed(&run.differential).into();
            }
        }
    }
    if judge {
        let mut material = data.clone();
        material["step"] = name.into();
        validation::decode::<DebuggingCaseMaterial>(&material).map_err(|_| invalid())?;
    }
    script::blind_value(&mut data, &tokens);
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
                debugging::output_schema(&kind)
                    .ok_or_else(invalid)?
                    .to_string(),
            ),
            ("nick".into(), target.to_string()),
            ("model_slug".into(), binding.model.to_string()),
            (
                "judge_model_slug".into(),
                if matches!(kind, WorkKind::DebuggingMerge | WorkKind::DebuggingSelect) {
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
            serde_json::json!({"input":data,"proseWordLimit":words}).to_string(),
        );
        let source = &frozen["compositions"][name];
        let render = |key: &str| -> Result<String> {
            crate::prompts::inputs_census::render_source(
                source[key].as_str().ok_or_else(invalid)?,
                &values,
            )
        };
        let system = render("system")?;
        let system = if matches!(kind, WorkKind::DebuggingMerge | WorkKind::DebuggingSelect) {
            script::blind(&system, &tokens).0
        } else {
            system
        };
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
            max_output_tokens: Some(if judge { 4096 } else { 2048 }),
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
            WorkKind::DebuggingHypothesize => 1,
            WorkKind::DebuggingMerge => 2,
            WorkKind::DebuggingTest => s.round * 4 - 1,
            WorkKind::DebuggingSelect => s.round * 4,
            WorkKind::DebuggingReport => u32::from(setup.rounds) * 4 + 1,
            _ => return Err(invalid()),
        },
        script_version: "debug-v1".parse().expect("version"),
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
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Debugging, seq)?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (mut item, binding, bundle) in items {
        if judge {
            tx.execute("INSERT OR IGNORE INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![run.case_id.to_string(),item.context_id.0,s.contest_id.to_string(),binding.model.as_str(),data.to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: item.context_id.clone(),
            origin_event_id: id,
            projection_key: "debug.step".parse().expect("key"),
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
    if submission.kind != ContestKind::Debugging
        || s.state_version != submission.expected_state_version
        || !matches!(
            submission.transition,
            ContestTransition::Debugging(DebuggingTransition::Advance)
        )
        || s.state.is_terminal()
        || s.state == DebuggingStatus::AwaitingHuman
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
