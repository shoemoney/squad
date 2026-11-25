//! The reset marker and all conversational deletions belong to the same receipt.
use rusqlite::Transaction;
use squad_protocol::*;
use squad_store::{Result, commit::CommitGroup};
pub fn apply(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    defaults: squad_store::clear::Defaults,
) -> Result<()> {
    let previous: i64 = tx.query_row("SELECT next_event_seq-1 FROM room_meta", [], |r| r.get(0))?;
    squad_store::clear::content_with_defaults(tx, defaults)?;
    group.epoch += 1;
    group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::RoomCleared(RoomCleared {
                epoch: group.epoch as u64,
                previous_through: Cursor::new(previous).ok(),
            }),
        ),
    )?;
    Ok(())
}
