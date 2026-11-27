//! Transactional tune admission, step allocation, barriers and authoritative tally.
use crate::orchestrator::tune as math;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use squad_protocol::parser::PlanSource;
use squad_protocol::tune::*;
use squad_protocol::*;
use squad_store::{
    Result, StoreError,
    commit::CommitGroup,
    tune::{TuneJob, TuneRun, TuneSheetRecord},
};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION, "Invalid tune move")
}
fn text(s: impl Into<String>) -> Text {
    Text::new(s).expect("nonempty tune text")
}
fn carrier(id: EventId, payload: EnvelopePayload) -> ProducerEnvelope {
    super::envelope(id, Actor::System, vec![Recipient::Me], payload)
}
fn plan(db: &Connection, id: ContestId) -> Result<ContestPreflight> {
    let raw: String = db.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn load_state(db: &Connection, id: ContestId) -> Result<TuneState> {
    let raw: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='tune'",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
fn save_state(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    state: &TuneState,
) -> Result<()> {
    state.validate().map_err(|_| invalid())?;
    let seq = g.push(tx, carrier(id, EnvelopePayload::TuneState(state.clone())))?;
    let changed=tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=? WHERE contest_id=? AND state_version=?",params![state.state_version as i64,serde_json::to_string(state)?,!state.state.is_terminal(),seq.get(),state.contest_id.to_string(),(state.state_version-1) as i64])?;
    if changed != 1 {
        return Err(invalid());
    }
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Tune, seq)?;
    if state.state.is_terminal() {
        squad_store::history::release(tx, state.contest_id)?;
        squad_store::accounting::release_contest(tx, state.contest_id)?;
    }
    Ok(())
}
pub fn validate_prompt(prompt: &str) -> std::result::Result<(), parser::ParseError> {
    parser::errors::tune_prompt(
        prompt,
        parser::Span {
            start: 0,
            end: prompt.len(),
        },
    )
}
pub fn steps(
    pack: &crate::prompts::PromptPack,
    setup: &TuneSetup,
    room_words: u32,
) -> Result<Vec<super::bounds::Step>> {
    use super::bounds::*;
    let m = setup.participants.len() as u32;
    let g = u32::from(setup.generations);
    let v2 = 2048;
    let v4 = 4096;
    let original = json_text(&setup.prompt);
    let task = json_text(&setup.task);
    let prompt = limited(v4, setup.length_budget as u32);
    let table = [
        (WorkKind::TuneCases, m, 2, v2, original + task),
        (
            WorkKind::TuneFreeze,
            1,
            3,
            v2,
            blinded(&setup.prompt) + task + u64::from(m) * project(limited(v2, room_words)),
        ),
        (
            WorkKind::TuneExecute,
            9 * g + 12,
            2,
            1024,
            prompt + limited(v2, 150),
        ),
        (
            WorkKind::TuneScore,
            g + 2,
            3,
            v2,
            task + 4 * limited(v2, 50) + 3 * limited(v2, 250) + 15 * artifact(1024),
        ),
        (
            WorkKind::TuneMutate,
            3 * g,
            2,
            v4,
            task + prompt + 3 * limited(v2, 100),
        ),
    ];
    let mut out = vec![];
    for (kind, calls, attempts, visible, data) in table {
        let (target, alternatives) = if kind == WorkKind::TuneExecute {
            ("executor".to_owned(), vec![])
        } else if kind.new_mode_judge() {
            ("judge".to_owned(), vec![])
        } else {
            (
                setup.participants[0].to_string(),
                setup
                    .participants
                    .iter()
                    .skip(1)
                    .map(ToString::to_string)
                    .collect(),
            )
        };
        out.push(Step {
            schema: tune::output_schema(&kind).map_or(0, schema_bytes),
            template: if kind.is_executor() {
                0
            } else {
                template(pack, work_name(&kind))?
            },
            kind: kind.clone(),
            target,
            alternatives,
            data,
            visible: kind.new_mode_visible_tokens().unwrap_or(visible),
            calls,
            attempts,
        });
    }
    Ok(out)
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    admission: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    squad_store::tune::ensure_holds(tx)?;
    let ParsedInput::Tune {
        participants,
        prompt,
        task,
        executor,
        generations,
        ..
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
            ErrorCode::DEBATE_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let prompt = match prompt {
        PlanSource::Pasted { text } => text.clone(),
        PlanSource::Quoted { event_id } => super::references::quotes(tx, &[*event_id])?
            .first()
            .ok_or_else(invalid)?
            .text
            .clone(),
        _ => return Err(invalid()),
    };
    if let Err(error) = validate_prompt(&prompt) {
        return Ok(RequestResult::Rejected {
            request_id: id,
            error: super::error(error.code, error.message),
        });
    }
    let participants: Vec<_> = participants
        .iter()
        .filter(|n| executor.as_ref() != Some(*n))
        .cloned()
        .collect();
    if !(3..=12).contains(&participants.len()) || !(1..=6).contains(generations) {
        return Err(invalid());
    }
    let executor = admission
        .bindings
        .get(executor.as_ref().map_or("judge", |n| n.as_str()))
        .ok_or(StoreError::Domain(
            ErrorCode::MODEL_UNAVAILABLE,
            "Tune executor is unavailable",
        ))?
        .clone();
    let seed = uuid::Uuid::new_v4().to_string();
    let setup = TuneSetup {
        length_budget: tune::length_budget(&prompt),
        prompt: prompt.clone(),
        task: task.clone(),
        rotation: math::shuffled(&participants, &seed, "rotation"),
        participants: participants.clone(),
        executor,
        generations: *generations,
        seed,
    };
    let pack = crate::prompts::PromptPack::from_override_dir(
        admission
            .config
            .and_then(|c| c.prompts.directory.as_deref()),
    )
    .map_err(|_| invalid())?;
    let room_words = admission
        .config
        .map_or(300, |c| c.prompts.limits.room_words);
    let mut bindings = admission.bindings.clone();
    bindings.insert("executor".into(), setup.executor.clone());
    let priced = super::bounds::price(tx, &bindings, &steps(&pack, &setup, room_words)?)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    squad_store::accounting::admit(tx, &day, priced.reserved_micros, admission.daily_max)?;
    let spend = squad_store::accounting::daily(tx, &day)?;
    let mut prompts = serde_json::Map::new();
    for name in ["tune.cases", "tune.freeze", "tune.score", "tune.mutate"] {
        let (system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        let (rs, ru) = pack.composition_source(name, true).map_err(|_| invalid())?;
        let rs = rs.replace("{{mode_step}}", name);
        let ru = ru.replace("{{mode_step}}", name);
        prompts.insert(name.into(),serde_json::json!({"system":system,"user":user,"repairSystem":rs,"repairUser":ru,"hash":squad_store::commit::digest(&(system,user,rs,ru))?}));
    }
    let contest_id = ContestId::new();
    let plan_id = PlanId::new();
    let mut settings = priced.settings();
    settings["reservedAttempts"] =
        serde_json::json!(2 * participants.len() as u32 + 27 * u32::from(*generations) + 33);
    settings["scriptVersion"] = serde_json::json!("tune-v1");
    settings["bindings"] = serde_json::json!(bindings);
    settings["packVersion"] = serde_json::json!(pack.version());
    settings["prompts"] = serde_json::json!(prompts);
    settings["roomWords"] = serde_json::json!(room_words);
    settings["dailyMax"] = serde_json::json!(admission.daily_max);
    settings["memberCalls"] = serde_json::json!(participants.len() + 3 * usize::from(*generations));
    settings["executorCalls"] = serde_json::json!(9 * u32::from(*generations) + 12);
    settings["judgeCalls"] = serde_json::json!(u32::from(*generations) + 3);
    let preflight = ContestPreflight {
        contest_id,
        kind: ContestKind::Tune,
        plan_id,
        expires_at: g.accepted_at
            + chrono::Duration::milliseconds(constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: priced.estimated_usd,
            reserved_usd: priced.reserved_usd(),
            remaining_usd: (admission.daily_max - spend.spent_micros - spend.reserved_micros)
                as f64
                / 1e6,
            model_calls: participants.len() as u32 + 13 * u32::from(*generations) + 15,
        },
        participants: participants.clone(),
        confirmation_required: priced.estimated_usd > admission.confirmation_threshold
            && !admission.preconfirmed,
        warning: None,
        settings,
    };
    let state = TuneState {
        contest_id,
        state_version: 1,
        state: if preflight.confirmation_required {
            TuneStatus::Preflight
        } else {
            TuneStatus::WaitingForParticipants
        },
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    let seq = g.push(
        tx,
        carrier(id, EnvelopePayload::TunePreflight(preflight.clone())),
    )?;
    tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at,updated_seq) VALUES(?,'tune',(SELECT epoch FROM room_meta),1,?,1,?,?,?,?)",params![contest_id.to_string(),serde_json::to_string(&state)?,plan_id.to_string(),serde_json::to_string(&preflight)?,preflight.expires_at.to_rfc3339(),seq.get()])?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Tune, seq)?;
    let candidate = TuneCandidate {
        id: uuid::Uuid::new_v4().to_string(),
        parent: None,
        generation: 0,
        author: None,
        prompt: prompt.clone(),
        changes: vec![],
        incomplete: false,
    };
    let mut tokens: Vec<_> = admission
        .bindings
        .iter()
        .flat_map(|(n, b)| vec![n.clone(), b.model.to_string()])
        .collect();
    if let Some(c) = admission.config {
        for a in &c.agents {
            tokens.push(a.persona.clone());
            tokens.push(a.model.clone());
        }
    }
    let run = TuneRun {
        setup: setup.clone(),
        proposals: BTreeMap::new(),
        suite: None,
        champion: candidate.id.clone(),
        champions: vec![candidate.id.clone()],
        candidates: vec![candidate],
        generation: 0,
        generations: vec![],
        sheets: vec![],
        digests: vec![],
        holdout: None,
        report: None,
        jobs: vec![],
        maps: BTreeMap::new(),
        finalists: vec![],
        skipped: vec![],
        skipped_members: vec![],
        skipped_steps: BTreeMap::new(),
        redaction: super::probe::blind_tokens(tokens),
    };
    squad_store::tune::save(tx, contest_id, &run)?;
    let mut event = super::envelope(
        id,
        Actor::Me,
        vec![Recipient::Me],
        EnvelopePayload::UserMessage(UserMessage {
            input: input.clone(),
            body: format!("{prompt}\n--\n{task}"),
            resolved_quotes: vec![],
            debate_id: None,
            case_id: None,
            contest_id: Some(contest_id),
        }),
    );
    event.base.id = id;
    let seq = g.push(tx, event)?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Tune, seq)?;
    let seq = g.push(
        tx,
        carrier(
            id,
            EnvelopePayload::TuneSetup(ContestSetup {
                contest_id,
                kind: ContestKind::Tune,
                script_version: text("tune-v1"),
                participants,
                source_event_ids: vec![],
                settings: serde_json::to_value(&setup)?,
            }),
        ),
    )?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Tune, seq)?;
    let seq = g.push(tx, carrier(id, EnvelopePayload::TuneState(state)))?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Tune, seq)?;
    let seq = g.push(
        tx,
        carrier(id, EnvelopePayload::ContestPreflight(preflight.clone())),
    )?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Tune, seq)?;
    if preflight.confirmation_required {
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(preflight),
            cursor: seq,
        });
    }
    reserve(tx, &preflight, admission.daily_max)?;
    confirmed(
        tx,
        g,
        id,
        &preflight,
        PreflightStatus::Confirmed,
        Some(if admission.preconfirmed {
            Confirmation::Preconfirmed
        } else {
            Confirmation::Automatic
        }),
    )?;
    Ok(g.accepted(id))
}
fn reserve(db: &Connection, p: &ContestPreflight, max: i64) -> Result<()> {
    let key = format!("plan:{}", p.plan_id);
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    squad_store::accounting::reserve(
        db,
        &key,
        None,
        &day,
        squad_store::accounting::reservation_micros(p.estimate.reserved_usd)
            .map_err(|_| invalid())?,
        max,
    )?;
    db.execute(
        "UPDATE reservations SET contest_id=?,plan_id=? WHERE reservation_id=?",
        params![p.contest_id.to_string(), p.plan_id.to_string(), key],
    )?;
    Ok(())
}
fn confirmed(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    p: &ContestPreflight,
    status: PreflightStatus,
    confirmation: Option<Confirmation>,
) -> Result<()> {
    let seq = g.push(
        tx,
        carrier(
            id,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: p.contest_id,
                plan_id: p.plan_id,
                status,
                confirmation,
                error: None,
            }),
        ),
    )?;
    squad_store::contests::associate(tx, p.contest_id, ContestKind::Tune, seq)
}
pub fn resolve(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan_id: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let p = plan(tx, contest)?;
    let mut state = load_state(tx, contest)?;
    if p.plan_id != plan_id || state.state != TuneStatus::Preflight {
        return Err(invalid());
    }
    let expired = g.accepted_at >= p.expires_at;
    if cancel || expired {
        state.state = if cancel {
            TuneStatus::Stopped
        } else {
            TuneStatus::Failed
        };
        state.terminal_reason = Some(text(if cancel { "stopped" } else { "plan_expired" }));
    } else {
        reserve(tx, &p, p.settings["dailyMax"].as_i64().ok_or_else(invalid)?)?;
        state.state = TuneStatus::WaitingForParticipants;
    }
    state.state_version += 1;
    save_state(tx, g, id, &state)?;
    confirmed(
        tx,
        g,
        id,
        &p,
        if cancel {
            PreflightStatus::Cancelled
        } else if expired {
            PreflightStatus::Expired
        } else {
            PreflightStatus::Confirmed
        },
        (!cancel && !expired).then_some(Confirmation::Explicit),
    )?;
    Ok(g.accepted(id))
}
/// Revalidate at the writer boundary, using the exact frozen work projection.
pub fn accepted_work(
    tx: &Connection,
    work: &Work,
    artifact: &ContestArtifact,
    seq: Cursor,
) -> Result<()> {
    let state = load_state(tx, work.contest_id.ok_or_else(invalid)?)?;
    if state.state.is_terminal()
        || state.step_id != work.step_id
        || artifact.artifact_kind.as_str() != work_name(&work.kind)
        || artifact.round != work.round
        || artifact.audience != Some(ArtifactAudience::Human)
    {
        return Err(invalid());
    }
    let run = squad_store::tune::load(tx, state.contest_id)?;
    let job = run
        .jobs
        .iter()
        .find(|j| j.work_id == work.work_id)
        .ok_or_else(invalid)?;
    let body = artifact.body.as_ref().ok_or_else(invalid)?;
    if work.kind == WorkKind::TuneExecute {
        let execution: TuneExecution = validation::decode(body).map_err(|_| invalid())?;
        if Some(&execution.candidate_id) != job.candidate.as_ref()
            || Some(&execution.case_label) != job.case_label.as_ref()
            || execution.output.is_none()
        {
            return Err(invalid());
        }
        squad_store::tune::retain_execution(tx, state.contest_id, &execution)?;
    } else {
        math::validate_output(&job.input, body).map_err(|_| invalid())?;
        math::validate_word_budget(
            &job.input,
            body,
            plan(tx, state.contest_id)?.settings["roomWords"]
                .as_u64()
                .ok_or_else(invalid)? as usize,
        )
        .map_err(|_| invalid())?;
    }
    squad_store::contest_steps::artifact(tx, artifact, seq)
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
    let mut state = load_state(tx, contest)?;
    if state.state.is_terminal() {
        return Err(invalid());
    }
    state.state = TuneStatus::Stopped;
    state.state_version += 1;
    state.terminal_reason = Some(text("stopped"));
    save_state(tx, g, id, &state)
}
pub fn recover(tx: &Connection, _contest: ContestId) -> Result<()> {
    squad_store::tune::ensure_holds(tx)
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let s = load_state(tx, submission.contest_id)?;
    if s.state_version != submission.expected_state_version {
        return Err(invalid());
    }
    match submission.transition {
        ContestTransition::Tune(TuneTransition::Stop) => stop(tx, g, id, s.contest_id),
        ContestTransition::Tune(TuneTransition::Fail) => {
            let mut s = s;
            s.state = TuneStatus::Failed;
            s.terminal_reason = Some(text("judge_failed"));
            s.state_version += 1;
            save_state(tx, g, id, &s)
        }
        _ => Err(invalid()),
    }
}
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
pub fn barrier(tx: &Transaction<'_>, g: &mut CommitGroup, commit: BarrierCommit) -> Result<()> {
    let s = load_state(tx, commit.contest_id)?;
    if s.state_version != commit.expected_state_version
        || s.step_id != Some(commit.step_id)
        || !commit.next_work.is_empty()
        || !commit.projections.is_empty()
    {
        return Err(invalid());
    }
    advance(
        tx,
        g,
        commit.barrier_event.base.correlation_id,
        s.contest_id,
        i64::MAX,
    )
}

pub fn ready(db: &Connection, contest: ContestId) -> Result<bool> {
    let s = load_state(db, contest)?;
    if s.state.is_terminal() {
        return Ok(false);
    }
    if s.state == TuneStatus::Preflight {
        return Ok(chrono::Utc::now() >= plan(db, contest)?.expires_at);
    }
    if let Some(step) = s.step_id {
        let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE step_id=? AND (status='running' OR (status='queued' AND attempt<CASE WHEN kind IN ('tune.freeze','tune.score') THEN 3 ELSE 2 END)))",[step.to_string()],|r|r.get(0))?;
        return Ok(!pending);
    }
    if s.state == TuneStatus::WaitingForParticipants {
        let run = squad_store::tune::load(db, contest)?;
        for n in &run.setup.participants {
            let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get(0))?;
            if pending {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn labelled(run: &TuneRun) -> Vec<TuneLabelledProposal> {
    let p: Vec<_> = run.proposals.values().cloned().collect();
    math::shuffled(&p, &run.setup.seed, "proposals")
        .into_iter()
        .enumerate()
        .map(|(i, proposal)| TuneLabelledProposal {
            label: format!("P{}", i + 1),
            proposal,
        })
        .collect()
}
fn failure(s: &mut TuneState, reason: &str) {
    s.state = TuneStatus::Failed;
    s.terminal_reason = Some(text(reason));
}
fn next_generation(s: &mut TuneState, run: &mut TuneRun) {
    if run.generation < u32::from(run.setup.generations) {
        run.generation += 1;
        s.round = run.generation;
        s.state = TuneStatus::Mutating;
    } else {
        nominate(s, run);
    }
}
fn nominate(s: &mut TuneState, run: &mut TuneRun) {
    if run.champions.len() == 1 {
        s.state = TuneStatus::Reporting;
        s.terminal_reason = None;
        return;
    }
    run.finalists = vec![run.candidates[0].id.clone()];
    for c in run
        .champions
        .iter()
        .skip(run.champions.len().saturating_sub(2))
    {
        if !run.finalists.contains(c) {
            run.finalists.push(c.clone());
        }
    }
    s.state = TuneStatus::HoldoutExecuting;
}
fn app_artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &TuneState,
    step: StepId,
    kind: &str,
    body: serde_json::Value,
) -> Result<()> {
    let a = ContestArtifact {
        kind: Some(ContestKind::Tune),
        round: Some(s.round),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        contest_id: s.contest_id,
        step_id: step,
        artifact_kind: text(kind),
        work_id: None,
        source_refs: vec![],
        data: body,
    };
    let seq = g.push(tx, carrier(id, EnvelopePayload::ContestArtifact(a.clone())))?;
    squad_store::contest_steps::artifact(tx, &a, seq)
}
fn body_for(db: &Connection, work: WorkId) -> Result<Option<serde_json::Value>> {
    let raw:Option<String>=db.query_row("SELECT a.body FROM contest_artifacts a JOIN mode_artifacts m ON m.event_seq=a.event_seq WHERE m.work_id=?",[work.to_string()],|r|r.get(0)).optional()?;
    raw.map(|v| serde_json::from_str(&v).map_err(Into::into))
        .transpose()
}
fn consume(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut TuneState,
    run: &mut TuneRun,
) -> Result<()> {
    let completed_state = s.state;
    let step = s.step_id.ok_or_else(invalid)?;
    tx.execute("UPDATE work SET status='failed',runnable=0 WHERE step_id=? AND status='queued' AND attempt>=CASE WHEN kind IN ('tune.freeze','tune.score') THEN 3 ELSE 2 END",[step.to_string()])?;
    for job in run.jobs.clone() {
        let (work, _) = squad_store::work::template(tx, job.work_id)?.ok_or_else(invalid)?;
        let body = body_for(tx, job.work_id)?;
        if body.is_none() {
            run.skipped.push(job.work_id);
            if let Target::Member(nick) = &work.target {
                if !run.skipped_members.contains(nick) {
                    run.skipped_members.push(nick.clone());
                }
                run.skipped_steps
                    .insert(job.work_id, (nick.clone(), work_name(&work.kind).into()));
            }
            squad_store::contest_steps::settle_if_pending(
                tx,
                step,
                &work.target,
                Some(&super::error(
                    ErrorCode::PROVIDER_ERROR,
                    "Tune work missing after attempts exhausted",
                )),
            )?;
            let cap:bool=tx.query_row("SELECT COALESCE(json_extract(error_json,'$.code'),'')='DAILY_MAX_REACHED' FROM work WHERE work_id=?",[job.work_id.to_string()],|r|r.get(0))?;
            if cap {
                failure(s, "daily_max_reached");
            }
        }
        match &job.input {
            TuneCaseMaterial::Cases { .. } => {
                if let Some(body) = body {
                    let Target::Member(n) = work.target else {
                        return Err(invalid());
                    };
                    run.proposals.insert(n, serde_json::from_value(body)?);
                }
            }
            TuneCaseMaterial::Freeze { proposals, .. } => {
                if let Some(body) = body {
                    let frozen: TuneFreeze = serde_json::from_value(body)?;
                    let mut cases = vec![];
                    for r in frozen.cases {
                        let (label, local) = r.split_once(':').ok_or_else(invalid)?;
                        let c = proposals
                            .iter()
                            .find(|p| p.label == label)
                            .and_then(|p| p.proposal.cases.iter().find(|c| c.local_id == local))
                            .ok_or_else(invalid)?;
                        // Select from the canonical proposals, never from a judge's rewritten text.
                        let original = labelled(run)
                            .into_iter()
                            .find(|p| p.label == label)
                            .and_then(|p| {
                                p.proposal.cases.into_iter().find(|c| c.local_id == local)
                            })
                            .ok_or_else(invalid)?;
                        if c.local_id != original.local_id {
                            return Err(invalid());
                        }
                        cases.push(TuneFrozenCase {
                            label: String::new(),
                            source_ref: r,
                            case: original,
                        });
                    }
                    let (train, holdout) = math::split(&cases, &run.setup.seed);
                    run.suite = Some(TuneSuite {
                        train,
                        holdout,
                        rubric: frozen.rubric,
                        dropped: frozen.dropped,
                    });
                    app_artifact(
                        tx,
                        g,
                        id,
                        s,
                        step,
                        "tune.suite",
                        serde_json::to_value(&run.suite)?,
                    )?;
                } else {
                    failure(s, "judge_failed");
                }
            }
            TuneCaseMaterial::Mutate { .. } => {
                if let Some(body) = body {
                    let m: TuneMutation = serde_json::from_value(body)?;
                    let Target::Member(n) = work.target else {
                        return Err(invalid());
                    };
                    let c = TuneCandidate {
                        id: uuid::Uuid::new_v4().to_string(),
                        parent: Some(run.champion.clone()),
                        generation: run.generation,
                        author: Some(n),
                        prompt: m.prompt,
                        changes: m.changes,
                        incomplete: false,
                    };
                    app_artifact(
                        tx,
                        g,
                        id,
                        s,
                        step,
                        "tune.candidate",
                        serde_json::to_value(&c)?,
                    )?;
                    run.candidates.push(c);
                }
            }
            TuneCaseMaterial::Execute { .. } => {
                if body.is_none() {
                    let value = TuneExecution {
                        candidate_id: job.candidate.ok_or_else(invalid)?,
                        case_label: job.case_label.ok_or_else(invalid)?,
                        output: None,
                        truncated: false,
                    };
                    squad_store::tune::retain_execution(tx, s.contest_id, &value)?;
                    app_artifact(
                        tx,
                        g,
                        id,
                        s,
                        step,
                        "tune.execute",
                        serde_json::to_value(value)?,
                    )?;
                }
            }
            TuneCaseMaterial::Score { .. } => {
                if let Some(body) = body {
                    let scored_generation = run.generation;
                    let sheet: TuneScoreSheet = serde_json::from_value(body)?;
                    let scores = math::scores(&sheet, &run.maps);
                    let holdout = s.state == TuneStatus::HoldoutScoring;
                    if holdout {
                        let original = &run.candidates[0].id;
                        let chronological: Vec<_> = run
                            .champions
                            .iter()
                            .filter(|c| run.finalists.contains(c))
                            .cloned()
                            .collect();
                        let winner = math::final_pick(original, &chronological, &scores);
                        let dropped = run
                            .suite
                            .as_ref()
                            .expect("suite")
                            .holdout
                            .iter()
                            .filter(|c| !sheet.cases.iter().any(|s| s.case_label == c.label))
                            .map(|c| c.label.clone())
                            .collect();
                        let h = TuneHoldout {
                            scores,
                            finalists: run.finalists.clone(),
                            winner,
                            dropped,
                        };
                        app_artifact(
                            tx,
                            g,
                            id,
                            s,
                            step,
                            "tune.holdout",
                            serde_json::to_value(&h)?,
                        )?;
                        run.holdout = Some(h);
                        s.state = TuneStatus::Reporting;
                    } else {
                        if run.generation > 0 {
                            let generation = math::promotion(
                                &run.candidates,
                                run.generation,
                                &run.champion,
                                &scores,
                                &run.setup.seed,
                            );
                            if generation.promoted {
                                run.champion = generation.champion.clone();
                                run.champions.push(run.champion.clone());
                            }
                            app_artifact(
                                tx,
                                g,
                                id,
                                s,
                                step,
                                "tune.generation",
                                serde_json::to_value(&generation)?,
                            )?;
                            run.generations.push(generation);
                        }
                        let d = math::digest(
                            &sheet,
                            &run.maps,
                            &run.champion,
                            &run.suite.as_ref().expect("suite").train,
                        );
                        app_artifact(tx, g, id, s, step, "tune.digest", serde_json::to_value(&d)?)?;
                        run.digests.push(d);
                        if scores[&run.champion] >= 1.0 - 1e-12 {
                            if run.generation == 0 {
                                s.state = TuneStatus::Reporting;
                            } else {
                                nominate(s, run);
                            }
                        } else {
                            next_generation(s, run);
                        }
                    }
                    run.sheets.push(TuneSheetRecord {
                        sheet,
                        maps: run.maps.clone(),
                        holdout,
                        generation: scored_generation,
                    });
                } else {
                    failure(s, "judge_failed");
                }
            }
        }
    }
    if completed_state == TuneStatus::BuildingCases && !s.state.is_terminal() {
        if run.proposals.len() < 2
            || run.proposals.values().map(|p| p.cases.len()).sum::<usize>() < 4
        {
            failure(s, "insufficient_cases");
        } else {
            s.state = TuneStatus::Freezing;
        }
    } else if matches!(completed_state, TuneStatus::Freezing | TuneStatus::Mutating)
        && !s.state.is_terminal()
    {
        s.state = TuneStatus::Executing;
    }
    let seq = g.push(
        tx,
        carrier(
            id,
            EnvelopePayload::ContestReport(ContestReport {
                contest_id: s.contest_id,
                step_id: step,
                artifact_event_ids: vec![],
                skipped_work_ids: run.skipped.clone(),
                data: serde_json::json!({"tuneBarrier":true}),
            }),
        ),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Tune, seq)?;
    s.step_id = None;
    run.jobs.clear();
    Ok(())
}
/// Identity scrub for every non-executor Tune input; public for the isolation fixtures.
#[doc(hidden)]
pub fn redact_value(value: &mut serde_json::Value, tokens: &[String]) {
    match value {
        serde_json::Value::String(s) => *s = super::probe::redact(s, tokens),
        serde_json::Value::Array(a) => {
            for v in a {
                redact_value(v, tokens)
            }
        }
        serde_json::Value::Object(o) => {
            for (key, v) in o {
                // Labels and reference tokens are structural identities, not
                // blind prose. Scrubbing A for a roster nick `a` would destroy
                // the private score map and change the meaning of a sheet.
                if !matches!(
                    key.as_str(),
                    "step" | "kind" | "label" | "caseLabel" | "localId" | "sourceRefs" | "targets"
                ) {
                    redact_value(v, tokens);
                }
            }
        }
        _ => {}
    }
}
fn score_input(
    tx: &Connection,
    s: &TuneState,
    run: &mut TuneRun,
    holdout: bool,
) -> Result<Option<TuneCaseMaterial>> {
    let suite = run.suite.as_ref().ok_or_else(invalid)?;
    let ids: Vec<_> = if holdout {
        run.finalists.clone()
    } else {
        let mut ids = vec![run.candidates[0].id.clone(), run.champion.clone()];
        ids.extend(
            run.candidates
                .iter()
                .filter(|c| c.generation == run.generation && !c.incomplete)
                .map(|c| c.id.clone()),
        );
        ids.sort();
        ids.dedup();
        ids
    };
    let source = if holdout {
        &suite.holdout
    } else {
        &suite.train
    };
    let mut cases = vec![];
    run.maps.clear();
    for c in source {
        let mut outputs = vec![];
        let mut missing = false;
        for candidate in &ids {
            match squad_store::tune::execution(tx, s.contest_id, candidate, &c.label)?
                .and_then(|e| e.output.map(|o| (o, e.truncated)))
            {
                Some((mut output, truncated)) => {
                    if truncated {
                        output.push_str("\n[cut off at the length cap]");
                    }
                    outputs.push((candidate.clone(), output));
                }
                None => {
                    missing = true;
                    break;
                }
            }
        }
        if missing {
            continue;
        }
        let order = math::shuffled(
            &outputs,
            &run.setup.seed,
            &format!("sheet:{}:{}", s.round, holdout),
        );
        // Cyclic offsets guarantee different label orders between adjacent cases.
        let mut order = order;
        if order.len() > 1 {
            let len = order.len();
            order.rotate_left(cases.len() % len);
        }
        let mut maps = BTreeMap::new();
        let outputs = order
            .into_iter()
            .enumerate()
            .map(|(i, (id, output))| {
                let label = ((b'A' + i as u8) as char).to_string();
                maps.insert(label.clone(), id);
                TuneScoringOutput { label, output }
            })
            .collect();
        run.maps.insert(c.label.clone(), maps);
        cases.push(TuneScoringCase {
            case_label: c.label.clone(),
            case: c.case.clone(),
            outputs,
        });
    }
    if cases.is_empty() {
        return Ok(None);
    }
    Ok(Some(TuneCaseMaterial::Score {
        task: run.setup.task.clone(),
        rubric: suite.rubric.clone(),
        cases,
    }))
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut TuneState,
    run: &mut TuneRun,
    actors: Vec<(Target, TuneCaseMaterial, Option<String>, Option<String>)>,
) -> Result<()> {
    let p = plan(tx, s.contest_id)?;
    let bindings: BTreeMap<String, ExecutorBinding> =
        serde_json::from_value(p.settings["bindings"].clone())?;
    let step_id = StepId::new();
    let ordinal: u32 = tx.query_row(
        "SELECT COALESCE(MAX(ordinal),0)+1 FROM contest_steps WHERE contest_id=?",
        [s.contest_id.to_string()],
        |r| r.get(0),
    )?;
    let mut jobs = vec![];
    let mut works = vec![];
    let mut projections = vec![];
    for (target, mut input, candidate, case_label) in actors {
        let kind = input.kind();
        let case_id = (target == Target::Judge).then(CaseId::new);
        let context_id = match &target {
            Target::Member(n) => ContextId(format!("contest:{}:{step_id}:{n}", s.contest_id)),
            Target::Judge => ContextId(format!("judge:{}", case_id.expect("case"))),
        };
        let binding = if kind.is_executor() {
            run.setup.executor.clone()
        } else {
            let mut b = bindings
                .get(&target.to_string())
                .ok_or_else(invalid)?
                .clone();
            if kind.new_mode_judge() {
                b.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
            }
            b
        };
        let (length,output,pricing,supported):(u32,Option<u32>,String,String)=tx.query_row("SELECT context_length,max_output,pricing_json,supported_params_json FROM catalog WHERE slug=?",[binding.model.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let model = squad_agents::resolver::Model {
            id: binding.model.to_string(),
            context_length: length,
            output_limit: output.unwrap_or(8192),
            pricing: serde_json::from_str(&pricing)?,
            supported_parameters: serde_json::from_str(&supported)?,
            reasoning_efforts: vec![],
        };
        // Executor requests preserve both inputs byte for byte. All other blind
        // projections use the shared identity scrubber before being frozen.
        if !kind.is_executor() {
            let mut v = serde_json::to_value(&input)?;
            redact_value(&mut v, &run.redaction);
            input = serde_json::from_value(v)?;
        }
        let frozen = p.settings["prompts"][work_name(&kind)].clone();
        let projection = serde_json::json!({"input":input,"frozen":frozen,"model":model,"candidateId":candidate,"caseLabel":case_label,"roomWords":p.settings["roomWords"],"stepWords":if kind==WorkKind::TuneMutate{run.setup.length_budget+150}else if kind==WorkKind::TuneScore{600}else{p.settings["roomWords"].as_u64().unwrap_or(300) as usize}});
        let w = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: kind.clone(),
            target: target.clone(),
            context_id,
            prompt_ref: if kind.is_executor() {
                None
            } else {
                Some(PromptRef {
                    pack_version: text(p.settings["packVersion"].as_str().ok_or_else(invalid)?),
                    template_id: text(work_name(&kind)),
                    template_hash: text(frozen["hash"].as_str().ok_or_else(invalid)?),
                })
            },
            max_output_tokens: kind.new_mode_visible_tokens(),
            case_id,
            trigger_event_id: id,
            history_through: HistoryPosition::new(1).expect("positive"),
            attempt: 1,
            debate_id: None,
            round: Some(s.round),
            replay: None,
            contest_id: Some(s.contest_id),
            step_id: Some(step_id),
        };
        jobs.push(TuneJob {
            work_id: w.work_id,
            input,
            candidate,
            case_label,
        });
        projections.push(projection);
        works.push((w, binding));
    }
    let step = ContestStep {
        contest_id: s.contest_id,
        step_id,
        ordinal,
        script_version: text("tune-v1"),
        move_name: if works.iter().all(|(w, _)| w.target == Target::Judge) {
            ContestMove::Judge
        } else {
            ContestMove::Fanout
        },
        inputs: vec![],
        assignments: works
            .iter()
            .map(|(w, _)| ContestAssignment {
                target: w.target.clone(),
                context_id: w.context_id.clone(),
                payload_refs: vec![],
                work_id: Some(w.work_id),
            })
            .collect(),
    };
    let seq = g.push(tx, carrier(id, EnvelopePayload::ContestStep(step.clone())))?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Tune, seq)?;
    let source = squad_store::events::at(tx, seq.get())?;
    for ((mut w, binding), projection) in works.into_iter().zip(projections) {
        if let Some(case_id) = w.case_id {
            let input = jobs
                .iter()
                .find(|j| j.work_id == w.work_id)
                .ok_or_else(invalid)?
                .input
                .clone();
            let opened = CaseOpened {
                case_id,
                judge_invocation_id: EventId::new(),
                work_id: Some(w.work_id),
                judge_model: binding.model.clone(),
                // The shared validator still rejects CaseMaterial::Tune. Carry
                // the same typed, scoped JSON through the released standalone
                // case until the integrator enables that registry arm.
                material: CaseMaterial::Standalone {
                    messages: vec![ResolvedQuote {
                        event_id: source.base.id,
                        author: Actor::System,
                        text: serde_json::to_string(&input)?,
                    }],
                    question: text(work_name(&w.kind)),
                },
                debate_id: None,
                contest_id: Some(s.contest_id),
            };
            let seq = g.push(tx, carrier(id, EnvelopePayload::CaseOpened(opened.clone())))?;
            squad_store::cases::open(tx, &opened, seq)?;
        }
        w.trigger_event_id = source.base.id;
        w.history_through = squad_store::history::project(
            tx,
            &HistoryProjection {
                context_id: w.context_id.clone(),
                origin_event_id: source.base.id,
                projection_key: text("tune.input"),
                role: HistoryRole::User,
                name: Some("squad".into()),
                content: serde_json::to_string(&projection)?,
                meta: HistoryMetadata::Anonymous { anonymous: True },
            },
        )?
        .ok_or_else(invalid)?;
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step_id, &w)?;
        g.push(
            tx,
            carrier(
                id,
                EnvelopePayload::WorkState(WorkStateEvent {
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
                }),
            ),
        )?;
    }
    run.jobs = jobs;
    s.step_id = Some(step_id);
    Ok(())
}
/// One serialized engine move: consume a finished barrier and allocate the next
/// work atomically. Every generated random identity is committed before dispatch.
pub fn advance(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    _max: i64,
) -> Result<()> {
    if !ready(tx, contest)? {
        return Err(invalid());
    }
    let mut s = load_state(tx, contest)?;
    let mut run = squad_store::tune::load(tx, contest)?;
    if s.state == TuneStatus::Preflight {
        let p = plan(tx, contest)?;
        resolve(tx, g, id, contest, p.plan_id, false)?;
        return Ok(());
    }
    if s.step_id.is_some() {
        consume(tx, g, id, &mut s, &mut run)?;
    }
    for _ in 0..32 {
        if s.state.is_terminal() {
            break;
        }
        let mut actors = vec![];
        match s.state {
            TuneStatus::WaitingForParticipants | TuneStatus::BuildingCases => {
                s.state = TuneStatus::BuildingCases;
                actors = run
                    .setup
                    .participants
                    .iter()
                    .map(|n| {
                        (
                            Target::Member(n.clone()),
                            TuneCaseMaterial::Cases {
                                prompt: run.setup.prompt.clone(),
                                task: run.setup.task.clone(),
                            },
                            None,
                            None,
                        )
                    })
                    .collect();
            }
            TuneStatus::Freezing => actors.push((
                Target::Judge,
                TuneCaseMaterial::Freeze {
                    prompt: run.setup.prompt.clone(),
                    task: run.setup.task.clone(),
                    proposals: labelled(&run),
                },
                None,
                None,
            )),
            TuneStatus::Mutating => {
                let digest = run.digests.last().ok_or_else(invalid)?.clone();
                let parent = run
                    .candidates
                    .iter()
                    .find(|c| c.id == run.champion)
                    .ok_or_else(invalid)?;
                actors = math::mutators(&run.setup.rotation, run.generation)
                    .into_iter()
                    .map(|n| {
                        (
                            Target::Member(n),
                            TuneCaseMaterial::Mutate {
                                task: run.setup.task.clone(),
                                prompt: parent.prompt.clone(),
                                digest: digest.clone(),
                                length_budget: run.setup.length_budget,
                                placeholders: run.setup.prompt.matches("{{input}}").count(),
                            },
                            None,
                            None,
                        )
                    })
                    .collect();
            }
            TuneStatus::Executing | TuneStatus::HoldoutExecuting => {
                let holdout = s.state == TuneStatus::HoldoutExecuting;
                let suite = run.suite.as_ref().ok_or_else(invalid)?;
                let cases = if holdout {
                    &suite.holdout
                } else {
                    &suite.train
                };
                let candidates: Vec<_> = run
                    .candidates
                    .iter()
                    .filter(|c| {
                        if holdout {
                            run.finalists.contains(&c.id)
                        } else {
                            c.generation == run.generation
                        }
                    })
                    .collect();
                for candidate in &candidates {
                    for case in cases {
                        if squad_store::tune::execution(tx, contest, &candidate.id, &case.label)?
                            .is_none()
                        {
                            actors.push((
                                Target::Judge,
                                TuneCaseMaterial::Execute {
                                    prompt: candidate.prompt.clone(),
                                    input: case.case.input.clone(),
                                },
                                Some(candidate.id.clone()),
                                Some(case.label.clone()),
                            ));
                            break;
                        }
                    }
                    if !actors.is_empty() {
                        break;
                    }
                }
                if actors.is_empty() {
                    if holdout {
                        s.state = TuneStatus::HoldoutScoring;
                    } else if run.generation == 0 {
                        let suite = run.suite.as_mut().expect("suite");
                        let mut train = vec![];
                        for c in &suite.train {
                            if squad_store::tune::execution(tx, contest, &run.champion, &c.label)?
                                .is_some_and(|e| e.output.is_some())
                            {
                                train.push(c.clone());
                            } else {
                                suite.dropped.push(TuneDroppedCase {
                                    case_ref: c.source_ref.clone(),
                                    why: "baseline execution missing".into(),
                                });
                            }
                        }
                        suite.train = train;
                        if suite.train.len() < 2 {
                            failure(&mut s, "insufficient_cases");
                        } else {
                            s.state = TuneStatus::Scoring;
                        }
                    } else {
                        let suite = run.suite.as_ref().expect("suite");
                        for c in run
                            .candidates
                            .iter_mut()
                            .filter(|c| c.generation == run.generation)
                        {
                            for case in &suite.train {
                                if squad_store::tune::execution(tx, contest, &c.id, &case.label)?
                                    .is_none_or(|e| e.output.is_none())
                                {
                                    c.incomplete = true;
                                }
                            }
                        }
                        if run
                            .candidates
                            .iter()
                            .any(|c| c.generation == run.generation && !c.incomplete)
                        {
                            s.state = TuneStatus::Scoring;
                        } else {
                            run.generations.push(TuneGeneration {
                                generation: run.generation,
                                scores: BTreeMap::new(),
                                best: None,
                                champion: run.champion.clone(),
                                promoted: false,
                            });
                            next_generation(&mut s, &mut run);
                        }
                    }
                    continue;
                }
            }
            TuneStatus::Scoring | TuneStatus::HoldoutScoring => {
                let holdout = s.state == TuneStatus::HoldoutScoring;
                if let Some(input) = score_input(tx, &s, &mut run, holdout)? {
                    actors.push((Target::Judge, input, None, None));
                } else {
                    failure(
                        &mut s,
                        if holdout {
                            "holdout_unavailable"
                        } else {
                            "insufficient_cases"
                        },
                    );
                    continue;
                }
            }
            TuneStatus::Reporting => {
                finish(tx, g, id, &mut s, &mut run)?;
                break;
            }
            _ => return Err(invalid()),
        }
        allocate(tx, g, id, &mut s, &mut run, actors)?;
        break;
    }
    if s.state == TuneStatus::Failed && run.report.is_none() {
        finish(tx, g, id, &mut s, &mut run)?;
    }
    let mut q=tx.prepare("SELECT e.id FROM mode_artifacts a JOIN events e ON e.seq=a.event_seq WHERE a.contest_id=? ORDER BY e.seq")?;
    s.artifact_event_ids = q
        .query_map([contest.to_string()], |r| r.get::<_, String>(0))?
        .map(|r| r?.parse().map_err(|_| invalid()))
        .collect::<Result<Vec<_>>>()?;
    resize_reservation(tx, &s, &run)?;
    s.state_version += 1;
    save_state(tx, g, id, &s)?;
    squad_store::tune::save(tx, contest, &run)?;
    Ok(())
}
fn finish(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut TuneState,
    run: &mut TuneRun,
) -> Result<()> {
    let reason = if let Some(reason) = &s.terminal_reason {
        reason.to_string()
    } else if let Some(h) = &run.holdout {
        if h.winner == run.candidates[0].id {
            "original_kept".into()
        } else {
            "tuned".into()
        }
    } else if run.champions.len() == 1 && run.generation == 0 {
        "nothing_to_fix".into()
    } else {
        "no_improvement".into()
    };
    let winner = run
        .holdout
        .as_ref()
        .map_or_else(|| run.candidates[0].id.clone(), |h| h.winner.clone());
    let prompt = run
        .candidates
        .iter()
        .find(|c| c.id == winner)
        .ok_or_else(invalid)?
        .prompt
        .clone();
    let result_line = match reason.as_str() {
        "tuned" => {
            let h = run.holdout.as_ref().expect("holdout");
            format!(
                "Tuned: {:.2} to {:.2} on held-out cases.",
                h.scores[&run.candidates[0].id], h.scores[&winner]
            )
        }
        "original_kept" => {
            "The original stands. The best variant did not hold up on unseen cases.".into()
        }
        "nothing_to_fix" => "Nothing to fix: the original already passes every train case.".into(),
        "no_improvement" => "The original stands. No variant beat it.".into(),
        _ => format!(
            "Tune failed: {reason}. The train champion was not verified; committed work remains exportable."
        ),
    };
    run.report = Some(TuneReport {
        reason: reason.clone(),
        result_line,
        winner,
        prompt: prompt.clone(),
        diff: crate::export::tune::unified_diff(&run.setup.prompt, &prompt),
    });
    let step = ContestStep {
        contest_id: s.contest_id,
        step_id: StepId::new(),
        ordinal: tx.query_row(
            "SELECT COALESCE(MAX(ordinal),0)+1 FROM contest_steps WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )?,
        script_version: text("tune-v1"),
        move_name: ContestMove::Report,
        inputs: vec![],
        assignments: vec![],
    };
    let seq = g.push(tx, carrier(id, EnvelopePayload::ContestStep(step.clone())))?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    app_artifact(
        tx,
        g,
        id,
        s,
        step.step_id,
        "tune.report",
        serde_json::to_value(&run.report)?,
    )?;
    let executions = squad_store::tune::executions(tx, s.contest_id)?;
    let seq = g.push(
        tx,
        carrier(
            id,
            EnvelopePayload::ContestReport(ContestReport {
                contest_id: s.contest_id,
                step_id: step.step_id,
                artifact_event_ids: vec![],
                skipped_work_ids: run.skipped.clone(),
                data: serde_json::json!({"tuneArchive":run,"executions":executions}),
            }),
        ),
    )?;
    squad_store::contest_steps::barrier(tx, step.step_id, seq)?;
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Tune, seq)?;
    if s.state != TuneStatus::Failed {
        s.state = TuneStatus::Complete;
    }
    s.step_id = None;
    s.terminal_reason = Some(text(reason));
    Ok(())
}

/// Release the unstarted difference as soon as the committed script makes it
/// impossible to spend it. Started/unknown attempt reservations are separate.
fn resize_reservation(tx: &Connection, s: &TuneState, run: &TuneRun) -> Result<()> {
    if s.state.is_terminal() {
        return Ok(());
    }
    let p = plan(tx, s.contest_id)?;
    let mut calls: BTreeMap<&str, u64> = BTreeMap::new();
    let mut add = |k: &'static str, n: u64| {
        *calls.entry(k).or_default() += n;
    };
    let g = u64::from(run.setup.generations);
    let current = u64::from(run.generation);
    let remaining = g.saturating_sub(current);
    let (t, h) = run
        .suite
        .as_ref()
        .map_or((3, 3), |x| (x.train.len() as u64, x.holdout.len() as u64));
    let future_generations = |add: &mut dyn FnMut(&'static str, u64), n: u64| {
        add("tune.mutate", 3 * n);
        add("tune.execute", 3 * n * t);
        add("tune.score", n);
    };
    let holdout = |add: &mut dyn FnMut(&'static str, u64)| {
        add("tune.execute", 3 * h);
        add("tune.score", 1);
    };
    match s.state {
        TuneStatus::BuildingCases | TuneStatus::Freezing => {
            if s.state == TuneStatus::BuildingCases {
                add("tune.freeze", 1);
            }
            add("tune.execute", t);
            add("tune.score", 1);
            future_generations(&mut add, g);
            holdout(&mut add);
        }
        TuneStatus::Mutating => {
            add("tune.execute", 3 * t);
            add("tune.score", 1);
            future_generations(&mut add, remaining);
            holdout(&mut add);
        }
        TuneStatus::Executing => {
            let suite = run.suite.as_ref().ok_or_else(invalid)?;
            let mut missing = 0_u64;
            for candidate in run
                .candidates
                .iter()
                .filter(|c| u64::from(c.generation) == current)
            {
                for case in &suite.train {
                    if squad_store::tune::execution(tx, s.contest_id, &candidate.id, &case.label)?
                        .is_none()
                    {
                        missing += 1;
                    }
                }
            }
            add(
                "tune.execute",
                missing.saturating_sub(run.jobs.len() as u64),
            );
            add("tune.score", 1);
            future_generations(&mut add, remaining);
            holdout(&mut add);
        }
        TuneStatus::Scoring => {
            future_generations(&mut add, remaining);
            holdout(&mut add);
        }
        TuneStatus::HoldoutExecuting => {
            let suite = run.suite.as_ref().ok_or_else(invalid)?;
            let mut missing = 0_u64;
            for candidate in &run.finalists {
                for case in &suite.holdout {
                    if squad_store::tune::execution(tx, s.contest_id, candidate, &case.label)?
                        .is_none()
                    {
                        missing += 1;
                    }
                }
            }
            add(
                "tune.execute",
                missing.saturating_sub(run.jobs.len() as u64),
            );
            add("tune.score", 1);
        }
        TuneStatus::HoldoutScoring | TuneStatus::Reporting => {}
        _ => return Ok(()),
    }
    let mut needed = 0_i64;
    for (kind, n) in calls {
        let attempts = if kind == "tune.freeze" || kind == "tune.score" {
            3
        } else {
            2
        };
        needed += p.settings["attemptShares"][kind]
            .as_i64()
            .ok_or_else(invalid)?
            * n as i64
            * attempts;
    }
    for job in &run.jobs {
        let kind = job.input.kind();
        let share = p.settings["attemptShares"][work_name(&kind)]
            .as_i64()
            .ok_or_else(invalid)?;
        let attempt: u32 = tx.query_row(
            "SELECT attempt FROM work WHERE work_id=?",
            [job.work_id.to_string()],
            |r| r.get(0),
        )?;
        needed +=
            share * i64::from(u32::from(constants::attempt_limit(&kind)).saturating_sub(attempt));
    }
    let key = format!("plan:{}", p.plan_id);
    let row: Option<(String, i64)> = tx
        .query_row(
            "SELECT day,amount_micros FROM reservations WHERE reservation_id=?",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((day, amount)) = row {
        if needed > amount {
            return Err(StoreError::Domain(
                ErrorCode::INVALID_STATE_TRANSITION,
                "Tune future work exceeds frozen reservation",
            ));
        }
        tx.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![amount - needed, day],
        )?;
        tx.execute(
            "UPDATE reservations SET amount_micros=? WHERE reservation_id=?",
            params![needed, key],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn blind_prose_scrubbing_preserves_structural_label_maps() {
        let mut value = serde_json::json!({"caseLabel":"T1","label":"A","localId":"a","input":"a secret story","sourceRefs":["P1:a"]});
        super::redact_value(&mut value, &["a".into(), "t1".into()]);
        assert_eq!(value["caseLabel"], "T1");
        assert_eq!(value["label"], "A");
        assert_eq!(value["localId"], "a");
        assert_eq!(value["sourceRefs"][0], "P1:a");
        assert_eq!(value["input"], "[redacted] secret story");
    }
}
