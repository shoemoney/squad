//! The M1 core/debate schema. Later modes add migrations when built.
use rusqlite::Connection;
pub const INITIAL: &str = include_str!("../migrations/0001_initial.sql");
/// Apply the initial schema atomically, once. Foreign keys remain enabled.
pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
    )?;
    let tx = connection.transaction()?;
    let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 0 {
        tx.execute_batch(INITIAL)?;
        tx.pragma_update(None, "user_version", 1)?;
    } else if version != 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    tx.commit()
}
/// Validate a mode-specific reference against the shared contest table.
/// Generic carriers do not confer admission or permission to execute that mode.
pub fn validate_contest_kind(
    connection: &Connection,
    id: squad_protocol::ContestId,
    kind: squad_protocol::ContestKind,
) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE contest_id=?1 AND kind=?2)",
        rusqlite::params![id.to_string(), kind.as_str()],
        |row| row.get(0),
    )
}
