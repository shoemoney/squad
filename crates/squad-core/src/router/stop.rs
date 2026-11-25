use rusqlite::{Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn apply(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    target: Option<MemberNick>,
) -> Result<()> {
    let mut query=tx.prepare("SELECT work_id FROM work WHERE status IN ('queued','running') AND (?1 IS NULL OR target=?1) ORDER BY trigger_event_seq,work_id")?;
    let ids = query
        .query_map([target.as_ref().map(|n| n.as_str())], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if ids.is_empty() {
        return Err(StoreError::Domain(
            ErrorCode::NOTHING_TO_STOP,
            "Nothing to stop",
        ));
    }
    for work_id in ids {
        let work_id = work_id.parse().map_err(|_| StoreError::Unavailable)?;
        let record = squad_store::work::record(tx, work_id)?.ok_or(StoreError::Unavailable)?;
        let event = EnvelopePayload::WorkState(WorkStateEvent {
            work_id,
            trigger_event_id: squad_store::work::template(tx, work_id)?
                .ok_or(StoreError::Unavailable)?
                .0
                .trigger_event_id,
            target: record.target,
            context_id: record.context_id,
            attempt: record.attempt,
            status: WorkStatus::Cancelled,
            error: None,
            debate_id: None,
            case_id: None,
            contest_id: None,
        });
        let seq = group.push(
            tx,
            super::envelope(id, Actor::System, vec![Recipient::Me], event),
        )?;
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1,completed_event_seq=? WHERE work_id=?",params![seq.get(),work_id.to_string()])?;
    }
    Ok(())
}
