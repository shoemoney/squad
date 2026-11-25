//! Recovery of execution ownership. Run under the sole writer's transaction.
use crate::{Result, error::invalid};
use rusqlite::{Connection, params};
use squad_protocol::*;

/// Startup invalidates every previous owner. Periodic scans invalidate expired leases.
/// The durable start ledger, rather than a delivered wakeup, consumes an ordinal.
pub fn recover(db: &Connection, startup: bool, now: DateTime) -> Result<Vec<WorkId>> {
    let mut query = db.prepare("SELECT work_id FROM work WHERE status='running' AND (?1 OR lease_until<=?2 OR execution_deadline<=?2) ORDER BY trigger_event_seq,rowid")?;
    let ids = query
        .query_map(params![startup, now.to_rfc3339()], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut recovered = Vec::new();
    for id in ids {
        // An attempt row is conservative evidence even if an interrupted older
        // implementation failed to persist its convenience flag.
        db.execute("UPDATE work SET attempt=attempt-CASE WHEN provider_may_have_started=0 AND NOT EXISTS(SELECT 1 FROM attempts WHERE attempts.work_id=work.work_id AND attempts.attempt=work.attempt) THEN 1 ELSE 0 END, status='queued',lease_owner=NULL,lease_until=NULL,lease_fence=lease_fence+1,provider_may_have_started=0 WHERE work_id=?", [&id])?;
        recovered.push(
            id.parse()
                .map_err(|_| invalid("Invalid stored work identity"))?,
        );
    }
    Ok(recovered)
}

/// Only unfinished, eligible work is a wakeup candidate. Held and terminal work
/// stays stored, including exhausted judge work awaiting its owning outcome policy.
pub fn eligible(db: &Connection) -> Result<Vec<WorkRecord>> {
    eligible_at(db, chrono::Utc::now())
}
pub fn eligible_at(db: &Connection, now: DateTime) -> Result<Vec<WorkRecord>> {
    let mut query = db.prepare("SELECT work_id FROM work WHERE status IN ('queued','running') AND runnable=1 AND held_by_contest_id IS NULL AND epoch=(SELECT epoch FROM room_meta) AND (execution_deadline IS NULL OR execution_deadline>?1) AND (status='running' OR attempt<CASE kind WHEN 'judge.round' THEN 2 WHEN 'judge.tournament' THEN 3 WHEN 'judge' THEN 3 ELSE 2 END) ORDER BY trigger_event_seq,rowid")?;
    let ids = query
        .query_map([now.to_rfc3339()], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ids.into_iter()
        .map(|id| {
            crate::work::record(
                db,
                id.parse()
                    .map_err(|_| invalid("Invalid stored work identity"))?,
            )?
            .ok_or(crate::StoreError::Unavailable)
        })
        .collect()
}

/// Single lease authority for interactive and invocation-scoped dispatch.
pub fn claim(
    db: &Connection,
    id: WorkId,
    key: &ExecutorKey,
    now: DateTime,
    scope: Option<&[WorkId]>,
) -> Result<Option<ExecutionGrant>> {
    if scope.is_some_and(|scope| !scope.contains(&id)) {
        return Ok(None);
    }
    let Some((mut work, binding)) = crate::work::template(db, id)? else {
        return Ok(None);
    };
    if !key.owns(&work) {
        return Err(crate::StoreError::Domain(
            ErrorCode::PRODUCER_FORBIDDEN,
            "Executor does not own work",
        ));
    }
    let scope = scope.map(serde_json::to_string).transpose()?;
    let boundary: i64 = db.query_row(
        "SELECT COALESCE(MAX(pos),0) FROM history WHERE context_id=?",
        [&work.context_id.0],
        |r| r.get(0),
    )?;
    if boundary == 0 {
        return Ok(None);
    }
    let lease = now + chrono::Duration::milliseconds(constants::WORK_LEASE_MS as i64);
    let changed=db.execute("UPDATE work SET status='running',attempt=attempt+1,provider_may_have_started=0,history_through=COALESCE(history_through,?1),lease_owner=?2,lease_fence=lease_fence+1,lease_until=?3 WHERE work_id=?4 AND status='queued' AND runnable=1 AND held_by_contest_id IS NULL AND epoch=(SELECT epoch FROM room_meta) AND (execution_deadline IS NULL OR execution_deadline>?5) AND attempt<CASE kind WHEN 'judge.tournament' THEN 3 WHEN 'judge' THEN 3 ELSE 2 END AND NOT EXISTS(SELECT 1 FROM work prior WHERE (?6 IS NULL OR prior.work_id IN (SELECT value FROM json_each(?6))) AND json_type(prior.scheduling_meta,'$.openModeDormant') IS NULL AND prior.context_id=work.context_id AND prior.status IN ('queued','running') AND (prior.trigger_event_seq<work.trigger_event_seq OR (prior.trigger_event_seq=work.trigger_event_seq AND prior.rowid<work.rowid)))",params![boundary,serde_json::to_string(key)?,lease.to_rfc3339(),id.to_string(),now.to_rfc3339(),scope])?;
    if changed == 0 {
        return Ok(None);
    }
    let (attempt,pos,fence,epoch,deadline):(u32,i64,i64,i64,Option<String>)=db.query_row("SELECT attempt,history_through,lease_fence,epoch,execution_deadline FROM work WHERE work_id=?",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    work.attempt = attempt;
    work.history_through = HistoryPosition::new(pos).map_err(invalid)?;
    Ok(Some(ExecutionGrant {
        binding,
        work,
        owner: key.clone(),
        fence: fence as u64,
        epoch: epoch as u64,
        lease_until: lease,
        deadline: deadline
            .map(|s| s.parse().map_err(|_| invalid("Invalid stored deadline")))
            .transpose()?,
    }))
}
