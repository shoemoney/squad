//! The M1 core/debate schema. Later modes add migrations when built.
use rusqlite::{Connection, TransactionBehavior};
pub const INITIAL: &str = include_str!("../migrations/0001_initial.sql");
/// Newest schema version this build writes.
const CURRENT_VERSION: i64 = 7;
/// How long a statement waits for another connection's write lock.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Apply the shared durability and integrity settings to one connection.
pub fn configure(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
}
/// Apply the initial schema atomically, once. Foreign keys remain enabled.
pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    configure(connection)?;
    connection.pragma_update(None, "foreign_keys", false)?;
    let result = (|| {
        // Take the write lock before the first read. SQLite skips the busy
        // handler when a read transaction upgrades to a write, so a deferred
        // transaction fails at once with SQLITE_BUSY while another connection
        // (the room writer) is mid-commit.
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > CURRENT_VERSION {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if version == 0 {
            tx.execute_batch(INITIAL)?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("../migrations/0002_tournament.sql"))?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("../migrations/0003_census.sql"))?;
        }
        if version < 4 {
            tx.execute_batch(include_str!("../migrations/0004_crosscheck.sql"))?;
        }
        if version < 5 {
            tx.execute_batch(include_str!("../migrations/0005_survey.sql"))?;
        }
        if version < 6 {
            tx.execute_batch(include_str!("../migrations/0006_grill.sql"))?;
        }
        if version < 7 {
            tx.execute_batch(include_str!("../migrations/0007_probe.sql"))?;
        }
        let invalid: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
            [],
            |r| r.get(0),
        )?;
        if invalid {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if version != CURRENT_VERSION {
            tx.pragma_update(None, "user_version", CURRENT_VERSION)?;
        }
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
