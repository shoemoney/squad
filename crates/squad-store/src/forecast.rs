//! Immutable forecast admission and work-authorized output validation.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use squad_protocol::{forecast::*, *};
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid forecast transition",
    )
}
pub fn setup(db: &Connection, id: ContestId) -> Result<ForecastSetup> {
    let raw: String = db.query_row(
        "SELECT setup_json FROM forecast_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn put<T: Serialize>(
    db: &Connection,
    id: ContestId,
    key: &str,
    body: &T,
    seq: Option<Cursor>,
) -> Result<()> {
    db.execute(
        "INSERT INTO forecast_records VALUES(?,?,?,?)",
        params![
            id.to_string(),
            key,
            serde_json::to_string(body)?,
            seq.map(|s| s.get())
        ],
    )?;
    Ok(())
}
pub fn get<T: DeserializeOwned>(db: &Connection, id: ContestId, key: &str) -> Result<Option<T>> {
    let raw: Option<String> = db
        .query_row(
            "SELECT body FROM forecast_records WHERE contest_id=? AND record_key=?",
            params![id.to_string(), key],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
pub fn input(db: &Connection, id: WorkId) -> Result<serde_json::Value> {
    let raw: String = db.query_row(
        "SELECT input_json FROM forecast_work_inputs WHERE work_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn accept(db: &Connection, work: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    let contest = work.contest_id.ok_or_else(invalid)?;
    let raw: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND active=1",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let state: ForecastState = serde_json::from_str(&raw)?;
    if state.step_id != work.step_id
        || a.round != Some(0)
        || a.artifact_kind.as_str() != work.kind.new_mode_composition().ok_or_else(invalid)?
        || a.audience != Some(ArtifactAudience::Human)
        || a.data != *a.body.as_ref().ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    let input = input(db, work.work_id)?;
    validate_output(
        &work.kind,
        &input["data"],
        a.body.as_ref().ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    crate::contest_steps::artifact(db, a, seq)
}
