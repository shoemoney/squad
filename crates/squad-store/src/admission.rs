use crate::{Result, error::invalid};
use rusqlite::Connection;
/// Completion, reconciliation and cleanup do not call this new-work gate.
pub fn new_work(db: &Connection, max_pending: usize) -> Result<()> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM work WHERE status IN ('queued','running')",
        [],
        |r| r.get(0),
    )?;
    if count >= max_pending as i64 {
        return Err(invalid("Work queue is full"));
    }
    Ok(())
}
