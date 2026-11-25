use rusqlite::Transaction;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn apply(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    work_id: WorkId,
) -> Result<()> {
    let (mut work, binding) = squad_store::work::template(tx, work_id)?.ok_or(
        StoreError::Domain(ErrorCode::INVALID_REFERENCE, "Unknown work"),
    )?;
    let record = squad_store::work::record(tx, work_id)?.ok_or(StoreError::Unavailable)?;
    if record.status != WorkStatus::Failed
        || work.kind != WorkKind::Reply
        || work.contest_id.is_some()
    {
        return Err(StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Only failed ordinary work can be retried",
        ));
    }
    // An explicit retry allocates a new identity, retaining the original reply anchor.
    work.work_id = WorkId::new();
    work.message_id = MessageId::new();
    work.attempt = 1;
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::WorkState(WorkStateEvent {
                work_id: work.work_id,
                trigger_event_id: work.trigger_event_id,
                target: work.target.clone(),
                context_id: work.context_id.clone(),
                attempt: 0,
                status: WorkStatus::Queued,
                error: None,
                debate_id: None,
                case_id: None,
                contest_id: None,
            }),
        ),
    )?;
    let _ = seq;
    squad_store::work::insert(tx, &work, &binding, None)
}
