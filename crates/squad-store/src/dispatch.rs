//! Request-scoped dispatch for one-shot opens. Dormant work remains untouched.
use crate::{
    Result,
    error::{encode, invalid},
    work::template,
};
use rusqlite::{Connection, params};
use squad_protocol::*;
#[derive(Clone, Default)]
pub struct DispatchScope {
    pub work_ids: Vec<WorkId>,
}
pub fn claim_scoped(
    db: &Connection,
    id: WorkId,
    key: &ExecutorKey,
    now: DateTime,
    scope: &DispatchScope,
) -> Result<Option<ExecutionGrant>> {
    if !scope.work_ids.contains(&id) {
        return Ok(None);
    }
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
    let changed=db.execute("UPDATE work SET status='running',attempt=attempt+1,history_through=COALESCE(history_through,?1),lease_owner=?2,lease_fence=lease_fence+1,lease_until=?3 WHERE work_id=?4 AND status='queued' AND runnable=1 AND held_by_contest_id IS NULL AND epoch=(SELECT epoch FROM room_meta) AND (kind<>'judge.round' OR attempt<2) AND NOT EXISTS(SELECT 1 FROM work prior WHERE prior.context_id=work.context_id AND prior.status IN ('queued','running') AND prior.work_id IN (SELECT value FROM json_each(?5)) AND (prior.trigger_event_seq<work.trigger_event_seq OR (prior.trigger_event_seq=work.trigger_event_seq AND prior.rowid<work.rowid)))",params![boundary,encode(key)?,lease.to_rfc3339(),id.to_string(),serde_json::to_string(&scope.work_ids)?])?;
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
