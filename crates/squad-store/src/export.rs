//! Consistent snapshots exclude private maps, repairs, scheduling metadata and reservations.
use crate::{Result, events};
use rusqlite::{Connection, params};
use squad_protocol::*;
pub fn room(db: &mut Connection) -> Result<Vec<CommittedEnvelope>> {
    let tx = db.transaction()?;
    let events = events::page(&tx, None, usize::MAX)?;
    tx.commit()?;
    Ok(events)
}
pub fn contest(db: &mut Connection, id: ContestId) -> Result<Vec<CommittedEnvelope>> {
    let tx = db.transaction()?;
    let events = {
        let mut query = tx.prepare(
            "SELECT event_seq FROM contest_events WHERE contest_id=? ORDER BY event_seq",
        )?;
        query
            .query_map(params![id.to_string()], |r| r.get::<_, i64>(0))?
            .map(|seq| events::at(&tx, seq?))
            .collect::<Result<Vec<_>>>()?
    };
    tx.commit()?;
    Ok(events)
}
