//! Transactional readback admission, assignments, barriers and human continuation.
use super::envelope;
use crate::{orchestrator::readback as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::readback::*;
use squad_protocol::*;
use squad_store::{
    Result, StoreError,
    commit::CommitGroup,
    readback::{Round, Run},
};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid readback transition",
    )
}
pub fn load_state(db: &Connection, id: ContestId) -> Result<ReadbackState> {
    let s: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='readback'",
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
        EnvelopePayload::ReadbackSetup(s) => s.contest_id,
        EnvelopePayload::ContestStep(s) => s.contest_id,
        EnvelopePayload::ContestReport(s) => s.contest_id,
        EnvelopePayload::ContestPreflight(s) => s.contest_id,
        EnvelopePayload::ContestPreflightResult(s) => s.contest_id,
        _ => return Err(invalid()),
    };
    let seq = g.push(tx, envelope(id, Actor::System, vec![Recipient::Me], p))?;
    squad_store::contests::associate(tx, contest, ContestKind::Readback, seq)?;
    Ok(seq)
}
pub fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ReadbackState,
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
            EnvelopePayload::ReadbackState(s.clone()),
        ),
    )?;
    if expected.is_none() {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'readback',(SELECT epoch FROM room_meta),?,?,1,?)",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,seq.get()])?;
    } else {
        tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=?,expires_at=CASE WHEN ? THEN NULL ELSE expires_at END WHERE contest_id=?",params![s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get(),s.state==ReadbackStatus::AwaitingHuman,s.contest_id.to_string()])?;
    }
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Readback, seq)?;
    if s.state.is_terminal() {
        tx.execute(
            "UPDATE contest_human_steps SET closed=1 WHERE contest_id=?",
            [s.contest_id.to_string()],
        )?;
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[s.contest_id.to_string()])?;
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
    spec: &str,
    templates: &[u64],
    words: u32,
) -> Vec<super::bounds::Step> {
    use super::bounds::*;
    let step = |kind: WorkKind, target: String, template, data, visible, attempts| Step {
        schema: schema_bytes(readback::output_schema(&kind).expect("readback schema")),
        kind,
        target,
        alternatives: vec![],
        template,
        data,
        visible,
        calls: rounds,
        attempts,
    };
    // Later versions may be twice the original byte size; each byte may need
    // six JSON escape bytes. All rounds reserve that worst case.
    let version_json = 12 * spec.len() as u64 + 2;
    let mut out = participants
        .iter()
        .map(|n| {
            step(
                WorkKind::ReadbackInterpret,
                n.to_string(),
                templates[0],
                2 * version_json + 512,
                2048,
                2,
            )
        })
        .collect::<Vec<_>>();
    out.push(step(
        WorkKind::ReadbackDiff,
        "judge".into(),
        templates[1],
        4 * version_json + participants.len() as u64 * project(limited(2048, words)) + 512,
        4096,
        3,
    ));
    out.push(step(
        WorkKind::ReadbackRewrite,
        "judge".into(),
        templates[2],
        2 * version_json + artifact(4096) + 10 * limited(2048, 100) + 1024,
        4096,
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
    let ParsedInput::Readback {
        participants,
        source,
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
    let (spec, sources) = super::census::source(tx, source)?;
    if spec.trim().is_empty() || participants.len() < 2 {
        return Err(invalid());
    }
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let names = ["readback.interpret", "readback.diff", "readback.rewrite"];
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
    let setup = ReadbackSetup {
        spec,
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
            &setup.spec,
            &templates,
            setup.room_words,
        ),
    )?;
    let contest = ContestId::new();
    let state = ReadbackState {
        contest_id: contest,
        state_version: 1,
        state: ReadbackStatus::Preflight,
        round: 1,
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &state, None)?;
    squad_store::readback::insert(tx, contest, &setup)?;
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
        "readback.prompts",
        &serde_json::json!({"repairTemplate":pack.template("mode.repair").map_err(|_|invalid())?,"compositions":frozen,"tokens":tokens,"personas":config.agents.iter().map(|a|(a.nick.to_string(),a.persona.clone())).collect::<BTreeMap<_,_>>(),"userName":config.user.name,"roster":participants}),
    )?;
    push(
        tx,
        g,
        id,
        EnvelopePayload::ReadbackSetup(ContestSetup {
            contest_id: contest,
            kind: ContestKind::Readback,
            script_version: "readback-v1".parse().expect("version"),
            participants: participants.clone(),
            source_event_ids: sources,
            settings: serde_json::json!({"spec":setup.spec,"rounds":rounds}),
        }),
    )?;
    super::variables::apply(tx, g, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    let (calls, attempts) = script::call_bounds(participants.len() as u32, u32::from(*rounds));
    settings["reservedAttempts"] = attempts.into();
    settings["memberCalls"] = (u32::from(*rounds) * participants.len() as u32).into();
    settings["judgeCalls"] = (u32::from(*rounds) * 2).into();
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Readback,
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
        s.state = ReadbackStatus::Failed;
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
    if state.state != ReadbackStatus::Preflight || plan.plan_id != plan_id {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let version = state.state_version;
    state.state_version += 1;
    if cancel || expired {
        state.state = ReadbackStatus::Stopped;
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
        state.state = ReadbackStatus::WaitingForParticipants;
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
        || readback::composition(&work.kind) != Some(artifact.artifact_kind.as_str())
    {
        return Err(invalid());
    }
    let data = material(db, work)?;
    let output = artifact.body.as_ref().ok_or_else(invalid)?;
    script::validate_output(&work.kind, &data["data"], output).map_err(|_| invalid())?;
    if readback::prose_words(output) > data["wordLimit"].as_u64().ok_or_else(invalid)? as usize {
        return Err(invalid());
    }
    squad_store::contest_steps::artifact(db, artifact, seq)
}

pub fn human_answer(db: &Connection, pending: &parser::PendingHuman, complete: bool) -> Result<()> {
    if !complete {
        return Ok(());
    }
    let mut run = squad_store::readback::run(db, pending.contest_id)?;
    let round = run.rounds.last_mut().ok_or_else(invalid)?;
    let ledger = round.ledger.as_ref().ok_or_else(invalid)?;
    // Shared pending answers retain the text but not whether a single letter
    // came from a token or a literal /answer. Recover that distinction from
    // the committed answer stream, scoped to this human step.
    let mut literal = BTreeMap::new();
    let mut query=db.prepare("SELECT payload FROM events WHERE kind='contest.human_answered' AND json_extract(payload,'$.contestId')=? AND json_extract(payload,'$.stepId')=? ORDER BY seq")?;
    let raws = query
        .query_map(
            params![pending.contest_id.to_string(), pending.step_id.to_string()],
            |r| r.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for raw in raws {
        let event: ContestHumanAnswered = serde_json::from_str(&raw)?;
        if let Some(text) = event.text {
            if let Some((n, _)) = text.split_once(':') {
                if let Ok(n) = n.trim().parse::<u32>() {
                    literal.insert(n, true);
                }
            } else {
                for token in text.split_whitespace() {
                    if let Some(n) = token
                        .get(..token.len().saturating_sub(1))
                        .and_then(|n| n.parse::<u32>().ok())
                    {
                        literal.insert(n, false);
                    }
                }
            }
        }
    }
    let mut answers = BTreeMap::new();
    for (number, answer) in &pending.answers {
        let entry = ledger
            .entries
            .get(*number as usize - 1)
            .ok_or_else(invalid)?;
        let reading = if literal.get(number) != Some(&true)
            && answer.len() == 1
            && answer.as_bytes()[0].is_ascii_lowercase()
        {
            entry
                .readings
                .get((answer.as_bytes()[0] - b'a') as usize)
                .map(|r| r.text.clone())
                .ok_or_else(invalid)?
        } else {
            answer.clone()
        };
        answers.insert(entry.entry_id.clone(), reading);
    }
    round.choices = Some(ReadbackChoices {
        answers,
        raw_answers: vec![],
    });
    run.resume = ReadbackStatus::Rewriting;
    squad_store::readback::save(db, pending.contest_id, &run)
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
    s: &mut ReadbackState,
    reason: &str,
    failed: bool,
) -> Result<()> {
    if s.state == ReadbackStatus::AwaitingHuman
        && let Some(pending) =
            squad_store::contest_steps::pending_human(tx)?.filter(|p| p.contest_id == s.contest_id)
    {
        human_answer(tx, &pending, true)?;
    }
    let run = squad_store::readback::run(tx, s.contest_id)?;
    let step = if let Some(step) = s.step_id {
        step
    } else {
        let step = ContestStep {
            contest_id: s.contest_id,
            step_id: StepId::new(),
            ordinal: s.round * 4,
            script_version: "readback-v1".parse().expect("version"),
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
                .and_then(|a| a.text)
        })
        .collect();
    let report = ReadbackReport {
        rounds: run
            .rounds
            .iter()
            .map(|r| ReadbackRoundReport {
                spec: r.spec.clone(),
                interpretations: r.interpretations.clone(),
                provenance: r.provenance.clone(),
                ledger: r.ledger.clone(),
                choices: r.choices.clone(),
                rewrite: r.rewrite.clone(),
                skipped: r.skipped.clone(),
            })
            .collect(),
        raw_answers,
        final_spec: run.spec,
        reason: reason.into(),
        last_rewrite_not_read_back: reason == "rounds_complete",
        convergence: script::convergence(&run.rounds),
    };
    let artifact = ContestArtifact {
        contest_id: s.contest_id,
        step_id: step,
        artifact_kind: "readback.report".parse().expect("kind"),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Readback),
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
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Readback, seq)?;
    let v = s.state_version;
    s.state_version += 1;
    s.state = if failed {
        ReadbackStatus::Failed
    } else if reason == "stopped" {
        ReadbackStatus::Stopped
    } else {
        ReadbackStatus::Complete
    };
    s.terminal_reason = Some(reason.parse().map_err(|_| invalid())?);
    s.artifact_event_ids.push(event);
    save(tx, g, id, s, Some(v))
}
pub(crate) fn drive_move(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut s: ReadbackState,
) -> Result<RequestResult> {
    if load_state(tx, s.contest_id)? != s {
        return Err(invalid());
    }
    let setup = squad_store::readback::setup(tx, s.contest_id)?;
    let mut run = squad_store::readback::run(tx, s.contest_id)?;
    if s.state == ReadbackStatus::Preflight {
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
    if s.state == ReadbackStatus::WaitingForParticipants {
        for n in &setup.participants {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get(0))?;
            if pending {
                return Ok(g.accepted(id));
            }
        }
        let v = s.state_version;
        s.state = run.resume;
        s.step_id = None;
        if s.state == ReadbackStatus::Rewriting
            && run
                .rounds
                .last()
                .and_then(|r| r.choices.as_ref())
                .is_some_and(|c| c.answers.is_empty())
        {
            finish(tx, g, id, &mut s, "no_readings_chosen", false)?;
        } else {
            s.state_version += 1;
            save(tx, g, id, &s, Some(v))?;
        }
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
        let record = squad_store::work::record(tx, work_id)?.ok_or_else(invalid)?;
        if matches!(record.status, WorkStatus::Queued | WorkStatus::Running) {
            return Ok(g.accepted(id));
        }
        let work = squad_store::work::template(tx, work_id)?
            .ok_or_else(invalid)?
            .0;
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
                finish(tx, g, id, &mut s, "judge_attempts_exhausted", true)?;
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
    match s.state {
        ReadbackStatus::Interpreting => {
            let r = run.rounds.last_mut().ok_or_else(invalid)?;
            for w in skipped {
                if let Target::Member(n) = w.target {
                    r.skipped.push(n);
                }
            }
            // Persisted random labels do not reveal roster order.
            let mut labelled = artifacts
                .into_iter()
                .map(|a| (CellId::new().to_string(), a))
                .collect::<Vec<_>>();
            labelled.sort_by(|a, b| a.0.cmp(&b.0));
            for (i, (_, (w, a, _))) in labelled.into_iter().enumerate() {
                let label = format!("Reader {}", i + 1);
                let Target::Member(n) = w.target else {
                    return Err(invalid());
                };
                r.provenance.insert(label.clone(), n);
                r.interpretations
                    .insert(label, serde_json::from_value(a.body.ok_or_else(invalid)?)?);
            }
            squad_store::contest_steps::private_map(
                tx,
                s.contest_id,
                &format!("readback.readers.{}", s.round),
                &serde_json::json!({"labels":r.provenance}),
            )?;
            if r.interpretations.len() < 2 {
                squad_store::readback::save(tx, s.contest_id, &run)?;
                finish(tx, g, id, &mut s, "insufficient_readings", true)?;
                return Ok(g.accepted(id));
            }
            s.state = ReadbackStatus::Diffing;
        }
        ReadbackStatus::Diffing => {
            let mut ledger: ReadbackLedger = serde_json::from_value(
                artifacts
                    .first()
                    .ok_or_else(invalid)?
                    .1
                    .body
                    .clone()
                    .ok_or_else(invalid)?,
            )?;
            let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='readback.prompts'",[s.contest_id.to_string()],|r|r.get(0))?;
            let frozen: serde_json::Value = serde_json::from_str(&frozen)?;
            let tokens: Vec<String> = serde_json::from_value(frozen["tokens"].clone())?;
            for entry in &mut ledger.entries {
                entry.passage = script::original_passage(&entry.passage, &run.spec, &tokens)
                    .ok_or_else(invalid)?;
            }
            let r = run.rounds.last_mut().ok_or_else(invalid)?;
            r.cut = ledger.entries.len().saturating_sub(10) as u32;
            ledger.entries.truncate(10);
            r.ledger = Some(ledger.clone());
            squad_store::readback::save(tx, s.contest_id, &run)?;
            if ledger.entries.is_empty() {
                finish(tx, g, id, &mut s, "converged", false)?;
                return Ok(g.accepted(id));
            }
            let v = s.state_version;
            s.state_version += 1;
            save(tx, g, id, &s, Some(v))?;
            let pending=parser::PendingHuman{contest_id:s.contest_id,kind:ContestKind::Readback,state_version:s.state_version+1,step_id:step,prompt:"READ IT BACK — 1b 2a ... or /answer 3: your reading; /continue leaves unanswered entries open".into(),items:ledger.entries.iter().map(|e|format!("{}\n{}\n{}",e.passage,e.question,e.readings.iter().enumerate().map(|(i,r)|format!("{}. {} ({} members)",(b'a'+i as u8) as char,r.text,r.source_labels.len())).collect::<Vec<_>>().join("\n"))).collect(),choices:ledger.entries.iter().map(|e|e.readings.iter().map(|r|format!("{} ({} members)",r.text,r.source_labels.len())).collect()).collect(),outcome_labels:vec![],answers:BTreeMap::new()};
            super::human::request(tx, g, id, &pending)?;
            return Ok(g.accepted(id));
        }
        ReadbackStatus::Rewriting => {
            let rewrite: ReadbackRewrite = serde_json::from_value(
                artifacts
                    .first()
                    .ok_or_else(invalid)?
                    .1
                    .body
                    .clone()
                    .ok_or_else(invalid)?,
            )?;
            run.spec = rewrite.spec.clone();
            run.rounds.last_mut().ok_or_else(invalid)?.rewrite = Some(rewrite);
            if s.round >= u32::from(setup.rounds) {
                squad_store::readback::save(tx, s.contest_id, &run)?;
                finish(tx, g, id, &mut s, "rounds_complete", false)?;
                return Ok(g.accepted(id));
            }
            s.round += 1;
            s.state = ReadbackStatus::Interpreting;
        }
        _ => return Err(invalid()),
    }
    squad_store::readback::save(tx, s.contest_id, &run)?;
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
    s: &ReadbackState,
    setup: &ReadbackSetup,
    run: &mut Run,
) -> Result<()> {
    let kind = match s.state {
        ReadbackStatus::Interpreting => WorkKind::ReadbackInterpret,
        ReadbackStatus::Diffing => WorkKind::ReadbackDiff,
        ReadbackStatus::Rewriting => WorkKind::ReadbackRewrite,
        _ => return Err(invalid()),
    };
    let judge = kind != WorkKind::ReadbackInterpret;
    let name = readback::composition(&kind).ok_or_else(invalid)?;
    if !judge {
        run.rounds.push(Round {
            spec: run.spec.clone(),
            ..Default::default()
        });
        squad_store::readback::save(tx, s.contest_id, run)?;
    }
    let r = run.rounds.last().ok_or_else(invalid)?;
    let words = if kind == WorkKind::ReadbackRewrite {
        setup
            .plan_words
            .max(setup.spec.split_whitespace().count() as u32 * 2)
    } else if judge {
        setup.plan_words
    } else {
        setup.room_words
    };
    let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='readback.prompts'",[s.contest_id.to_string()],|r|r.get(0))?;
    let frozen: serde_json::Value = serde_json::from_str(&frozen)?;
    let tokens: Vec<String> = serde_json::from_value(frozen["tokens"].clone())?;
    let data = match kind {
        WorkKind::ReadbackInterpret => serde_json::json!({"spec":run.spec}),
        WorkKind::ReadbackDiff => {
            serde_json::json!({"spec":script::blind(&run.spec,&tokens).0,"interpretations":r.interpretations.iter().map(|(label,reading)|{let mut value=serde_json::to_value(reading).expect("reading");script::blind_value(&mut value,&tokens);serde_json::json!({"label":label,"reading":value})}).collect::<Vec<_>>()})
        }
        WorkKind::ReadbackRewrite => {
            serde_json::json!({"spec":run.spec,"ledger":r.ledger,"choices":r.choices.as_ref().ok_or_else(invalid)?.answers,"firstBytes":setup.spec.len(),"wordLimit":words})
        }
        _ => return Err(invalid()),
    };
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
                readback::output_schema(&kind)
                    .ok_or_else(invalid)?
                    .to_string(),
            ),
            ("nick".into(), target.to_string()),
            ("model_slug".into(), binding.model.to_string()),
            (
                "judge_model_slug".into(),
                if kind == WorkKind::ReadbackDiff {
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
        let system = if kind == WorkKind::ReadbackDiff {
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
        ordinal: (s.round - 1) * 3
            + if judge {
                if kind == WorkKind::ReadbackDiff { 2 } else { 3 }
            } else {
                1
            },
        script_version: "readback-v1".parse().expect("version"),
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
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Readback, seq)?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (mut item, binding, bundle) in items {
        if judge {
            tx.execute("INSERT OR IGNORE INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![run.case_id.to_string(),item.context_id.0,s.contest_id.to_string(),binding.model.as_str(),data.to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: item.context_id.clone(),
            origin_event_id: id,
            projection_key: "readback.step".parse().expect("key"),
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
    if submission.kind != ContestKind::Readback
        || s.state_version != submission.expected_state_version
        || !matches!(
            submission.transition,
            ContestTransition::Readback(ReadbackTransition::Advance)
        )
        || s.state.is_terminal()
        || s.state == ReadbackStatus::AwaitingHuman
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
