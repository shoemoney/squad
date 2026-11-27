//! Frozen design/deal and accepted result snapshots; commits are router-owned.
use crate::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use squad_protocol::{rank::*, *};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Run {
    pub case_id: CaseId,
    pub batches: BTreeMap<MemberNick, RankBatch>,
    pub tally: Option<RankTally>,
    pub explanation: Option<RankExplanation>,
    pub skipped: Vec<MemberNick>,
}
pub fn insert(db: &Connection, id: ContestId, setup: &RankSetup) -> Result<()> {
    setup
        .validate()
        .map_err(|_| crate::StoreError::Unavailable)?;
    let run = Run {
        case_id: CaseId::new(),
        batches: BTreeMap::new(),
        tally: None,
        explanation: None,
        skipped: vec![],
    };
    db.execute(
        "INSERT INTO rank_runs VALUES (?,?,?)",
        params![
            id.to_string(),
            serde_json::to_string(setup)?,
            serde_json::to_string(&run)?
        ],
    )?;
    Ok(())
}
pub fn setup(db: &Connection, id: ContestId) -> Result<RankSetup> {
    let raw: String = db.query_row(
        "SELECT setup_json FROM rank_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn run(db: &Connection, id: ContestId) -> Result<Run> {
    let raw: String = db.query_row(
        "SELECT run_json FROM rank_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn save(db: &Connection, id: ContestId, run: &Run) -> Result<()> {
    if db.execute(
        "UPDATE rank_runs SET run_json=? WHERE contest_id=?",
        params![serde_json::to_string(run)?, id.to_string()],
    )? != 1
    {
        return Err(crate::StoreError::Unavailable);
    }
    Ok(())
}
