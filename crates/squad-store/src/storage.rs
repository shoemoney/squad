//! Durable-storage ceiling: new generation admission is refused once the
//! database holds `STORAGE_CAPACITY_BYTES`; history, export and cleanup stay available.
use crate::{Result, StoreError};
use rusqlite::Connection;
use squad_protocol::{ErrorCode, constants};

/// Bytes held by live pages (free pages left by deletes do not count).
pub fn used_bytes(db: &Connection) -> Result<u64> {
    let pages: i64 = db.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let free: i64 = db.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
    let size: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    Ok((pages - free).max(0) as u64 * size.max(0) as u64)
}

/// Refuse new generation work when the ceiling is reached.
pub fn ensure_capacity(db: &Connection, capacity: u64) -> Result<()> {
    if used_bytes(db)? >= capacity {
        return Err(StoreError::Domain(
            ErrorCode::STORAGE_LIMIT,
            STORAGE_LIMIT_TEXT,
        ));
    }
    Ok(())
}

pub const STORAGE_LIMIT_TEXT: &str = "Storage limit reached. History is preserved: export it with `squad export`, then free space with /clear.";

pub const DEFAULT_CAPACITY: u64 = constants::STORAGE_CAPACITY_BYTES;
