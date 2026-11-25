//! Census storage and immutable pass provenance. No executor can query private maps.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid census state or artifact",
    )
}
pub fn state(db: &Connection, id: ContestId) -> Result<CensusState> {
    let raw: String = db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='census' AND epoch=(SELECT epoch FROM room_meta)", [id.to_string()], |r| r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn setup(db: &Connection, id: ContestId) -> Result<CensusSetup> {
    let raw: String = db.query_row(
        "SELECT setup_json FROM census_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn plan(db: &Connection, id: ContestId) -> Result<CensusPlan> {
    let raw: String = db.query_row(
        "SELECT current_plan_json FROM census_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn save(
    db: &Connection,
    state: &CensusState,
    expected: Option<u64>,
    seq: Cursor,
) -> Result<()> {
    state.validate().map_err(|_| invalid())?;
    let old: Option<i64> = db
        .query_row(
            "SELECT state_version FROM contests WHERE contest_id=? AND kind='census'",
            [state.contest_id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    if old != expected.map(|v| v as i64) {
        return Err(invalid());
    }
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'census',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq", params![state.contest_id.to_string(),state.state_version as i64,serde_json::to_string(state)?,!state.status.is_terminal(),seq.get()])?;
    crate::contests::associate(db, state.contest_id, ContestKind::Census, seq)
}
pub fn events(db: &Connection, id: ContestId) -> Result<Vec<CommittedEnvelope>> {
    let mut q =
        db.prepare("SELECT event_seq FROM contest_events WHERE contest_id=? ORDER BY event_seq")?;
    q.query_map([id.to_string()], |r| r.get::<_, i64>(0))?
        .map(|s| crate::events::at(db, s?))
        .collect()
}
pub fn input(db: &Connection, work: WorkId) -> Result<std::collections::BTreeMap<String, String>> {
    let raw: String = db.query_row(
        "SELECT values_json FROM census_work_inputs WHERE work_id=?",
        [work.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
/// Validate provider shape and cross-references using only the frozen step input.
pub fn validate_output(db: &Connection, work: &Work, output: &serde_json::Value) -> Result<()> {
    let values = input(db, work.work_id)?;
    let data: serde_json::Value =
        serde_json::from_str(values.get("mode_data_json").ok_or_else(invalid)?)?;
    if census_prose_words(output)
        > values["step_word_limit"]
            .parse::<usize>()
            .map_err(|_| invalid())?
    {
        return Err(invalid());
    }
    let valid = match work.kind {
        WorkKind::CensusHunt => {
            validation::decode::<CensusHuntOutput>(output).and_then(|v| v.validate())
        }
        WorkKind::CensusMerge => {
            let ids: Vec<String> = serde_json::from_value(data["sourceIds"].clone())?;
            validation::decode::<CensusMergeOutput>(output).and_then(|v| v.validate_against(&ids))
        }
        WorkKind::CensusRate => {
            let ledger: Vec<CensusGap> = data["ledger"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|g| {
                    Ok(CensusGap {
                        id: serde_json::from_value(g["id"].clone())?,
                        omission: serde_json::from_value(g["omission"].clone())?,
                        consequence: serde_json::from_value(g["consequence"].clone())?,
                        lenses: vec![],
                        source_find_ids: vec![],
                    })
                })
                .collect::<Result<_>>()?;
            validation::decode::<CensusRatingOutput>(output).and_then(|v| {
                v.validate_against(&ledger, data["plan"].as_str().unwrap_or_default())
            })
        }
        WorkKind::CensusFinal => {
            let v: CensusFinalOutput = validation::decode(output).map_err(|_| invalid())?;
            census_validate_final(&v, &data)
        }
        _ => return Err(invalid()),
    };
    valid.map_err(|_| invalid())
}
/// The judge can order confirmed gaps, but cannot reverse a computed majority.
/// The real router calls this only after checking the lease fence and before commit.
pub fn accept(db: &Connection, work: &Work, artifact: &ContestArtifact, seq: Cursor) -> Result<()> {
    let state = state(db, artifact.contest_id)?;
    if state.status.is_terminal()
        || state.step_id != work.step_id
        || !state.unfinished_work_ids.contains(&work.work_id)
        || artifact.artifact_kind.as_str() != work.kind.census_composition().ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    validate_output(db, work, &artifact.data)?;
    crate::contest_steps::validate_artifact(db, artifact)?;
    db.execute(
        "INSERT INTO mode_artifacts VALUES (?,?,?,?,?,?)",
        params![
            seq.get(),
            artifact.contest_id.to_string(),
            artifact.step_id.to_string(),
            artifact.artifact_kind.as_str(),
            work.work_id.to_string(),
            serde_json::to_string(&artifact.source_refs)?
        ],
    )?;
    crate::contest_steps::settle(db, artifact.step_id, &work.target, None)?;
    crate::work::finish(db, work.work_id, WorkStatus::Completed, seq)?;
    crate::contests::associate(db, artifact.contest_id, ContestKind::Census, seq)
}
