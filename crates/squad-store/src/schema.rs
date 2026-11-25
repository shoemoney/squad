//! The M1 core/debate schema. Later modes add migrations when built.
use rusqlite::Connection;
pub const INITIAL: &str = include_str!("../migrations/0001_initial.sql");
/// Apply the shared durability and integrity settings to one connection.
pub fn configure(connection: &Connection) -> rusqlite::Result<()> {
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
}
/// Apply the initial schema atomically, once. Foreign keys remain enabled.
pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    configure(connection)?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if !matches!(version, 0 | 1 | 10) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if version == 10 {
        return Ok(());
    }
    connection.pragma_update(None, "foreign_keys", false)?;
    let result = (|| {
        let tx = connection.transaction()?;
        if version == 0 {
            tx.execute_batch(INITIAL)?;
        }
        tx.execute_batch(include_str!("../migrations/0010_probe.sql"))?;
        let violations: bool = tx.prepare("PRAGMA foreign_key_check")?.exists([])?;
        if violations {
            return Err(rusqlite::Error::InvalidQuery);
        }
        tx.pragma_update(None, "user_version", 10)?;
        tx.commit()
    })();
    connection.pragma_update(None, "foreign_keys", true)?;
    result
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
