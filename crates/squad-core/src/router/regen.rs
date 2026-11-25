//! Supersession and replacement work are accepted in the same transaction.
use rusqlite::{OptionalExtension, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn apply(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    message: Option<MessageId>,
) -> Result<()> {
    let candidate: Option<i64> = if let Some(message) = message {
        tx.query_row("SELECT seq FROM events WHERE kind='message.final' AND json_extract(payload,'$.messageId')=?",[message.to_string()],|r|r.get(0)).optional()?
    } else {
        tx.query_row("SELECT MAX(seq) FROM events WHERE kind='message.final' AND NOT EXISTS(SELECT 1 FROM superseded WHERE superseded_event_id=events.id)",[],|r|r.get(0))?
    };
    let source = squad_store::events::at(
        tx,
        candidate.ok_or(StoreError::Domain(
            ErrorCode::REGEN_NOT_ALLOWED,
            "No eligible message to regenerate",
        ))?,
    )?;
    let EnvelopePayload::MessageFinal(reply) = source.content else {
        return Err(StoreError::Domain(
            ErrorCode::REGEN_NOT_ALLOWED,
            "Only ordinary replies can be regenerated",
        ));
    };
    let (mut work, binding) = squad_store::work::template(tx, reply.completed.generation.work_id)?
        .ok_or(StoreError::Unavailable)?;
    if work.kind != WorkKind::Reply || work.contest_id.is_some() {
        return Err(StoreError::Domain(
            ErrorCode::REGEN_NOT_ALLOWED,
            "Contest replies cannot be regenerated",
        ));
    }
    let Target::Member(member) = work.target.clone() else {
        return Err(super::authorization::forbidden());
    };
    let superseded: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM superseded WHERE superseded_event_id=?)",
        [source.base.id.to_string()],
        |r| r.get(0),
    )?;
    if superseded {
        return Err(StoreError::Domain(
            ErrorCode::REGEN_NOT_ALLOWED,
            "Message is already superseded",
        ));
    }
    work.work_id = WorkId::new();
    work.message_id = MessageId::new();
    work.attempt = 1;
    let marker = MessageSuperseded {
        superseded_event_id: source.base.id,
        superseded_message_id: reply.completed.generation.message_id,
        member: member.clone(),
        in_reply_to_message_id: reply.in_reply_to_message_id,
        replacement_work_id: work.work_id,
        replacement_message_id: work.message_id,
    };
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::MessageSuperseded(marker),
        ),
    )?;
    squad_store::work::insert(tx, &work, &binding, None)?;
    tx.execute(
        "INSERT INTO superseded VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            source.base.id.to_string(),
            reply.completed.generation.message_id.to_string(),
            member.as_str(),
            reply.in_reply_to_message_id.to_string(),
            work.work_id.to_string(),
            work.message_id.to_string(),
            seq.get()
        ],
    )?;
    tx.execute(
        "UPDATE history SET excluded=1 WHERE origin_event_id=?",
        [source.base.id.to_string()],
    )?;
    Ok(())
}
