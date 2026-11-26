//! Durable mode snapshots. Executors read only assigned history, never these records.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use squad_protocol::*;
pub fn put<T: Serialize>(db: &Connection, id: ContestId, key: &str, value: &T) -> Result<()> {
    db.execute("INSERT INTO decide_records VALUES(?,?,?) ON CONFLICT(contest_id,record_key) DO UPDATE SET body=excluded.body",params![id.to_string(),key,serde_json::to_string(value)?])?;
    Ok(())
}
pub fn get<T: DeserializeOwned>(db: &Connection, id: ContestId, key: &str) -> Result<T> {
    let raw: String = db.query_row(
        "SELECT body FROM decide_records WHERE contest_id=? AND record_key=?",
        params![id.to_string(), key],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn optional<T: DeserializeOwned>(
    db: &Connection,
    id: ContestId,
    key: &str,
) -> Result<Option<T>> {
    let raw: Option<String> = db
        .query_row(
            "SELECT body FROM decide_records WHERE contest_id=? AND record_key=?",
            params![id.to_string(), key],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|r| Ok(serde_json::from_str(&r)?)).transpose()
}
pub fn state(db: &Connection, id: ContestId) -> Result<DecideState> {
    let raw:String=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='decide' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn save(db: &Connection, s: &DecideState, expected: Option<u64>, seq: Cursor) -> Result<()> {
    s.validate().map_err(|_| {
        StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION, "Invalid decide state")
    })?;
    let old: Option<i64> = db
        .query_row(
            "SELECT state_version FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    if old != expected.map(|v| v as i64) {
        return Err(StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Stale decide state",
        ));
    }
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'decide',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get()])?;
    crate::contests::associate(db, s.contest_id, ContestKind::Decide, seq)
}
