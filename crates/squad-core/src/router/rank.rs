//! Transactional rank admission, assignments, barriers and human continuation.
use super::envelope;
use crate::{orchestrator::rank as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use squad_protocol::rank::*;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup, rank::Run};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid rank transition",
    )
}
pub fn load_state(db: &Connection, id: ContestId) -> Result<RankState> {
    let s: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='rank'",
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
        EnvelopePayload::RankSetup(s) => s.contest_id,
        EnvelopePayload::ContestStep(s) => s.contest_id,
        EnvelopePayload::ContestReport(s) => s.contest_id,
        EnvelopePayload::ContestPreflight(s) => s.contest_id,
        EnvelopePayload::ContestPreflightResult(s) => s.contest_id,
        _ => return Err(invalid()),
    };
    let seq = g.push(tx, envelope(id, Actor::System, vec![Recipient::Me], p))?;
    squad_store::contests::associate(tx, contest, ContestKind::Rank, seq)?;
    Ok(seq)
}
pub fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &RankState,
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
            EnvelopePayload::RankState(s.clone()),
        ),
    )?;
    if expected.is_none() {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'rank',(SELECT epoch FROM room_meta),?,?,1,?)",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,seq.get()])?;
    } else {
        tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=?,expires_at=CASE WHEN ? THEN NULL ELSE expires_at END WHERE contest_id=?",params![s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get(),s.state.is_terminal(),s.contest_id.to_string()])?;
    }
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Rank, seq)?;
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
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::Rank {
        participants,
        criterion,
        items,
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
    script::prepare(parsed).map_err(|_| invalid())?;
    if participants.len() < 3 {
        return Err(invalid());
    }
    let sources = vec![];
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let names = ["rank.compare", "rank.explain"];
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
    let seed = rand::random::<u64>();
    let (pairs, votes_per_pair, deal) = script::design(items.len(), participants, seed);
    let setup = RankSetup {
        criterion: criterion.clone(),
        items: items
            .iter()
            .enumerate()
            .map(|(i, text)| RankItem {
                item_id: format!("I{}", i + 1),
                text: text.clone(),
            })
            .collect(),
        pairs,
        votes_per_pair,
        deal,
        participants: participants.clone(),
        room_words: config.prompts.limits.room_words,
        models,
        prompt_refs: refs,
    };
    setup.validate().map_err(|_| invalid())?;
    let templates = names
        .into_iter()
        .map(|n| super::bounds::template(&pack, n))
        .collect::<Result<Vec<_>>>()?;
    let priced = super::bounds::price(tx, &setup.models, &steps(&setup, &templates)?)?;
    let contest = ContestId::new();
    let state = RankState {
        contest_id: contest,
        state_version: 1,
        state: RankStatus::Preflight,
        round: 0,
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &state, None)?;
    squad_store::rank::insert(tx, contest, &setup)?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "rank.seed",
        &serde_json::json!({"seed":seed}),
    )?;
    let tokens = config
        .agents
        .iter()
        .flat_map(|a| [a.nick.to_string(), a.model.clone(), a.persona.clone()])
        .chain(std::iter::once(config.judge.model.clone()))
        .collect::<Vec<_>>();
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "rank.prompts",
        &serde_json::json!({"repairTemplate":pack.template("mode.repair").map_err(|_|invalid())?,"compositions":frozen,"tokens":tokens,"personas":config.agents.iter().map(|a|(a.nick.to_string(),a.persona.clone())).collect::<BTreeMap<_,_>>(),"userName":config.user.name,"roster":participants}),
    )?;
    push(
        tx,
        g,
        id,
        EnvelopePayload::RankSetup(ContestSetup {
            contest_id: contest,
            kind: ContestKind::Rank,
            script_version: "rank-v1".parse().expect("version"),
            participants: participants.clone(),
            source_event_ids: sources,
            settings: serde_json::json!({"criterion":setup.criterion,"items":setup.items}),
        }),
    )?;
    super::variables::apply(tx, g, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    let (calls, attempts) = script::call_bounds(participants.len() as u32);
    settings["reservedAttempts"] = attempts.into();
    settings["memberCalls"] = (participants.len() as u32).into();
    settings["judgeCalls"] = 1.into();
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Rank,
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
        s.state = RankStatus::Failed;
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
    if state.state != RankStatus::Preflight || plan.plan_id != plan_id {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let version = state.state_version;
    state.state_version += 1;
    if cancel || expired {
        state.state = RankStatus::Stopped;
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
        state.state = RankStatus::WaitingForParticipants;
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
        || rank::composition(&work.kind) != Some(artifact.artifact_kind.as_str())
    {
        return Err(invalid());
    }
    let data = material(db, work)?;
    let output = artifact.body.as_ref().ok_or_else(invalid)?;
    script::validate_output(&work.kind, &data["data"], output).map_err(|_| invalid())?;
    if rank::prose_words(output) > data["wordLimit"].as_u64().ok_or_else(invalid)? as usize {
        return Err(invalid());
    }
    squad_store::contest_steps::artifact(db, artifact, seq)
}

pub fn steps(setup: &RankSetup, templates: &[u64]) -> Result<Vec<super::bounds::Step>> {
    use super::bounds::{Step, artifact, blinded, json_text, limited, schema_bytes};
    let mut out = vec![];
    for nick in &setup.participants {
        out.push(Step {
            kind: WorkKind::RankCompare,
            target: nick.to_string(),
            alternatives: vec![],
            template: templates[0],
            schema: schema_bytes(rank::output_schema(&WorkKind::RankCompare).expect("schema")),
            data: json_text(&setup.criterion) + 120 * limited(2048, 60) + 8192,
            visible: 4096,
            calls: 1,
            attempts: 2,
        });
    }
    out.push(Step {
        kind: WorkKind::RankExplain,
        target: "judge".into(),
        alternatives: vec![],
        template: templates[1],
        schema: schema_bytes(rank::output_schema(&WorkKind::RankExplain).expect("schema")),
        data: blinded(&setup.criterion)
            + setup
                .items
                .iter()
                .map(|i| 2 * json_text(&i.text))
                .sum::<u64>()
            + artifact(4096)
            + 40 * limited(2048, 40),
        visible: 4096,
        calls: 1,
        attempts: 3,
    });
    Ok(out)
}
pub fn human_answer(
    _db: &Connection,
    _pending: &parser::PendingHuman,
    _complete: bool,
) -> Result<()> {
    Err(invalid())
}
pub fn recover(db: &Connection, id: ContestId) -> Result<()> {
    load_state(db, id)?.validate().map_err(|_| invalid())?;
    squad_store::rank::setup(db, id)?
        .validate()
        .map_err(|_| invalid())
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
fn artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut RankState,
    step: StepId,
    kind: &str,
    body: serde_json::Value,
) -> Result<()> {
    let a = ContestArtifact {
        contest_id: s.contest_id,
        step_id: step,
        artifact_kind: kind.parse().map_err(|_| invalid())?,
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
        kind: Some(ContestKind::Rank),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
    };
    let e = envelope(
        id,
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestArtifact(a.clone()),
    );
    let event = e.base.id;
    let seq = g.push(tx, e)?;
    squad_store::contest_steps::artifact(tx, &a, seq)?;
    s.artifact_event_ids.push(event);
    Ok(())
}
pub fn finish(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut RankState,
    reason: &str,
    failed: bool,
) -> Result<()> {
    let run = squad_store::rank::run(tx, s.contest_id)?;
    let step = if let Some(step) = s.step_id {
        step
    } else {
        let step = ContestStep {
            contest_id: s.contest_id,
            step_id: StepId::new(),
            ordinal: 4,
            script_version: "rank-v1".parse().expect("version"),
            move_name: ContestMove::Report,
            inputs: vec![],
            assignments: vec![],
        };
        let seq = push(tx, g, id, EnvelopePayload::ContestStep(step.clone()))?;
        squad_store::contest_steps::insert(tx, &step, seq)?;
        step.step_id
    };
    let report = RankReport {
        setup: squad_store::rank::setup(tx, s.contest_id)?,
        batches: run.batches,
        tally: run.tally,
        explanation: run.explanation,
        skipped: run.skipped,
        reason: reason.into(),
    };
    artifact(
        tx,
        g,
        id,
        s,
        step,
        "rank.report",
        serde_json::to_value(report)?,
    )?;
    let v = s.state_version;
    s.state_version += 1;
    s.state = if failed {
        RankStatus::Failed
    } else if reason == "stopped" {
        RankStatus::Stopped
    } else {
        RankStatus::Complete
    };
    s.terminal_reason = Some(reason.parse().map_err(|_| invalid())?);
    save(tx, g, id, s, Some(v))
}
pub(crate) fn drive_move(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    mut s: RankState,
) -> Result<RequestResult> {
    if load_state(tx, s.contest_id)? != s || s.state.is_terminal() {
        return Err(invalid());
    }
    let setup = squad_store::rank::setup(tx, s.contest_id)?;
    let mut run = squad_store::rank::run(tx, s.contest_id)?;
    if s.state == RankStatus::Preflight {
        let raw: String = tx.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )?;
        let plan: ContestPreflight = serde_json::from_str(&raw)?;
        if g.accepted_at >= plan.expires_at {
            return resolve(tx, g, id, s.contest_id, plan.plan_id, true);
        }
        return Ok(g.accepted(id));
    }
    let daily:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE contest_id=? AND json_extract(error_json,'$.code')='DAILY_MAX_REACHED')",[s.contest_id.to_string()],|r|r.get(0))?;
    if daily {
        finish(tx, g, id, &mut s, "daily_max_reached", true)?;
        return Ok(g.accepted(id));
    }
    if s.state == RankStatus::WaitingForParticipants {
        for nick in &setup.participants {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[nick.as_str()],|r|r.get(0))?;
            if pending {
                return Ok(g.accepted(id));
            }
        }
        let v = s.state_version;
        s.state = RankStatus::Comparing;
        s.state_version += 1;
        save(tx, g, id, &s, Some(v))?;
        return Ok(g.accepted(id));
    }
    if s.state == RankStatus::Aggregating {
        let tally = script::aggregate(&setup, &run.batches);
        let judged = tally
            .wins
            .iter()
            .enumerate()
            .map(|(i, r)| {
                r.iter()
                    .enumerate()
                    .filter(|(j, w)| i < *j && **w + tally.wins[*j][i] > 0)
                    .count()
            })
            .sum::<usize>();
        if run.batches.len() < 2 || judged < setup.items.len() - 1 {
            finish(tx, g, id, &mut s, "insufficient_comparisons", true)?;
            return Ok(g.accepted(id));
        }
        let step = s.step_id.ok_or_else(invalid)?;
        artifact(
            tx,
            g,
            id,
            &mut s,
            step,
            "rank.tally",
            serde_json::to_value(&tally)?,
        )?;
        run.tally = Some(tally);
        squad_store::rank::save(tx, s.contest_id, &run)?;
        let v = s.state_version;
        s.state = RankStatus::Explaining;
        s.step_id = None;
        s.state_version += 1;
        save(tx, g, id, &s, Some(v))?;
        return Ok(g.accepted(id));
    }
    if s.state == RankStatus::Reporting {
        finish(tx, g, id, &mut s, "ranked", false)?;
        return Ok(g.accepted(id));
    }
    if s.step_id.is_none() {
        allocate(tx, g, id, &s, &setup, &mut run)?;
        return Ok(g.accepted(id));
    }
    let step = s.step_id.ok_or_else(invalid)?;
    let rows = {
        let mut q =
            tx.prepare("SELECT work_id FROM contest_assignments WHERE step_id=? ORDER BY target")?;
        q.query_map([step.to_string()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for raw in rows {
        let work_id: WorkId = raw.parse().map_err(|_| invalid())?;
        let (work, _) = squad_store::work::template(tx, work_id)?.ok_or_else(invalid)?;
        let record = squad_store::work::record(tx, work_id)?.ok_or_else(invalid)?;
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
            if !s.artifact_event_ids.contains(&e.base.id) {
                s.artifact_event_ids.push(e.base.id);
            }
            match work.target {
                Target::Member(n) => {
                    run.batches
                        .insert(n, serde_json::from_value(a.body.ok_or_else(invalid)?)?);
                }
                Target::Judge => {
                    run.explanation = Some(serde_json::from_value(a.body.ok_or_else(invalid)?)?);
                }
            }
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
            if let Target::Member(n) = work.target
                && !run.skipped.contains(&n)
            {
                run.skipped.push(n);
            }
        }
    }
    squad_store::rank::save(tx, s.contest_id, &run)?;
    let seq = push(
        tx,
        g,
        id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: s.contest_id,
            step_id: step,
            artifact_event_ids: s.artifact_event_ids.clone(),
            skipped_work_ids: vec![],
            data: serde_json::json!({"rankBarrier":true}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    let v = s.state_version;
    s.state_version += 1;
    s.state = if s.state == RankStatus::Comparing {
        RankStatus::Aggregating
    } else {
        RankStatus::Reporting
    };
    save(tx, g, id, &s, Some(v))?;
    Ok(g.accepted(id))
}

fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &RankState,
    setup: &RankSetup,
    run: &mut Run,
) -> Result<()> {
    let kind = match s.state {
        RankStatus::Comparing => WorkKind::RankCompare,
        RankStatus::Explaining => WorkKind::RankExplain,
        _ => return Err(invalid()),
    };
    let judge = kind == WorkKind::RankExplain;
    let name = rank::composition(&kind).ok_or_else(invalid)?;
    let words = if judge { setup.room_words } else { 800 };
    let frozen: String = tx.query_row(
        "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='rank.prompts'",
        [s.contest_id.to_string()],
        |r| r.get(0),
    )?;
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
        let context = if judge {
            ContextId(format!("judge:{}", run.case_id))
        } else {
            ContextId(format!("contest:{}:{}:{}", s.contest_id, step_id, target))
        };
        let data = if judge {
            let tally = run.tally.as_ref().ok_or_else(invalid)?;
            // Keep only comparison reasons relevant to top items and displayed cycles.
            let mut involved = std::collections::BTreeSet::new();
            let top = tally.order.iter().take(5).copied().collect::<Vec<_>>();
            for &a in &top {
                for &b in &top {
                    if a != b {
                        involved.insert((a.min(b), a.max(b)));
                    }
                }
            }
            for c in &tally.cycles {
                for (a, b) in [
                    (c.items[0], c.items[1]),
                    (c.items[1], c.items[2]),
                    (c.items[2], c.items[0]),
                ] {
                    involved.insert((a.min(b), a.max(b)));
                }
            }
            let mut reasons = BTreeMap::<String, Vec<String>>::new();
            for (nick, batch) in &run.batches {
                for choice in &batch.choices {
                    let pair = setup.deal[nick]
                        .iter()
                        .find(|d| d.pair_id == choice.pair_id)
                        .ok_or_else(invalid)?;
                    if involved.contains(&(pair.a.min(pair.b), pair.a.max(pair.b))) {
                        reasons
                            .entry(choice.pair_id.clone())
                            .or_default()
                            .push(choice.reason.clone());
                    }
                }
            }
            // Ten top-item pairs plus three edges per displayed cycle: at most forty.
            let reasons = reasons.into_iter().take(40).collect::<BTreeMap<_, _>>();
            let mut value = serde_json::json!({"criterion":setup.criterion,"items":setup.items,"tally":tally,"reasons":reasons,"topIds":tally.order.iter().take(5).map(|i|setup.items[*i].item_id.clone()).collect::<Vec<_>>(),"cycleIds":tally.cycles.iter().map(|c|c.cycle_id.clone()).collect::<Vec<_>>(),"wordLimit":words});
            blind_value(&mut value, &tokens);
            value
        } else {
            let Target::Member(nick) = &target else {
                return Err(invalid());
            };
            serde_json::json!({"criterion":setup.criterion,"pairs":setup.deal[nick].iter().map(|d|serde_json::json!({"pairId":d.pair_id,"A":setup.items[d.a].text,"B":setup.items[d.b].text})).collect::<Vec<_>>()})
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
                rank::output_schema(&kind).ok_or_else(invalid)?.to_string(),
            ),
            ("nick".into(), target.to_string()),
            ("model_slug".into(), binding.model.to_string()),
            ("judge_model_slug".into(), binding.model.to_string()),
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
        let bundle = serde_json::json!({"repairTemplate":frozen["repairTemplate"],"round":s.round,"data":data,"wordLimit":words,"model":model,"rendered":{"system":render("system")?,"user":render("user")?,"provider_user_name":"squad"}});
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
            max_output_tokens: Some(4096),
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
        ordinal: if judge { 2 } else { 1 },
        script_version: "rank-v1".parse().expect("version"),
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
    squad_store::contests::associate(tx, s.contest_id, ContestKind::Rank, seq)?;
    squad_store::contest_steps::insert(tx, &step, seq)?;
    for (mut item, binding, bundle) in items {
        if judge {
            tx.execute("INSERT OR IGNORE INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![run.case_id.to_string(),item.context_id.0,s.contest_id.to_string(),binding.model.as_str(),bundle["data"].to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: item.context_id.clone(),
            origin_event_id: id,
            projection_key: "rank.step".parse().expect("key"),
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
    if submission.kind != ContestKind::Rank
        || s.state_version != submission.expected_state_version
        || !matches!(
            submission.transition,
            ContestTransition::Rank(RankTransition::Advance)
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
pub fn barrier(tx: &Transaction<'_>, g: &mut CommitGroup, _c: BarrierCommit) -> Result<()> {
    let s = load_state(tx, _c.contest_id)?;
    if s.state_version != _c.expected_state_version
        || s.step_id != Some(_c.step_id)
        || !_c.next_work.is_empty()
        || !_c.projections.is_empty()
    {
        return Err(invalid());
    }
    drive_move(tx, g, _c.barrier_event.base.id, s)?;
    Ok(())
}

pub fn blind(text: &str, tokens: &[String]) -> (String, Vec<std::ops::Range<usize>>) {
    let mut tokens = tokens
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    for slug in tokens.clone() {
        if slug.contains('/') {
            tokens.extend(
                slug.split(['/', '-', '_', '.', '~'])
                    .filter(|p| p.chars().any(char::is_alphabetic))
                    .map(str::to_owned),
            );
        }
    }
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    if tokens.is_empty() {
        return (text.into(), (0..text.len()).map(|i| i..i + 1).collect());
    }
    let pattern = format!(
        r"(?i)\b(?:{})\b",
        tokens
            .iter()
            .map(|s| regex::escape(s.trim_start_matches('~')))
            .collect::<Vec<_>>()
            .join("|")
    );
    let regex = regex::Regex::new(&pattern).expect("escaped tokens");
    let mut out = String::new();
    let mut spans = vec![];
    let mut end = 0;
    for m in regex.find_iter(text) {
        out.push_str(&text[end..m.start()]);
        spans.extend((end..m.start()).map(|i| i..i + 1));
        out.push_str("[redacted]");
        spans.extend((0..10).map(|_| m.start()..m.end()));
        end = m.end();
    }
    out.push_str(&text[end..]);
    spans.extend((end..text.len()).map(|i| i..i + 1));
    (out, spans)
}
pub fn blind_value(value: &mut serde_json::Value, tokens: &[String]) {
    match value {
        serde_json::Value::String(s) => *s = blind(s, tokens).0,
        serde_json::Value::Array(a) => {
            for v in a {
                blind_value(v, tokens)
            }
        }
        serde_json::Value::Object(o) => {
            for (key, v) in o {
                if key != "localId" {
                    blind_value(v, tokens)
                }
            }
        }
        _ => {}
    }
}
