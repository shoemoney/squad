//! Mode-owned durable snapshots; executor access remains through leased scoped readers.
use crate::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use squad_protocol::debugging::*;
use squad_protocol::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub tables: BTreeMap<String, DebuggingHypotheses>,
    pub provenance: BTreeMap<String, MemberNick>,
    pub differential: Vec<DebuggingLiveHypothesis>,
    pub quotes: Vec<DebuggingQuoteCheck>,
    pub rounds: Vec<DebuggingRound>,
    pub skipped: Vec<String>,
    pub reason: Option<String>,
    pub narrative: Option<DebuggingSummary>,
    pub resume: DebuggingStatus,
    pub case_id: CaseId,
}
pub fn insert(db: &Connection, id: ContestId, setup: &DebuggingSetup) -> Result<()> {
    let data = Run {
        tables: BTreeMap::new(),
        provenance: BTreeMap::new(),
        differential: vec![],
        quotes: vec![],
        rounds: vec![],
        skipped: vec![],
        reason: None,
        narrative: None,
        resume: DebuggingStatus::Hypothesizing,
        case_id: CaseId::new(),
    };
    db.execute(
        "INSERT INTO debugging_runs VALUES(?,?,?)",
        params![
            id.to_string(),
            serde_json::to_string(setup)?,
            serde_json::to_string(&data)?
        ],
    )?;
    Ok(())
}
pub fn setup(db: &Connection, id: ContestId) -> Result<DebuggingSetup> {
    let s: String = db.query_row(
        "SELECT setup_json FROM debugging_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn run(db: &Connection, id: ContestId) -> Result<Run> {
    let s: String = db.query_row(
        "SELECT data_json FROM debugging_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn save(db: &Connection, id: ContestId, run: &Run) -> Result<()> {
    db.execute(
        "UPDATE debugging_runs SET data_json=? WHERE contest_id=?",
        params![serde_json::to_string(run)?, id.to_string()],
    )?;
    Ok(())
}
