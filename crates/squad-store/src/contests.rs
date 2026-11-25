use crate::{
    Result,
    error::{decode, encode, invalid},
};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
pub fn save_debate(
    db: &Connection,
    state: &DebateState,
    expected: Option<u64>,
    seq: Cursor,
) -> Result<()> {
    state
        .validate()
        .map_err(|_| invalid("Invalid admitted debate state"))?;
    let old: Option<i64> = db
        .query_row(
            "SELECT state_version FROM contests WHERE contest_id=? AND kind='debate'",
            [state.debate_id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    if old != expected.map(|n| n as i64) {
        return Err(invalid("Contest state version changed"));
    }
    let active = !matches!(
        state.state,
        DebateStatus::Completed
            | DebateStatus::CompleteUnjudged
            | DebateStatus::Stopped
            | DebateStatus::Failed
            | DebateStatus::Rejected
            | DebateStatus::Cancelled
            | DebateStatus::Expired
    );
    if active {
        let conflict: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1 AND contest_id<>?)",
            [state.debate_id.to_string()],
            |r| r.get(0),
        )?;
        if conflict {
            return Err(invalid("A contest is already active"));
        }
    }
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?1,'debate',(SELECT epoch FROM room_meta),?2,?3,?4,?5) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![state.debate_id.to_string(),state.state_version as i64,encode(state)?,active,seq.get()])?;
    Ok(())
}
pub fn debate(db: &Connection, id: DebateId) -> Result<DebateState> {
    decode(db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='debate' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?)
}
pub fn associate(db: &Connection, id: ContestId, kind: ContestKind, seq: Cursor) -> Result<()> {
    if !crate::schema::validate_contest_kind(db, id, kind)? {
        return Err(invalid("Contest kind mismatch"));
    }
    db.execute(
        "INSERT OR IGNORE INTO contest_events VALUES (?,?)",
        params![id.to_string(), seq.get()],
    )?;
    Ok(())
}
