//! Mode-owned durable snapshots; executor access remains through leased scoped readers.
use crate::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use squad_protocol::trace::*;
use squad_protocol::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub case_id: CaseId,
    pub seed: String,
    pub extractions: BTreeMap<String, TraceExtraction>,
    pub provenance: BTreeMap<String, MemberNick>,
    pub quote_checks: Vec<TraceQuoteCheck>,
    pub claims: TraceClaims,
    pub assignments: BTreeMap<MemberNick, Vec<String>>,
    pub verifications: BTreeMap<MemberNick, TraceVerification>,
    pub ledger: TraceLedger,
    pub brief: Option<TraceBrief>,
    pub skipped: Vec<String>,
}
pub fn insert(db: &Connection, id: ContestId, setup: &TraceSetup) -> Result<()> {
    let run = Run {
        case_id: CaseId::new(),
        seed: CellId::new().to_string(),
        extractions: BTreeMap::new(),
        provenance: BTreeMap::new(),
        quote_checks: vec![],
        claims: TraceClaims { claims: vec![] },
        assignments: BTreeMap::new(),
        verifications: BTreeMap::new(),
        ledger: TraceLedger { claims: vec![] },
        brief: None,
        skipped: vec![],
    };
    db.execute(
        "INSERT INTO trace_runs VALUES(?,?,?)",
        params![
            id.to_string(),
            serde_json::to_string(setup)?,
            serde_json::to_string(&run)?
        ],
    )?;
    Ok(())
}
pub fn setup(db: &Connection, id: ContestId) -> Result<TraceSetup> {
    let s: String = db.query_row(
        "SELECT setup_json FROM trace_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn run(db: &Connection, id: ContestId) -> Result<Run> {
    let s: String = db.query_row(
        "SELECT data_json FROM trace_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn save(db: &Connection, id: ContestId, run: &Run) -> Result<()> {
    db.execute(
        "UPDATE trace_runs SET data_json=? WHERE contest_id=?",
        params![serde_json::to_string(run)?, id.to_string()],
    )?;
    Ok(())
}
