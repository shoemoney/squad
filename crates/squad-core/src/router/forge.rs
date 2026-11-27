//! Transactional Forge admission, frozen projections, barriers and recovery.
use crate::{orchestrator::forge as policy, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};
use squad_protocol::forge::*;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup, forge as storage};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid forge transition",
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
    squad_store::contests::associate(tx, c, ContestKind::Forge, seq)?;
    Ok(seq)
}
fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ForgeState,
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
            EnvelopePayload::ForgeState(s.clone()),
        ),
    )?;
    storage::save(tx, s, expected, seq)?;
    if s.state.is_terminal() {
        squad_store::history::release(tx, s.contest_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'",[s.contest_id.to_string()])?;
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[s.contest_id.to_string()])?;
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
pub fn load_state(db: &Connection, c: ContestId) -> Result<ForgeState> {
    storage::state(db, c)
}
pub fn steps(
    participants: &[MemberNick],
    brief: &str,
    constraints: &str,
    templates: &[u64],
    words: (u32, u32),
) -> Vec<super::bounds::Step> {
    use super::bounds::*;
    let m = participants.len() as u64;
    let base = blinded(brief) + blinded(constraints) + 512;
    let draft = project(limited(4096, words.1));
    let notes = project(limited(2048, words.0));
    let make = |kind: WorkKind,
                target: String,
                t: usize,
                data: u64,
                calls: u32,
                attempts: u32,
                alternatives: Vec<String>| Step {
        schema: schema_bytes(output_schema(&kind).expect("forge schema")),
        kind,
        target,
        alternatives,
        template: templates[t],
        data,
        visible: if t == 1 { 2048 } else { 4096 },
        calls,
        attempts,
    };
    let mut out = vec![];
    for n in participants {
        out.push(make(
            WorkKind::ForgeDraft,
            n.to_string(),
            0,
            base,
            1,
            2,
            vec![],
        ));
        out.push(make(
            WorkKind::ForgeRank,
            n.to_string(),
            1,
            base + (m - 1) * draft,
            1,
            2,
            vec![],
        ));
    }
    out.push(make(
        WorkKind::ForgeFuse,
        participants[0].to_string(),
        2,
        base + m * draft + m * notes,
        2,
        2,
        participants
            .iter()
            .skip(1)
            .map(ToString::to_string)
            .collect(),
    ));
    out.push(make(
        WorkKind::ForgeJudge,
        "judge".into(),
        3,
        base + 3 * draft,
        1,
        3,
        vec![],
    ));
    out
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    _input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::Forge {
        input,
        participants,
        brief,
        constraints,
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
    let names = ["forge.draft", "forge.rank", "forge.fuse", "forge.judge"];
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
    let setup = ForgeSetup {
        pack_version: pack.version().to_owned(),
        brief: brief.clone(),
        constraints: constraints.clone().unwrap_or_default(),
        participants: participants.clone(),
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
    if participants.len() < 3 {
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
            brief,
            &setup.constraints,
            &templates,
            (setup.room_words, setup.plan_words),
        ),
    )?;
    let state = ForgeState {
        contest_id: c,
        state_version: 1,
        state: ForgeStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &state, None)?;
    storage::put(tx, c, "setup", &serde_json::to_value(&setup)?)?;
    let ties = participants
        .iter()
        .map(|n| (n.to_string(), CellId::new().to_string()))
        .collect::<BTreeMap<_, _>>();
    storage::put(tx, c, "ties", &json!(ties))?;
    push(
        tx,
        g,
        id,
        c,
        EnvelopePayload::ForgeSetup(ContestSetup {
            contest_id: c,
            kind: ContestKind::Forge,
            script_version: "forge-v1".parse().expect("literal"),
            participants: participants.clone(),
            source_event_ids: vec![],
            settings: json!({"brief":brief,"constraints":setup.constraints}),
        }),
    )?;
    super::variables::apply(tx, g, id, &input.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let mut settings = priced.settings();
    settings["reservedAttempts"] = json!(4 * participants.len() + 7);
    settings["memberCalls"] = json!(2 * participants.len() + 2);
    settings["judgeCalls"] = json!(1);
    let plan = ContestPreflight {
        contest_id: c,
        kind: ContestKind::Forge,
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
            model_calls: 2 * participants.len() as u32 + 3,
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
        s.state = ForgeStatus::Failed;
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
    if s.state != ForgeStatus::Preflight || p != plan.plan_id {
        return Err(invalid());
    }
    let expired = g.accepted_at >= plan.expires_at;
    let old = s.state_version;
    s.state_version += 1;
    if cancel || expired {
        s.state = ForgeStatus::Stopped;
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
        s.state = ForgeStatus::WaitingForParticipants;
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
    s.state = ForgeStatus::Stopped;
    s.state_version += 1;
    s.terminal_reason = Some("stopped".parse().expect("literal"));
    save(tx, g, id, &s, Some(v))
}
pub fn recover(db: &Connection, c: ContestId) -> Result<()> {
    load_state(db, c)?.validate().map_err(|_| invalid())
}
fn drafts(db: &Connection, c: ContestId) -> Result<BTreeMap<MemberNick, ForgeDraft>> {
    storage::artifacts(db, c, "forge.draft")?
        .into_iter()
        .map(|(n, v)| Ok((n, serde_json::from_value(v)?)))
        .collect()
}
fn ballots(db: &Connection, c: ContestId) -> Result<Vec<policy::AcceptedBallot>> {
    storage::artifacts(db, c, "forge.rank")?
        .into_iter()
        .map(|(n, v)| {
            let map = serde_json::from_value(storage::get(db, c, &format!("ballot:{n}"))?)?;
            Ok((n, serde_json::from_value(v)?, map))
        })
        .collect()
}
fn own_artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ForgeState,
    kind: &str,
    body: Value,
) -> Result<()> {
    let a = ContestArtifact {
        kind: Some(ContestKind::Forge),
        round: Some(0),
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
    s: &mut ForgeState,
    reason: &str,
) -> Result<()> {
    s.state = ForgeStatus::Failed;
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
    mut s: ForgeState,
) -> Result<()> {
    if load_state(tx, s.contest_id)? != s {
        return Err(invalid());
    }
    let setup = storage::setup(tx, s.contest_id)?;
    let old = s.state_version;
    if s.state.is_terminal() {
        return Ok(());
    }
    if s.state == ForgeStatus::Preflight {
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
    if s.state == ForgeStatus::WaitingForParticipants {
        for n in &setup.participants {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get::<_,bool>(0))?{return Ok(());}
        }
        s.state = ForgeStatus::Drafting;
        s.state_version += 1;
        return save(tx, g, id, &s, Some(old));
    }
    if s.state == ForgeStatus::Reporting {
        report(tx, g, id, &s)?;
        s.state = ForgeStatus::Complete;
        let r: ForgeReport = serde_json::from_value(storage::get(tx, s.contest_id, "report")?)?;
        s.terminal_reason = Some(r.reason.parse().map_err(|_| invalid())?);
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
        let limit = if work.kind == WorkKind::ForgeJudge {
            3
        } else {
            2
        };
        // Recovery preserves consumed provider attempts. A queued last attempt
        // has no dispatch capacity left and must settle instead of waiting forever.
        if record.status == WorkStatus::Queued && record.attempt >= limit {
            let failure = super::error(
                ErrorCode::PROVIDER_TIMEOUT,
                "Forge attempt interrupted and exhausted",
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
                "Forge slot skipped after exhausted attempts",
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
        ForgeStatus::Drafting => {
            if drafts(tx, s.contest_id)?.len() < 3 {
                return finish(tx, g, id, &mut s, "insufficient_drafts");
            }
            s.state = ForgeStatus::Ranking;
        }
        ForgeStatus::Ranking => {
            let b = ballots(tx, s.contest_id)?;
            if b.len() < 2 {
                return finish(tx, g, id, &mut s, "insufficient_ballots");
            }
            let ties = serde_json::from_value(storage::get(tx, s.contest_id, "ties")?)?;
            let t = policy::tally(&drafts(tx, s.contest_id)?, &b, &ties);
            let v = serde_json::to_value(&t)?;
            storage::put(tx, s.contest_id, "tally", &v)?;
            own_artifact(tx, g, id, &s, "forge.tally", v)?;
            s.state = ForgeStatus::Fusing;
        }
        ForgeStatus::Fusing => {
            s.state = if storage::artifacts(tx, s.contest_id, "forge.fuse")?.is_empty() {
                ForgeStatus::Reporting
            } else {
                ForgeStatus::Judging
            };
        }
        ForgeStatus::Judging => {
            let any:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM contest_artifacts WHERE contest_id=? AND artifact_kind='forge.judge')",[s.contest_id.to_string()],|r|r.get(0))?;
            if !any {
                return finish(tx, g, id, &mut s, "judge_failed");
            }
            s.state = ForgeStatus::Reporting;
        }
        _ => return Err(invalid()),
    }
    // Reporting retains the last committed step as the report's anchor.
    if s.state != ForgeStatus::Reporting {
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
fn data(db: &Connection, s: &ForgeState, setup: &ForgeSetup, target: &Target) -> Result<Value> {
    let c = s.contest_id;
    let ds = drafts(db, c)?;
    let mut v = json!({"brief":policy::blind(&setup.brief,setup),"constraints":policy::blind(&setup.constraints,setup),"wordLimit":if s.state==ForgeStatus::Ranking||s.state==ForgeStatus::Judging{setup.room_words}else{setup.plan_words}});
    match s.state {
        ForgeStatus::Drafting => {}
        ForgeStatus::Ranking => {
            let Target::Member(n) = target else {
                return Err(invalid());
            };
            let map = policy::labels(ds.keys().filter(|a| *a != n).cloned());
            storage::put(db, c, &format!("ballot:{n}"), &json!(map))?;
            v["drafts"] = json!(
                map.iter()
                    .map(|(l, n)| (l.clone(), policy::blind(&ds[n].body, setup)))
                    .collect::<BTreeMap<_, _>>()
            );
            v["labels"] = json!(map.keys().collect::<Vec<_>>());
        }
        ForgeStatus::Fusing => {
            // The shared map is generated once before dispatch, never once per fuser.
            let map: BTreeMap<String, MemberNick> =
                serde_json::from_value(storage::get(db, c, "fusionLabels")?)?;
            let tally: ForgeTally = serde_json::from_value(storage::get(db, c, "tally")?)?;
            v["drafts"] = json!(
                map.iter()
                    .map(|(l, n)| (l.clone(), policy::blind(&ds[n].body, setup)))
                    .collect::<BTreeMap<_, _>>()
            );
            v["labels"] = json!(map.keys().collect::<Vec<_>>());
            v["maxWords"] = json!(tally.max_words);
            let mut notes = vec![];
            for (_, b, bm) in ballots(db, c)? {
                for mut note in b.notes {
                    note.label = map
                        .iter()
                        .find(|(_, n)| **n == bm[&note.label])
                        .ok_or_else(invalid)?
                        .0
                        .clone();
                    note.steal = policy::blind(&note.steal, setup);
                    note.fatal_flaw = note.fatal_flaw.map(|f| policy::blind(&f, setup));
                    if !notes.contains(&note) {
                        notes.push(note);
                    }
                }
            }
            let mut steals = vec![];
            let mut flaws = vec![];
            for note in notes {
                let steal = json!({"label":note.label,"text":note.steal});
                if !steals.contains(&steal) {
                    steals.push(steal);
                }
                if let Some(f) = note.fatal_flaw {
                    let flaw = json!({"label":note.label,"text":f});
                    if !flaws.contains(&flaw) {
                        flaws.push(flaw);
                    }
                }
            }
            v["steals"] = json!(steals);
            v["flaws"] = json!(flaws);
        }
        ForgeStatus::Judging => {
            let candidates = storage::get(db, c, "candidates")?;
            v["candidates"] = candidates["bodies"].clone();
            v["labels"] = json!(
                candidates["bodies"]
                    .as_object()
                    .ok_or_else(invalid)?
                    .keys()
                    .collect::<Vec<_>>()
            );
        }
        _ => return Err(invalid()),
    }
    Ok(v)
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &ForgeState,
    setup: &ForgeSetup,
) -> Result<()> {
    let kind = match s.state {
        ForgeStatus::Drafting => WorkKind::ForgeDraft,
        ForgeStatus::Ranking => WorkKind::ForgeRank,
        ForgeStatus::Fusing => WorkKind::ForgeFuse,
        ForgeStatus::Judging => WorkKind::ForgeJudge,
        _ => return Err(invalid()),
    };
    let ds = drafts(tx, s.contest_id)?;
    let targets = match s.state {
        ForgeStatus::Drafting => setup
            .participants
            .iter()
            .cloned()
            .map(Target::Member)
            .collect::<Vec<_>>(),
        ForgeStatus::Ranking => ds.keys().cloned().map(Target::Member).collect(),
        ForgeStatus::Fusing => {
            storage::put(
                tx,
                s.contest_id,
                "fusionLabels",
                &json!(policy::labels(ds.keys().cloned())),
            )?;
            let t: ForgeTally = serde_json::from_value(storage::get(tx, s.contest_id, "tally")?)?;
            t.fusers.into_iter().map(Target::Member).collect()
        }
        ForgeStatus::Judging => {
            let tally: ForgeTally =
                serde_json::from_value(storage::get(tx, s.contest_id, "tally")?)?;
            let mut candidates = vec![(
                CellId::new().to_string(),
                tally.best_original.clone(),
                false,
                ds[&tally.best_original].body.clone(),
            )];
            for (n, v) in storage::artifacts(tx, s.contest_id, "forge.fuse")? {
                let fusion: ForgeFusion = serde_json::from_value(v)?;
                candidates.push((CellId::new().to_string(), n, true, fusion.body));
            }
            candidates.sort_by(|a, b| a.0.cmp(&b.0));
            let mut bodies = serde_json::Map::new();
            let mut identities = serde_json::Map::new();
            for (i, (_, n, f, b)) in candidates.into_iter().enumerate() {
                let label = format!("C{}", i + 1);
                bodies.insert(label.clone(), json!(policy::blind(&b, setup)));
                identities.insert(label, json!({"author":n,"fusion":f}));
            }
            storage::put(
                tx,
                s.contest_id,
                "candidates",
                &json!({"bodies":bodies,"identities":identities}),
            )?;
            vec![Target::Judge]
        }
        _ => return Err(invalid()),
    };
    let step_id = StepId::new();
    let judge = kind == WorkKind::ForgeJudge;
    let name = storage::composition(&kind).ok_or_else(invalid)?;
    let mut items = vec![];
    let mut assignments = vec![];
    for target in targets {
        let case = judge.then(CaseId::new);
        let context = ContextId(if let Some(case) = case {
            format!("judge:{case}")
        } else {
            format!("contest:{}:{}:{}", s.contest_id, step_id, target)
        });
        let mut binding = setup
            .models
            .get(&target.to_string())
            .ok_or_else(invalid)?
            .clone();
        if judge {
            binding.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
        }
        let source = &setup.prompts[name];
        let prompt = PromptRef {
            pack_version: setup.pack_version.parse().map_err(|_| invalid())?,
            template_id: name.parse().expect("literal"),
            template_hash: squad_store::commit::digest(source)?
                .parse()
                .map_err(|_| invalid())?,
        };
        let d = data(tx, s, setup, &target)?;
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
            max_output_tokens: Some(if kind == WorkKind::ForgeRank {
                2048
            } else {
                4096
            }),
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
    let step = ContestStep {
        contest_id: s.contest_id,
        step_id,
        ordinal: match s.state {
            ForgeStatus::Drafting => 1,
            ForgeStatus::Ranking => 2,
            ForgeStatus::Fusing => 3,
            _ => 4,
        },
        script_version: "forge-v1".parse().expect("literal"),
        move_name: if judge {
            ContestMove::Judge
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
    let mut anchored = s.clone();
    anchored.step_id = Some(step_id);
    match s.state {
        ForgeStatus::Ranking => {
            for (w, _, _) in &items {
                let scope = format!("ballot:{}", w.target);
                let labels = storage::get(tx, s.contest_id, &scope)?;
                own_artifact(
                    tx,
                    g,
                    id,
                    &anchored,
                    "forge.labels",
                    json!({"scope":scope,"labels":labels}),
                )?;
            }
        }
        ForgeStatus::Fusing => {
            own_artifact(
                tx,
                g,
                id,
                &anchored,
                "forge.labels",
                json!({"scope":"fusionLabels","labels":storage::get(tx,s.contest_id,"fusionLabels")?}),
            )?;
        }
        ForgeStatus::Judging => {
            let v = storage::get(tx, s.contest_id, "candidates")?;
            let labels = v["identities"]
                .as_object()
                .ok_or_else(invalid)?
                .iter()
                .map(|(k, v)| (k.clone(), v["author"].clone()))
                .collect::<serde_json::Map<_, _>>();
            own_artifact(
                tx,
                g,
                id,
                &anchored,
                "forge.labels",
                json!({"scope":"candidates","labels":labels}),
            )?;
        }
        _ => {}
    }
    for (mut w, binding, input) in items {
        if let Some(case) = w.case_id {
            tx.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![case.to_string(),w.context_id.0,s.contest_id.to_string(),binding.model.as_str(),input.to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: w.context_id.clone(),
            origin_event_id: id,
            projection_key: "forge.step".parse().expect("literal"),
            role: HistoryRole::User,
            name: Some("squad".into()),
            content: input.to_string(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        w.history_through = squad_store::history::project(tx, &projection)?.ok_or_else(invalid)?;
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step_id, &w)?;
        tx.execute(
            "INSERT INTO forge_inputs VALUES (?,?)",
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
fn report(tx: &Transaction<'_>, g: &mut CommitGroup, id: EventId, s: &ForgeState) -> Result<()> {
    let c = s.contest_id;
    let ds = drafts(tx, c)?;
    let tally: ForgeTally = serde_json::from_value(storage::get(tx, c, "tally")?)?;
    let fusions = storage::artifacts(tx, c, "forge.fuse")?
        .into_iter()
        .map(|(n, v)| Ok((n, serde_json::from_value::<ForgeFusion>(v)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut verdict = None;
    for e in storage::events(tx, c)? {
        if let EnvelopePayload::ContestArtifact(a) = e.content
            && a.artifact_kind.as_str() == "forge.judge"
        {
            verdict = Some(serde_json::from_value::<ForgeVerdict>(
                a.body.ok_or_else(invalid)?,
            )?);
        }
    }
    let (winner, fusion) = if let Some(v) = &verdict {
        let candidates = storage::get(tx, c, "candidates")?;
        let identity = &candidates["identities"][&v.ranking[0]];
        (
            serde_json::from_value(identity["author"].clone())?,
            identity["fusion"].as_bool().ok_or_else(invalid)?,
        )
    } else {
        (tally.best_original.clone(), false)
    };
    let reason = if fusion {
        "fusion_won"
    } else if fusions.is_empty() {
        "no_fusion"
    } else {
        "original_won"
    };
    let (body, provenance, handled_flaws) = if fusion {
        let f = &fusions[&winner];
        (
            f.body.clone(),
            f.provenance.clone(),
            f.handled_flaws.clone(),
        )
    } else {
        (ds[&winner].body.clone(), vec![], vec![])
    };
    let sources: BTreeMap<String, MemberNick> =
        serde_json::from_value(storage::get(tx, c, "fusionLabels")?)?;
    let mut maps = serde_json::Map::new();
    let mut notes = vec![];
    for (n, b, map) in ballots(tx, c)? {
        maps.insert(n.to_string(), json!(map));
        for mut note in b.notes {
            note.label = sources
                .iter()
                .find(|(_, author)| **author == map[&note.label])
                .ok_or_else(invalid)?
                .0
                .clone();
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
    }
    let mut skipped = vec![];
    let mut q=tx.prepare("SELECT a.target,s.ordinal FROM contest_assignments a JOIN contest_steps s USING(step_id) WHERE s.contest_id=? AND a.outcome='skipped' ORDER BY s.ordinal,a.target")?;
    for row in q.query_map([c.to_string()], |r| {
        Ok(format!(
            "{}: step {}",
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?
        ))
    })? {
        skipped.push(row?);
    }
    let r = ForgeReport {
        winner,
        reason: reason.into(),
        body,
        tally,
        provenance,
        sources,
        handled_flaws,
        notes,
        verdict,
        skipped,
        ballot_maps: Value::Object(maps),
    };
    let v = serde_json::to_value(r)?;
    storage::put(tx, c, "report", &v)?;
    own_artifact(tx, g, id, s, "forge.report", v)
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    sub: ContestTransitionSubmission,
) -> Result<()> {
    let s = load_state(tx, sub.contest_id)?;
    if sub.kind != ContestKind::Forge || sub.expected_state_version != s.state_version {
        return Err(invalid());
    }
    match sub.transition {
        ContestTransition::Forge(ForgeTransition::Advance) => advance(tx, g, id, s),
        ContestTransition::Forge(ForgeTransition::Stop) => stop(tx, g, id, s.contest_id),
        _ => Err(invalid()),
    }
}
// Forge allocates and commits its barriers inside advance, with no caller-supplied projections.
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
pub fn barrier(_tx: &Transaction<'_>, _g: &mut CommitGroup, _c: BarrierCommit) -> Result<()> {
    Err(invalid())
}
