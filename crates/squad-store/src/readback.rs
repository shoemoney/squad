//! Mode-owned durable snapshots; executor access remains through leased scoped readers.
use crate::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use squad_protocol::readback::*;
use squad_protocol::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    pub spec: String,
    pub interpretations: BTreeMap<String, ReadbackInterpretation>,
    pub provenance: BTreeMap<String, MemberNick>,
    pub skipped: Vec<MemberNick>,
    pub ledger: Option<ReadbackLedger>,
    pub cut: u32,
    pub choices: Option<ReadbackChoices>,
    pub rewrite: Option<ReadbackRewrite>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub spec: String,
    pub rounds: Vec<Round>,
    pub resume: ReadbackStatus,
    pub case_id: CaseId,
}
pub fn insert(db: &Connection, id: ContestId, setup: &ReadbackSetup) -> Result<()> {
    let data = Run {
        spec: setup.spec.clone(),
        rounds: vec![],
        resume: ReadbackStatus::Interpreting,
        case_id: CaseId::new(),
    };
    db.execute(
        "INSERT INTO readback_runs VALUES(?,?,?)",
        params![
            id.to_string(),
            serde_json::to_string(setup)?,
            serde_json::to_string(&data)?
        ],
    )?;
    Ok(())
}
pub fn setup(db: &Connection, id: ContestId) -> Result<ReadbackSetup> {
    let s: String = db.query_row(
        "SELECT setup_json FROM readback_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn run(db: &Connection, id: ContestId) -> Result<Run> {
    let s: String = db.query_row(
        "SELECT data_json FROM readback_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn save(db: &Connection, id: ContestId, run: &Run) -> Result<()> {
    db.execute(
        "UPDATE readback_runs SET data_json=? WHERE contest_id=?",
        params![serde_json::to_string(run)?, id.to_string()],
    )?;
    Ok(())
}
