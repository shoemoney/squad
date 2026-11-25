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
    let kind = serde_json::to_value(&work.kind)?;
    let seq: i64 = db.query_row(
        "SELECT seq FROM events WHERE id=?",
        [work.trigger_event_id.to_string()],
        |r| r.get(0),
    )?;
    let epoch: i64 = db.query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))?;
    let meta = serde_json::json!({"work":work,"binding":binding});
    db.execute("INSERT INTO work(work_id,kind,target,context_id,trigger_event_seq,trigger_event_id,message_id,in_reply_to_message_id,status,history_through,epoch,runnable,held_by_contest_id,scheduling_meta,contest_id,debate_id,round,replay,case_id,prompt_ref,max_output_tokens,step_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'queued',?21,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",params![work.work_id.to_string(),kind.as_str(),work.target.to_string(),work.context_id.0,seq,work.trigger_event_id.to_string(),work.message_id.to_string(),work.in_reply_to_message_id.map(|id|id.to_string()),epoch,held.is_none(),held.map(|id|id.to_string()),encode(&meta)?,work.contest_id.map(|id|id.to_string()),work.debate_id.map(|id|id.to_string()),work.round,work.replay,work.case_id.map(|id|id.to_string()),work.prompt_ref.as_ref().map(encode).transpose()?,work.max_output_tokens,work.step_id.map(|id|id.to_string()),if work.kind!=WorkKind::Reply || work.step_id.is_some() {Some(work.history_through.get())}else{None}])?;
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
pub fn claim(
    db: &Connection,
    id: WorkId,
    key: &ExecutorKey,
    now: DateTime,
) -> Result<Option<ExecutionGrant>> {
    let Some((mut work, binding)) = template(db, id)? else {
        return Ok(None);
    };
    if !key.owns(&work) {
        return Err(crate::StoreError::Domain(
            ErrorCode::PRODUCER_FORBIDDEN,
            "Executor does not own work",
        ));
    }
    let boundary: i64 = db.query_row(
        "SELECT COALESCE(MAX(pos),0) FROM history WHERE context_id=?",
        [&work.context_id.0],
        |r| r.get(0),
    )?;
    if boundary == 0 {
        return Ok(None);
    }
    let lease = now + chrono::Duration::milliseconds(constants::WORK_LEASE_MS as i64);
    let changed=db.execute("UPDATE work SET status='running',attempt=attempt+1,history_through=COALESCE(history_through,?1),lease_owner=?2,lease_fence=lease_fence+1,lease_until=?3 WHERE work_id=?4 AND status='queued' AND runnable=1 AND held_by_contest_id IS NULL AND epoch=(SELECT epoch FROM room_meta) AND (kind<>'judge.round' OR attempt<2) AND NOT EXISTS(SELECT 1 FROM work prior WHERE prior.context_id=work.context_id AND prior.status IN ('queued','running') AND (prior.trigger_event_seq<work.trigger_event_seq OR (prior.trigger_event_seq=work.trigger_event_seq AND prior.rowid<work.rowid)))",params![boundary,encode(key)?,lease.to_rfc3339(),id.to_string()])?;
    if changed == 0 {
        return Ok(None);
    }
    let (attempt, pos, fence, epoch): (u32, i64, i64, i64) = db.query_row(
        "SELECT attempt,history_through,lease_fence,epoch FROM work WHERE work_id=?",
        [id.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    work.attempt = attempt;
    work.history_through = HistoryPosition::new(pos).map_err(invalid)?;
    Ok(Some(ExecutionGrant {
        binding,
        work,
        owner: key.clone(),
        fence: fence as u64,
        epoch: epoch as u64,
        lease_until: lease,
        deadline: None,
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
    let status = serde_json::to_value(status)?;
    if db.execute("UPDATE work SET status=?1,runnable=0,lease_fence=lease_fence+1,completed_event_seq=?2 WHERE work_id=?3 AND status='running'",params![status.as_str(),event.get(),id.to_string()])?!=1 {return Err(invalid("Work already terminal"));}
    Ok(())
}
