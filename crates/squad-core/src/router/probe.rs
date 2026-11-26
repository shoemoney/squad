//! Transactional implementation of the probe script's shared engine hooks.
use super::{envelope, error};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION, "Invalid probe move")
}
fn carrier(id: EventId, payload: EnvelopePayload) -> ProducerEnvelope {
    envelope(id, Actor::System, vec![Recipient::Me], payload)
}
pub fn save(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &ProbeState,
) -> Result<()> {
    let pending: bool = tx.query_row(
        "SELECT json_extract(state_json,'$.state')='PREFLIGHT' FROM contests WHERE contest_id=?",
        [state.contest_id.to_string()],
        |r| r.get(0),
    )?;
    let seq = group.push(tx, carrier(id, EnvelopePayload::ProbeState(state.clone())))?;
    squad_store::probe::save(tx, state, state.state_version - 1, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Probe, seq)?;
    if pending && state.state.is_terminal() {
        let (plan, expires): (String, String) = tx.query_row(
            "SELECT plan_id,expires_at FROM contests WHERE contest_id=?",
            [state.contest_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let status = if state.state == ProbeStatus::Stopped {
            PreflightStatus::Cancelled
        } else if expires <= group.accepted_at.to_rfc3339() {
            PreflightStatus::Expired
        } else {
            PreflightStatus::Rejected
        };
        let result = ContestPreflightResult {
            contest_id: state.contest_id,
            plan_id: plan.parse().map_err(|_| invalid())?,
            error: (status == PreflightStatus::Rejected).then(|| {
                error(
                    ErrorCode::INVALID_STATE_TRANSITION,
                    "Probe preflight rejected",
                )
            }),
            status,
            confirmation: None,
        };
        let seq = group.push(
            tx,
            carrier(id, EnvelopePayload::ContestPreflightResult(result)),
        )?;
        squad_store::contests::associate(tx, state.contest_id, ContestKind::Probe, seq)?;
    }
    Ok(())
}
/// Frozen probe steps: every member proposes or checks blind, explainers predict
/// over all contributions, and the judge reports over everything accepted.
pub fn steps(
    pack: &crate::prompts::PromptPack,
    setup: &ProbeSetup,
    words: (u32, u32),
) -> Result<Vec<super::bounds::Step>> {
    use super::bounds::{Step, blinded, limited, project, schema_bytes, template};
    let m = setup.participants.len() as u64;
    let member = constants::DEBATE_TURN_VISIBLE_TOKENS;
    let judge = constants::FINAL_JUDGE_VISIBLE_TOKENS;
    let brief = blinded(setup.brief.observation.as_str()) + 512;
    let contribution = project(limited(member, words.0));
    let contributions = m * contribution;
    let rows = (m / 2) * contribution;
    let mut out = vec![];
    let mut push =
        |kind, target: &str, composition: &str, schema, data, visible, attempts| -> Result<()> {
            out.push(Step {
                kind,
                target: target.into(),
                alternatives: vec![],
                template: template(pack, composition)?,
                schema: schema_bytes(schema),
                data,
                visible,
                calls: 1,
                attempts,
            });
            Ok(())
        };
    for nick in &setup.participants {
        if setup.explainers.contains(nick) {
            push(
                WorkKind::ProbeExplain,
                nick.as_str(),
                "probe.explain",
                validation::schema::<ProbeExplanation>(),
                brief,
                member,
                2,
            )?;
            push(
                WorkKind::ProbePredict,
                nick.as_str(),
                "probe.predict",
                validation::schema::<ProbePrediction>(),
                brief + contributions,
                member,
                2,
            )?;
        } else {
            push(
                WorkKind::ProbeCheck,
                nick.as_str(),
                "probe.check",
                validation::schema::<ProbeCheck>(),
                brief,
                member,
                2,
            )?;
        }
    }
    push(
        WorkKind::ProbeReport,
        "judge",
        "probe.report",
        validation::schema::<ProbeReport>(),
        brief + contributions + rows,
        judge,
        3,
    )?;
    Ok(out)
}
/// Admission freezes the resolved roster and split before any call can be made.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    participants: Vec<MemberNick>,
    brief: String,
    bindings: &BTreeMap<String, ExecutorBinding>,
    max: i64,
    threshold: f64,
    preconfirmed: bool,
    config: Option<&SquadConfig>,
) -> Result<RequestResult> {
    let active: Option<String> = tx
        .query_row("SELECT kind FROM contests WHERE active=1", [], |r| r.get(0))
        .optional()?;
    if active.is_some() {
        return Err(StoreError::Domain(
            ErrorCode::DEBATE_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let mut participants = participants;
    let sitting_out = if !participants.len().is_multiple_of(2) {
        participants.pop()
    } else {
        None
    };
    if participants.len() < 4 {
        return Err(StoreError::Domain(
            ErrorCode::INVALID_COMMAND,
            "Probe needs at least four participating members",
        ));
    }
    let contest_id = ContestId::new();
    let mut shuffled = participants.clone();
    shuffled.sort_by_cached_key(|_| CellId::new().to_string());
    let setup = ProbeSetup {
        contest_id,
        brief: ProbeBrief {
            observation: Text::new(brief.clone()).map_err(|_| invalid())?,
            ruled_out: String::new(),
            inspectable: Text::new("Only resources explicitly supplied in the diagnostic brief")
                .expect("literal"),
        },
        participants: participants.clone(),
        explainers: shuffled[..shuffled.len() / 2].to_vec(),
        checkers: shuffled[shuffled.len() / 2..].to_vec(),
        sitting_out,
    };
    setup.validate().map_err(|_| invalid())?;
    let pack = crate::prompts::PromptPack::from_override_dir(
        config.and_then(|c| c.prompts.directory.as_deref()),
    )
    .map_err(|_| StoreError::Domain(ErrorCode::CONFIG_INVALID, "Invalid probe prompt pack"))?;
    let priced = super::bounds::price(
        tx,
        bindings,
        &steps(
            &pack,
            &setup,
            config.map_or((300, 1500), |c| {
                (c.prompts.limits.room_words, c.prompts.limits.plan_words)
            }),
        )?,
    )
    .map_err(|e| match e {
        StoreError::Domain(ErrorCode::CONTEXT_TOO_LARGE, _) => StoreError::Domain(
            ErrorCode::CONTEXT_TOO_LARGE,
            "Probe evidence does not fit the model context",
        ),
        e => e,
    })?;
    let nominal = priced.estimated_usd;
    let worst = priced.reserved_usd();
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let reserve = priced.reserved_micros;
    if let Err(StoreError::Domain(ErrorCode::DAILY_MAX_REACHED, _)) =
        squad_store::accounting::admit(tx, &day, reserve, max)
    {
        return Ok(RequestResult::Rejected {
            request_id: id,
            error: super::daily_cap_error(tx, &day, reserve, max),
        });
    }
    squad_store::accounting::admit(tx, &day, reserve, max)?;
    let spend = squad_store::accounting::daily(tx, &day)?;
    let plan_id = PlanId::new();
    let pack = crate::prompts::PromptPack::from_override_dir(
        config.and_then(|c| c.prompts.directory.as_deref()),
    )
    .map_err(|_| StoreError::Domain(ErrorCode::CONFIG_INVALID, "Invalid probe prompt pack"))?;
    let mut frozen_prompts = serde_json::Map::new();
    for composition in [
        "probe.explain",
        "probe.check",
        "probe.predict",
        "probe.report",
    ] {
        let (system, user) = pack
            .composition_source(composition, false)
            .map_err(|_| invalid())?;
        let (repair_system, repair_user) = pack
            .composition_source(composition, true)
            .map_err(|_| invalid())?;
        frozen_prompts.insert(composition.into(),serde_json::json!({"system":system,"user":user,"repairSystem":repair_system,"repairUser":repair_user,"hash":squad_store::commit::digest(&(system,user,repair_system,repair_user))?}));
    }
    let plan = ProbePreflight {
        base: ContestPreflight {
            contest_id,
            kind: ContestKind::Probe,
            plan_id,
            expires_at: group.accepted_at
                + chrono::Duration::milliseconds(
                    squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64,
                ),
            estimate: DebateEstimate {
                estimated_usd: nominal,
                reserved_usd: worst,
                remaining_usd: (max - spend.spent_micros - spend.reserved_micros) as f64 / 1e6,
                model_calls: setup.calls(),
            },
            participants,
            confirmation_required: nominal > threshold && !preconfirmed,
            warning: setup.sitting_out.as_ref().map(|n| {
                Text::new(format!("@{n} sits out to keep equal groups")).expect("nonempty")
            }),
            settings: serde_json::json!({"attemptShares":priced.shares,"stepInputBounds":priced.input_bounds,"scriptVersion":"probe-v1","packVersion":pack.version(),"prompts":frozen_prompts,"bindings":bindings,"roomWords":config.map(|c|c.prompts.limits.room_words).unwrap_or(300),"planWords":config.map(|c|c.prompts.limits.plan_words).unwrap_or(1500)}),
        },
        reserved_attempts: setup.attempts(),
        setup: setup.clone(),
    };
    let state = ProbeState {
        contest_id,
        state: if plan.base.confirmation_required {
            ProbeStatus::Preflight
        } else {
            ProbeStatus::WaitingForParticipants
        },
        state_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        skipped_work_ids: vec![],
        skipped_members: vec![],
        terminal_reason: None,
    };
    let seq = group.push(
        tx,
        carrier(id, EnvelopePayload::ProbePreflight(plan.clone())),
    )?;
    tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at,updated_seq) VALUES (?,'probe',(SELECT epoch FROM room_meta),1,?,1,?,?,?,?)",params![contest_id.to_string(),serde_json::to_string(&state)?,plan_id.to_string(),serde_json::to_string(&plan)?,plan.base.expires_at.to_rfc3339(),seq.get()])?;
    squad_store::probe::roles(tx, &setup)?;
    let mut tokens = bindings
        .iter()
        .flat_map(|(n, b)| vec![n.clone(), b.model.to_string()])
        .collect::<Vec<_>>();
    if let Some(config) = config {
        tokens.push(config.judge.model.clone());
        for member in &config.agents {
            tokens.push(member.persona.clone());
            tokens.push(member.model.clone());
        }
    }
    let tokens = blind_tokens(tokens);
    squad_store::contest_steps::private_map(
        tx,
        contest_id,
        "probe.redaction",
        &serde_json::json!({"tokens":tokens}),
    )?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Probe, seq)?;
    let mut event = envelope(
        id,
        Actor::Me,
        vec![Recipient::Me],
        EnvelopePayload::UserMessage(UserMessage {
            input,
            body: brief,
            resolved_quotes: vec![],
            debate_id: None,
            case_id: None,
            contest_id: Some(contest_id),
        }),
    );
    event.base.id = id;
    let seq = group.push(tx, event)?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Probe, seq)?;
    let seq = group.push(tx, carrier(id, EnvelopePayload::ProbeSetup(setup)))?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Probe, seq)?;
    let seq = group.push(tx, carrier(id, EnvelopePayload::ProbeState(state)))?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Probe, seq)?;
    let seq = group.push(
        tx,
        carrier(id, EnvelopePayload::ContestPreflight(plan.base.clone())),
    )?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Probe, seq)?;
    if plan.base.confirmation_required {
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(plan.base),
            cursor: seq,
        });
    }
    reserve_plan(tx, &plan, max)?;
    let seq = group.push(
        tx,
        carrier(
            id,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id,
                plan_id,
                status: PreflightStatus::Confirmed,
                confirmation: Some(if preconfirmed {
                    Confirmation::Preconfirmed
                } else {
                    Confirmation::Automatic
                }),
                error: None,
            }),
        ),
    )?;
    squad_store::contests::associate(tx, contest_id, ContestKind::Probe, seq)?;
    Ok(group.accepted(id))
}
fn reserve_plan(tx: &Connection, plan: &ProbePreflight, max: i64) -> Result<()> {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let key = format!("plan:{}", plan.base.plan_id);
    let amount = squad_store::accounting::reservation_micros(plan.base.estimate.reserved_usd)
        .map_err(|_| invalid())?;
    squad_store::accounting::reserve(tx, &key, None, &day, amount, max)?;
    tx.execute(
        "UPDATE reservations SET contest_id=?,plan_id=? WHERE reservation_id=?",
        params![
            plan.setup.contest_id.to_string(),
            plan.base.plan_id.to_string(),
            key
        ],
    )?;
    Ok(())
}
pub fn resolve(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan: PlanId,
    cancel: bool,
    max: i64,
) -> Result<RequestResult> {
    let mut state = squad_store::probe::state(tx, contest)?;
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=? AND plan_id=?",
        params![contest.to_string(), plan.to_string()],
        |r| r.get(0),
    )?;
    let preflight: ProbePreflight = serde_json::from_str(&raw)?;
    if state.state != ProbeStatus::Preflight {
        return Err(invalid());
    }
    state.state_version += 1;
    state.state = if cancel {
        ProbeStatus::Stopped
    } else if group.accepted_at >= preflight.base.expires_at {
        ProbeStatus::Failed
    } else {
        reserve_plan(tx, &preflight, max)?;
        ProbeStatus::WaitingForParticipants
    };
    save(tx, group, id, &state)?;
    if state.state.is_terminal() {
        return Ok(group.accepted(id));
    }
    let status = if cancel {
        PreflightStatus::Cancelled
    } else if state.state == ProbeStatus::Failed {
        PreflightStatus::Expired
    } else {
        PreflightStatus::Confirmed
    };
    let seq = group.push(
        tx,
        carrier(
            id,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: contest,
                plan_id: plan,
                status: status.clone(),
                confirmation: (status == PreflightStatus::Confirmed)
                    .then_some(Confirmation::Explicit),
                error: None,
            }),
        ),
    )?;
    squad_store::contests::associate(tx, contest, ContestKind::Probe, seq)?;
    Ok(group.accepted(id))
}
pub fn input_for(db: &Connection, state: &ProbeState, target: &Target) -> Result<ProbeInput> {
    squad_store::probe::stage_input(db, state, target)
}
pub(crate) fn actors(db: &Connection, state: &ProbeState) -> Result<Vec<(Target, WorkKind)>> {
    let setup = squad_store::probe::setup(db, state.contest_id)?;
    match state.state {
        ProbeStatus::WaitingForParticipants | ProbeStatus::Proposing => Ok(setup
            .participants
            .iter()
            .map(|n| {
                (
                    Target::Member(n.clone()),
                    if setup.explainers.contains(n) {
                        WorkKind::ProbeExplain
                    } else {
                        WorkKind::ProbeCheck
                    },
                )
            })
            .collect()),
        ProbeStatus::Predicting => {
            let mut actors = vec![];
            for n in &setup.explainers {
                let accepted:bool=db.query_row("SELECT initial_event_seq IS NOT NULL FROM probe_roles WHERE contest_id=? AND nick=?",params![state.contest_id.to_string(),n.as_str()],|r|r.get(0))?;
                if accepted {
                    actors.push((Target::Member(n.clone()), WorkKind::ProbePredict));
                }
            }
            Ok(actors)
        }
        ProbeStatus::Reporting => Ok(vec![(Target::Judge, WorkKind::ProbeReport)]),
        _ => Err(invalid()),
    }
}
/// The hook derives its own input projections instead of trusting arbitrary text.
pub fn allocate(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    step: ContestStep,
    work: Vec<Work>,
    _bindings: &BTreeMap<String, ExecutorBinding>,
) -> Result<RequestResult> {
    let mut state = squad_store::probe::state(tx, step.contest_id)?;
    if state.step_id.is_some() || state.state.is_terminal() {
        return Err(invalid());
    }
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [state.contest_id.to_string()],
        |r| r.get(0),
    )?;
    let plan: ProbePreflight = serde_json::from_str(&raw)?;
    let bindings: BTreeMap<String, ExecutorBinding> =
        serde_json::from_value(plan.base.settings["bindings"].clone())?;
    let actors = actors(tx, &state)?;
    if work.len() != actors.len()
        || step.assignments.len() != actors.len()
        || step.script_version.as_str() != "probe-v1"
        || step.ordinal
            != match state.state {
                ProbeStatus::WaitingForParticipants | ProbeStatus::Proposing => 1,
                ProbeStatus::Predicting => 2,
                ProbeStatus::Reporting => 3,
                _ => return Err(invalid()),
            }
    {
        return Err(invalid());
    }
    let seq = group.push(tx, carrier(id, EnvelopePayload::ContestStep(step.clone())))?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Probe, seq)?;
    let source = squad_store::events::at(tx, seq.get())?;
    for mut w in work {
        if !actors.contains(&(w.target.clone(), w.kind.clone()))
            || w.contest_id != Some(state.contest_id)
            || w.step_id != Some(step.step_id)
        {
            return Err(invalid());
        }
        let composition = w.kind.probe_composition().ok_or_else(invalid)?;
        let prompt = w.prompt_ref.as_ref().ok_or_else(invalid)?;
        if prompt.template_id.as_str() != composition
            || Some(prompt.template_hash.as_str())
                != plan.base.settings["prompts"][composition]["hash"].as_str()
            || Some(prompt.pack_version.as_str()) != plan.base.settings["packVersion"].as_str()
        {
            return Err(invalid());
        }
        let input = input_for(tx, &state, &w.target)?;
        if w.target == Target::Judge {
            let case = w.case_id.ok_or_else(invalid)?;
            let ProbeInput::Report { material } = &input else {
                return Err(invalid());
            };
            let binding = bindings.get("judge").ok_or_else(invalid)?;
            let opened = CaseOpened {
                case_id: case,
                judge_invocation_id: EventId::new(),
                work_id: Some(w.work_id),
                judge_model: binding.model.clone(),
                material: CaseMaterial::Probe(material.clone()),
                debate_id: None,
                contest_id: Some(state.contest_id),
            };
            let seq = group.push(tx, carrier(id, EnvelopePayload::CaseOpened(opened.clone())))?;
            squad_store::cases::open(tx, &opened, seq)?;
        }
        let projection = HistoryProjection {
            context_id: w.context_id.clone(),
            origin_event_id: source.base.id,
            projection_key: Text::new("probe.input").expect("literal"),
            role: HistoryRole::User,
            name: Some("squad".into()),
            content: serde_json::to_string(&input)?,
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        let pos = squad_store::history::project(tx, &projection)?;
        w.history_through = pos.ok_or_else(invalid)?;
        let mut binding = bindings
            .get(&w.target.to_string())
            .ok_or_else(invalid)?
            .clone();
        if w.kind == WorkKind::ProbeReport {
            binding.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
        }
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step.step_id, &w)?;
        // The one-shot dispatcher admits later steps of an admitted contest from
        // their queued announcement; without it `send` never ran the predictions.
        group.push(
            tx,
            carrier(
                id,
                EnvelopePayload::WorkState(WorkStateEvent {
                    work_id: w.work_id,
                    trigger_event_id: w.trigger_event_id,
                    target: w.target.clone(),
                    context_id: w.context_id.clone(),
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
    state.step_id = Some(step.step_id);
    if state.state == ProbeStatus::WaitingForParticipants {
        state.state = ProbeStatus::Proposing;
    }
    state.state_version += 1;
    save(tx, group, id, &state)?;
    Ok(group.accepted(id))
}
fn events(db: &Connection, id: ContestId) -> Result<Vec<CommittedEnvelope>> {
    let mut q =
        db.prepare("SELECT event_seq FROM mode_artifacts WHERE contest_id=? ORDER BY event_seq")?;
    q.query_map([id.to_string()], |r| r.get::<_, i64>(0))?
        .map(|r| squad_store::events::at(db, r?))
        .collect()
}
/// Barrier effects and the next lifecycle snapshot commit together. Retry is a receipt.
pub fn barrier(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    commit: BarrierCommit,
) -> Result<()> {
    let mut state = squad_store::probe::state(tx, commit.contest_id)?;
    if state.state_version != commit.expected_state_version
        || state.step_id != Some(commit.step_id)
        || state.state.is_terminal()
        || !commit.next_work.is_empty()
        || !commit.projections.is_empty()
    {
        return Err(invalid());
    }
    let mut q=tx.prepare("SELECT a.target,a.work_id,w.status FROM contest_assignments a JOIN work w ON w.work_id=a.work_id WHERE a.step_id=? AND a.outcome='pending'")?;
    let rows = q
        .query_map([commit.step_id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (target, work_id, status) in rows {
        if !matches!(status.as_str(), "failed" | "cancelled") {
            return Err(invalid());
        }
        state
            .skipped_work_ids
            .push(work_id.parse().map_err(|_| invalid())?);
        if target != "judge" {
            let nick: MemberNick = target.parse().map_err(|_| invalid())?;
            if !state.skipped_members.contains(&nick) {
                state.skipped_members.push(nick);
            }
        }
        let target = if target == "judge" {
            Target::Judge
        } else {
            Target::Member(target.parse().map_err(|_| invalid())?)
        };
        squad_store::contest_steps::settle(
            tx,
            commit.step_id,
            &target,
            Some(&error(
                ErrorCode::PROVIDER_ERROR,
                "Probe work missing after attempts exhausted",
            )),
        )?;
    }
    let accepted = events(tx, state.contest_id)?;
    let setup = squad_store::probe::setup(tx, state.contest_id)?;
    match state.state {
        ProbeStatus::Proposing => {
            let mut material = ProbeCaseMaterial {
                brief: setup.brief,
                explanations: vec![],
                checks: vec![],
                prediction_matrix: BTreeMap::new(),
                missing_rows: vec![],
                skipped_members: vec![],
            };
            let mut owners = BTreeMap::new();
            for e in &accepted {
                match &e.content {
                    EnvelopePayload::ProbeExplanation(v) => {
                        let Actor::Member(n) = &e.base.sender else {
                            return Err(invalid());
                        };
                        let item = CellId::new();
                        owners.insert(n.clone(), item);
                        material.explanations.push(ProbeItem {
                            id: item,
                            data: v.data.clone(),
                        });
                    }
                    EnvelopePayload::ProbeCheck(v) => material.checks.push(ProbeItem {
                        id: CellId::new(),
                        data: v.data.clone(),
                    }),
                    _ => {}
                }
            }
            // Random opaque order decouples delivery from source acceptance order.
            material.checks.sort_by_key(|i| i.id.to_string());
            material.explanations.sort_by_key(|i| i.id.to_string());
            for n in &setup.participants {
                let missing:bool=tx.query_row("SELECT initial_event_seq IS NULL FROM probe_roles WHERE contest_id=? AND nick=?",params![state.contest_id.to_string(),n.as_str()],|r|r.get(0))?;
                if missing {
                    material.skipped_members.push(n.clone());
                }
            }
            // Human originals stay in canonical events. Only the anonymous view is redacted.
            let tokens:Vec<String>=serde_json::from_str(&tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='probe.redaction'",[state.contest_id.to_string()],|r|r.get::<_,String>(0))?).map(|v:serde_json::Value|serde_json::from_value(v["tokens"].clone())).map_err(StoreError::from)??;
            redact_material(&mut material, &tokens);
            for check in &mut material.checks {
                for (i, outcome) in check.data.outcomes.iter_mut().enumerate() {
                    outcome.id = Text::new(format!("outcome{}", i + 1)).expect("literal");
                }
            }
            squad_store::contest_steps::private_map(
                tx,
                state.contest_id,
                "probe.explanationOwners",
                &serde_json::to_value(owners)?,
            )?;
            squad_store::contest_steps::private_map(
                tx,
                state.contest_id,
                "probe.material",
                &serde_json::to_value(&material)?,
            )?;
            state.state = if material.explanations.is_empty() || material.checks.is_empty() {
                ProbeStatus::Failed
            } else {
                ProbeStatus::Predicting
            };
            if state.state == ProbeStatus::Failed {
                state.terminal_reason =
                    Some(Text::new("Insufficient usable explanations or checks").expect("literal"));
            }
        }
        ProbeStatus::Predicting => {
            let mut material = squad_store::probe::material(tx, state.contest_id)?;
            let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='probe.explanationOwners'",[state.contest_id.to_string()],|r|r.get(0))?;
            let owners: BTreeMap<MemberNick, CellId> = serde_json::from_str(&raw)?;
            for e in &material.explanations {
                material.prediction_matrix.insert(e.id, None);
            }
            for e in &accepted {
                if let EnvelopePayload::ProbePrediction(p) = &e.content {
                    let Actor::Member(nick) = &e.base.sender else {
                        return Err(invalid());
                    };
                    let item = owners.get(nick).ok_or_else(invalid)?;
                    material
                        .prediction_matrix
                        .insert(*item, Some(p.data.clone()));
                }
            }
            let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='probe.redaction'",[state.contest_id.to_string()],|r|r.get(0))?;
            let tokens: Vec<String> = serde_json::from_value(
                serde_json::from_str::<serde_json::Value>(&raw)?["tokens"].clone(),
            )?;
            redact_material(&mut material, &tokens);
            material.missing_rows = material
                .prediction_matrix
                .iter()
                .filter_map(|(id, row)| row.is_none().then_some(*id))
                .collect();
            tx.execute("UPDATE contest_private_maps SET map_json=? WHERE contest_id=? AND scope_key='probe.material'",params![serde_json::to_string(&material)?,state.contest_id.to_string()])?;
            state.state = ProbeStatus::Reporting;
        }
        ProbeStatus::Reporting => {
            state.state = if accepted
                .iter()
                .any(|e| matches!(e.content, EnvelopePayload::ProbeReport(_)))
            {
                ProbeStatus::Complete
            } else {
                ProbeStatus::Failed
            };
        }
        _ => return Err(invalid()),
    }
    state.artifact_event_ids = accepted.iter().map(|e| e.base.id).collect();
    let seq = group.push(
        tx,
        carrier(
            id,
            EnvelopePayload::ContestReport(ContestReport {
                contest_id: state.contest_id,
                step_id: commit.step_id,
                artifact_event_ids: state.artifact_event_ids.clone(),
                skipped_work_ids: state.skipped_work_ids.clone(),
                data: serde_json::json!({"probeBarrier":true}),
            }),
        ),
    )?;
    squad_store::contest_steps::barrier(tx, commit.step_id, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Probe, seq)?;
    state.step_id = None;
    state.state_version += 1;
    save(tx, group, id, &state)
}
pub fn redact(text: &str, tokens: &[String]) -> String {
    let mut result = String::new();
    let mut offset = 0;
    while offset < text.len() {
        let rest = &text[offset..];
        let previous = text[..offset].chars().next_back();
        let found = tokens
            .iter()
            .filter(|t| !t.is_empty())
            .filter_map(|token| {
                let end = rest
                    .char_indices()
                    .nth(token.chars().count())
                    .map(|(i, _)| i)
                    .unwrap_or(rest.len());
                let candidate = &rest[..end];
                let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
                (candidate.to_lowercase() == token.to_lowercase()
                    && boundary(previous)
                    && boundary(rest[end..].chars().next()))
                .then_some(end)
            })
            .max();
        if let Some(len) = found {
            result.push_str("[redacted]");
            offset += len;
        } else {
            let c = rest.chars().next().expect("nonempty");
            result.push(c);
            offset += c.len_utf8();
        }
    }
    result
}

fn redact_material(material: &mut ProbeCaseMaterial, tokens: &[String]) {
    let t = |value: &mut Text| {
        *value = Text::new(redact(value.as_str(), tokens)).expect("nonempty redaction")
    };
    t(&mut material.brief.observation);
    material.brief.ruled_out = redact(&material.brief.ruled_out, tokens);
    t(&mut material.brief.inspectable);
    for e in &mut material.explanations {
        t(&mut e.data.explanation);
        t(&mut e.data.key_assumption);
        for l in &mut e.data.limitations {
            t(l);
        }
    }
    for c in &mut material.checks {
        t(&mut c.data.check);
        t(&mut c.data.procedure);
        t(&mut c.data.measurement);
        t(&mut c.data.effort);
        for o in &mut c.data.outcomes {
            t(&mut o.description);
        }
        for s in &mut c.data.safety_notes {
            t(s);
        }
    }
    for row in material.prediction_matrix.values_mut().flatten() {
        for cell in &mut row.predictions {
            if let Some(value) = &mut cell.expected_observation {
                t(value);
            }
            t(&mut cell.reason);
        }
    }
}

pub fn transition(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let mut state = squad_store::probe::state(tx, submission.contest_id)?;
    if state.state_version != submission.expected_state_version || state.state.is_terminal() {
        return Err(invalid());
    }
    let ContestTransition::Probe(transition) = submission.transition else {
        return Err(invalid());
    };
    state.state = match transition {
        ProbeTransition::Stop => ProbeStatus::Stopped,
        ProbeTransition::Fail => ProbeStatus::Failed,
        _ => return Err(invalid()), // Model-step transitions require the durable barrier hook.
    };
    state.terminal_reason = Some(
        Text::new(if state.state == ProbeStatus::Stopped {
            "Stopped"
        } else {
            "Probe could not continue"
        })
        .expect("literal"),
    );
    state.state_version += 1;
    save(tx, group, id, &state)?;
    tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[state.contest_id.to_string()])?;
    Ok(())
}

/// Include complete provider names and short families such as o3, without
/// splitting a provider like x-ai into unrelated one-letter prose tokens.
pub fn blind_tokens(mut tokens: Vec<String>) -> Vec<String> {
    let mut derived = Vec::new();
    for token in &tokens {
        if let Some((provider, model)) = token.trim_start_matches('~').split_once('/') {
            derived.push(provider.to_owned());
            derived.extend(
                model
                    .split(['-', '_', '.', ':'])
                    .filter(|part| part.chars().any(char::is_alphabetic))
                    .map(str::to_owned),
            );
        }
    }
    tokens.extend(derived);
    tokens.retain(|t| !t.is_empty());
    tokens.sort();
    tokens.dedup();
    tokens
}
