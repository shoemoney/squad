//! Durable queues and frozen executable boundaries.
use crate::{
    Result,
    error::{decode, encode, invalid},
};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
pub fn insert(
    db: &Connection,
    work: &Work,
    binding: &ExecutorBinding,
    held: Option<ContestId>,
) -> Result<()> {
    work.validate()
        .map_err(|_| invalid("Invalid executable work"))?;
    if work.kind == WorkKind::JudgeTournament
        || (work.kind == WorkKind::Reply && work.round.is_some())
    {
        crate::tournaments::validate_execution(db, work)?;
    }
    if let Some(step) = work.step_id {
        let assignment: String = db.query_row(
            "SELECT assignment_json FROM contest_assignments WHERE step_id=? AND target=?",
            params![step.to_string(), work.target.to_string()],
            |r| r.get(0),
        )?;
        let owner = owner(work)?;
        work.validate_assignment(&owner, &decode(assignment)?)
            .map_err(|_| invalid("Invalid frozen assignment"))?;
    }
    if !owner(work)?.owns(work) {
        return Err(invalid("Work context is not owned by executor"));
    }
    if let Some(step) = work.step_id {
        crate::contest_steps::validate_step(
            db,
            work.contest_id.ok_or_else(|| invalid("Missing contest"))?,
            step,
        )?;
    }
    if let Some(debate) = work.debate_id {
        let state = crate::contests::debate(db, debate)?;
        work.validate_round_bound(state.total_rounds)
            .map_err(|_| invalid("Work exceeds admitted round limit"))?;
        if work.kind == WorkKind::JudgeRound
            && (!state.judge_requested || work.case_id != state.case_id)
        {
            return Err(invalid("Round judge does not match admitted case"));
        }
        if work.kind == WorkKind::DebateTurn {
            if !state
                .debaters
                .iter()
                .any(|n| work.target == Target::Member(n.clone()))
            {
                return Err(invalid("Work target is not a debater"));
            }
            let duplicate:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE debate_id=? AND round=? AND target=? AND kind='debate.turn')",params![debate.to_string(),work.round,work.target.to_string()],|r|r.get(0))?;
            if duplicate {
                return Err(invalid("Debate slot already allocated"));
            }
        }
    }
    if work.kind != WorkKind::Reply || work.step_id.is_some() {
        let boundary: i64 = db.query_row(
            "SELECT COALESCE(MAX(pos),0) FROM history WHERE context_id=?",
            [&work.context_id.0],
            |r| r.get(0),
        )?;
        if work.history_through.get() > boundary {
            return Err(invalid("Execution boundary is not retained history"));
        }
    }
    let kind = serde_json::to_value(&work.kind)?;
    let seq: i64 = db.query_row(
        "SELECT seq FROM events WHERE id=?",
        [work.trigger_event_id.to_string()],
        |r| r.get(0),
    )?;
    let epoch: i64 = db.query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))?;
    let mut meta = serde_json::json!({"work":work,"binding":binding});
    if let Some(contest) = work.contest_id {
        let raw: Option<String> = db.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [contest.to_string()],
            |r| r.get(0),
        )?;
        if let Some(raw) = raw {
            let value: serde_json::Value = decode(raw)?;
            let frozen_share = value
                .get("settings")
                .or_else(|| value.get("base").and_then(|b| b.get("settings")))
                .and_then(|s| s.get("attemptShares"))
                .and_then(|s| s.get(kind.as_str().unwrap_or_default()))
                .and_then(|n| n.as_i64())
                .filter(|n| *n > 0);
            let (plan_id, reserved, calls) = if work.kind.grill_composition().is_some() {
                let plan: ContestPreflight = serde_json::from_value(value)?;
                (
                    plan.plan_id,
                    plan.estimate.reserved_usd,
                    crate::grill::setup(db, contest)?.setup.normal_calls() * 2,
                )
            } else if work.kind.is_probe() {
                let plan: ProbePreflight = serde_json::from_value(value)?;
                (
                    plan.base.plan_id,
                    plan.base.estimate.reserved_usd,
                    plan.reserved_attempts,
                )
            } else if value.get("kind").and_then(|v| v.as_str()) == Some("survey") {
                let plan: ContestPreflight = serde_json::from_value(value)?;
                let attempts = plan.settings["reservedAttempts"]
                    .as_u64()
                    .ok_or_else(|| invalid("Missing survey attempt bound"))?;
                (
                    plan.plan_id,
                    plan.estimate.reserved_usd,
                    u32::try_from(attempts).map_err(|_| invalid("Invalid survey attempts"))?,
                )
            } else if work.kind.is_crosscheck() {
                let plan: CrosscheckPreflight = serde_json::from_value(value)?;
                (
                    plan.base.plan_id,
                    plan.base.estimate.reserved_usd,
                    plan.reserved_attempts,
                )
            } else if work.kind.census_composition().is_some() {
                let plan: ContestPreflight = serde_json::from_value(value)?;
                let setup = crate::census::setup(db, contest)?;
                (
                    plan.plan_id,
                    plan.estimate.reserved_usd,
                    plan.settings["reservedCalls"]
                        .as_u64()
                        .map(|v| v as u32)
                        .unwrap_or_else(|| setup.reserved_calls()),
                )
            } else if work.kind.new_mode_kind().is_some() {
                let plan: ContestPreflight = serde_json::from_value(value)?;
                if Some(plan.kind) != work.kind.new_mode_kind() {
                    return Err(invalid("Work does not match frozen contest plan"));
                }
                let attempts = plan.settings["reservedAttempts"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| invalid("Missing mode attempt bound"))?;
                (plan.plan_id, plan.estimate.reserved_usd, attempts)
            } else if value.get("base").is_some() {
                let p: TournamentPreflight = serde_json::from_value(value)?;
                (
                    p.base.plan_id,
                    p.base.estimate.reserved_usd,
                    (2..=p.total_rounds + 1).map(|n| 2 * (2 * n + 3)).sum(),
                )
            } else {
                let p: DebatePreflight = serde_json::from_value(value)?;
                (p.plan_id, p.estimate.reserved_usd, p.reserved_calls())
            };
            let total = crate::accounting::reservation_micros(reserved)
                .map_err(|_| invalid("Invalid plan reservation"))?;
            let share = frozen_share.unwrap_or(total / i64::from(calls));
            meta["planId"] = serde_json::json!(plan_id);
            meta["attemptReservationMicros"] = serde_json::json!(share);
        }
    }
    db.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,in_reply_to_message_id,status,history_through,epoch,runnable,held_by_contest_id,scheduling_meta,contest_id,debate_id,round,replay,case_id,prompt_ref,max_output_tokens,step_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'queued',?21,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",params![work.work_id.to_string(),kind.as_str(),work.target.to_string(),work.context_id.0,seq,work.trigger_event_id.to_string(),work.message_id.to_string(),work.in_reply_to_message_id.map(|id|id.to_string()),epoch,held.is_none(),held.map(|id|id.to_string()),encode(&meta)?,work.contest_id.map(|id|id.to_string()),work.debate_id.map(|id|id.to_string()),work.round,work.replay,work.case_id.map(|id|id.to_string()),work.prompt_ref.as_ref().map(encode).transpose()?,work.max_output_tokens,work.step_id.map(|id|id.to_string()),if work.kind!=WorkKind::Reply || work.step_id.is_some() {Some(work.history_through.get())}else{None}])?;
    crate::prompt_provenance::record(db, work, binding)?;
    Ok(())
}
pub fn owner(work: &Work) -> Result<ExecutorKey> {
    match &work.target {
        Target::Member(n) => Ok(ExecutorKey::Member(n.clone())),
        Target::Judge => Ok(ExecutorKey::Case(
            work.case_id.ok_or_else(|| invalid("Missing case"))?,
        )),
    }
}
pub fn template(db: &Connection, id: WorkId) -> Result<Option<(Work, ExecutorBinding)>> {
    let meta: Option<String> = db
        .query_row(
            "SELECT scheduling_meta FROM work WHERE work_id=?",
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    meta.map(|meta| {
        let value: serde_json::Value = decode(meta)?;
        Ok((
            serde_json::from_value(value["work"].clone())?,
            serde_json::from_value(value["binding"].clone())?,
        ))
    })
    .transpose()
}
type RecordColumns = (
    String,
    u32,
    bool,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
);
pub fn record(db: &Connection, id: WorkId) -> Result<Option<WorkRecord>> {
    let Some((mut work, _)) = template(db, id)? else {
        return Ok(None);
    };
    let (status,attempt,runnable,boundary,lease,deadline,error):RecordColumns=db.query_row("SELECT status,attempt,runnable,history_through,lease_until,execution_deadline,error_json FROM work WHERE work_id=?",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
    work.attempt = attempt;
    if let Some(pos) = boundary {
        work.history_through = HistoryPosition::new(pos).map_err(invalid)?;
    }
    Ok(Some(WorkRecord {
        work_id: id,
        message_id: work.message_id,
        in_reply_to_message_id: work.in_reply_to_message_id,
        kind: work.kind.clone(),
        target: work.target.clone(),
        context_id: work.context_id.clone(),
        status: decode(format!("\"{status}\""))?,
        attempt,
        runnable,
        executable: boundary.filter(|_| attempt > 0).map(|_| work),
        lease_until: lease.map(|s| decode(format!("\"{s}\""))).transpose()?,
        deadline: deadline.map(|s| decode(format!("\"{s}\""))).transpose()?,
        error: error.map(decode).transpose()?,
    }))
}
pub fn check_fence(
    db: &Connection,
    id: WorkId,
    key: &ExecutorKey,
    attempt: u32,
    fence: u64,
    epoch: u64,
    now: DateTime,
) -> Result<Work> {
    let (work, _) = template(db, id)?.ok_or(crate::StoreError::Domain(
        ErrorCode::STALE_WORK_ATTEMPT,
        "Stale work attempt",
    ))?;
    if !key.owns(&work) {
        return Err(crate::StoreError::Domain(
            ErrorCode::PRODUCER_FORBIDDEN,
            "Executor does not own work",
        ));
    }
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE work_id=?1 AND status='running' AND attempt=?2 AND lease_fence=?3 AND epoch=?4 AND epoch=(SELECT epoch FROM room_meta) AND lease_owner=?5 AND lease_until>?6 AND (execution_deadline IS NULL OR execution_deadline>?6))",params![id.to_string(),attempt,fence as i64,epoch as i64,encode(key)?,now.to_rfc3339()],|r|r.get(0))?;
    if !valid {
        return Err(crate::StoreError::Domain(
            ErrorCode::STALE_WORK_ATTEMPT,
            "Stale work attempt",
        ));
    }
    let mut executable = work;
    executable.attempt = attempt;
    executable.history_through = HistoryPosition::new(db.query_row(
        "SELECT history_through FROM work WHERE work_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?)
    .map_err(invalid)?;
    Ok(executable)
}
pub fn finish(db: &Connection, id: WorkId, status: WorkStatus, event: Cursor) -> Result<()> {
    crate::accounting::release_unstarted_work(db, id)?;
    let status = serde_json::to_value(status)?;
    if db.execute("UPDATE work SET status=?1,runnable=0,lease_fence=lease_fence+1,completed_event_seq=?2,error_json=CASE WHEN ?1='completed' THEN NULL ELSE error_json END WHERE work_id=?3 AND status='running'",params![status.as_str(),event.get(),id.to_string()])?!=1 {return Err(invalid("Work already terminal"));}
    Ok(())
}
