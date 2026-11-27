//! Private durable mode snapshots. Executor reads use frozen history capabilities.
use crate::Result;
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use squad_protocol::blow_it_up::*;
use squad_protocol::*;
pub fn insert(db: &Connection, setup: &BlowItUpSetup) -> Result<()> {
    db.execute(
        "INSERT INTO blow_it_up_runs(contest_id,setup_json) VALUES(?,?)",
        params![setup.contest_id.to_string(), serde_json::to_string(setup)?],
    )?;
    crate::contest_steps::private_map(
        db,
        setup.contest_id,
        "blow_it_up.lenses",
        &serde_json::to_value(&setup.lenses)?,
    )
}
pub fn setup(db: &Connection, id: ContestId) -> Result<BlowItUpSetup> {
    read(db, id, "setup_json")
}
pub fn pool(db: &Connection, id: ContestId) -> Result<BlowItUpPool> {
    read(db, id, "pool_json")
}
pub fn tally(db: &Connection, id: ContestId) -> Result<BlowItUpTally> {
    read(db, id, "tally_json")
}
fn read<T: DeserializeOwned>(db: &Connection, id: ContestId, column: &str) -> Result<T> {
    let raw: String = db.query_row(
        &format!("SELECT {column} FROM blow_it_up_runs WHERE contest_id=?"),
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn save_pool(db: &Connection, id: ContestId, pool: &BlowItUpPool) -> Result<()> {
    db.execute(
        "UPDATE blow_it_up_runs SET pool_json=? WHERE contest_id=?",
        params![serde_json::to_string(pool)?, id.to_string()],
    )?;
    Ok(())
}
pub fn save_tally(db: &Connection, id: ContestId, tally: &BlowItUpTally) -> Result<()> {
    db.execute(
        "UPDATE blow_it_up_runs SET tally_json=? WHERE contest_id=?",
        params![serde_json::to_string(tally)?, id.to_string()],
    )?;
    Ok(())
}
pub fn ballot(
    db: &Connection,
    id: ContestId,
    nick: &str,
    stage: &str,
    body: &impl Serialize,
    seq: Cursor,
) -> Result<()> {
    db.execute(
        "INSERT INTO blow_it_up_ballots VALUES(?,?,?,?,?)",
        params![
            id.to_string(),
            nick,
            stage,
            serde_json::to_string(body)?,
            seq.get()
        ],
    )?;
    Ok(())
}
pub fn ballots<T: DeserializeOwned>(
    db: &Connection,
    id: ContestId,
    stage: &str,
) -> Result<Vec<(String, T)>> {
    let mut q=db.prepare("SELECT nick,body FROM blow_it_up_ballots WHERE contest_id=? AND stage=? ORDER BY event_seq")?;
    let rows = q
        .query_map(params![id.to_string(), stage], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(n, b)| Ok((n, serde_json::from_str(&b)?)))
        .collect()
}
/// Reuse the same private draw and already committed pool after reopening.
pub fn state(db: &Connection, id: ContestId) -> Result<BlowItUpState> {
    let raw: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='blow_it_up'",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
