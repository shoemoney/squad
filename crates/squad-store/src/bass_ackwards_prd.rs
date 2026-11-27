//! Durable PRD records. Private source maps never cross executor capabilities.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use squad_protocol::bass_ackwards_prd::*;
use squad_protocol::*;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid bass_ackwards_prd state or artifact",
    )
}
pub fn state(db: &Connection, id: ContestId) -> Result<BassAckwardsPrdState> {
    let raw:String=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='bass_ackwards_prd' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn put<T: Serialize>(db: &Connection, id: ContestId, key: &str, value: &T) -> Result<()> {
    db.execute("INSERT INTO bass_ackwards_prd_records VALUES(?,?,?) ON CONFLICT(contest_id,record_key) DO UPDATE SET body=excluded.body",params![id.to_string(),key,serde_json::to_string(value)?])?;
    Ok(())
}
pub fn get<T: DeserializeOwned>(db: &Connection, id: ContestId, key: &str) -> Result<T> {
    let raw: String = db.query_row(
        "SELECT body FROM bass_ackwards_prd_records WHERE contest_id=? AND record_key=?",
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
            "SELECT body FROM bass_ackwards_prd_records WHERE contest_id=? AND record_key=?",
            params![id.to_string(), key],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|r| Ok(serde_json::from_str(&r)?)).transpose()
}
pub fn setup(db: &Connection, id: ContestId) -> Result<BassAckwardsPrdSetup> {
    get(db, id, "setup")
}
pub fn save(
    db: &Connection,
    s: &BassAckwardsPrdState,
    expected: Option<u64>,
    seq: Cursor,
) -> Result<()> {
    s.validate().map_err(|_| invalid())?;
    let old: Option<i64> = db
        .query_row(
            "SELECT state_version FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    if old != expected.map(|n| n as i64) {
        return Err(invalid());
    }
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES(?,'bass_ackwards_prd',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get()])?;
    crate::contests::associate(db, s.contest_id, ContestKind::BassAckwardsPrd, seq)
}
