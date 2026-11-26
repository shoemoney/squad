//! Durable tune journal; pair identities prevent execution replay across restart.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use squad_protocol::tune::*;
use squad_protocol::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuneSheetRecord {
    pub sheet: TuneScoreSheet,
    pub maps: BTreeMap<String, BTreeMap<String, String>>,
    pub holdout: bool,
    pub generation: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuneJob {
    pub work_id: WorkId,
    pub input: TuneCaseMaterial,
    pub candidate: Option<String>,
    pub case_label: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuneRun {
    pub setup: TuneSetup,
    pub proposals: BTreeMap<MemberNick, TuneCaseProposal>,
    pub suite: Option<TuneSuite>,
    pub candidates: Vec<TuneCandidate>,
    pub champion: String,
    pub champions: Vec<String>,
    pub generation: u32,
    pub generations: Vec<TuneGeneration>,
    pub sheets: Vec<TuneSheetRecord>,
    pub digests: Vec<TuneDigest>,
    pub holdout: Option<TuneHoldout>,
    pub report: Option<TuneReport>,
    pub jobs: Vec<TuneJob>,
    pub maps: BTreeMap<String, BTreeMap<String, String>>,
    pub finalists: Vec<String>,
    pub skipped: Vec<WorkId>,
    #[serde(default)]
    pub skipped_members: Vec<MemberNick>,
    pub redaction: Vec<String>,
}
pub fn load(db: &Connection, id: ContestId) -> Result<TuneRun> {
    let raw: String = db.query_row(
        "SELECT run_json FROM tune_runs WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn save(db: &Connection, id: ContestId, run: &TuneRun) -> Result<()> {
    db.execute("INSERT INTO tune_runs VALUES(?,?) ON CONFLICT(contest_id) DO UPDATE SET run_json=excluded.run_json",params![id.to_string(),serde_json::to_string(run)?])?;
    Ok(())
}
pub fn execution(
    db: &Connection,
    id: ContestId,
    candidate: &str,
    case: &str,
) -> Result<Option<TuneExecution>> {
    let raw:Option<String>=db.query_row("SELECT execution_json FROM tune_executions WHERE contest_id=? AND candidate_id=? AND case_label=?",params![id.to_string(),candidate,case],|r|r.get(0)).optional()?;
    raw.map(|v| serde_json::from_str(&v).map_err(StoreError::from))
        .transpose()
}
pub fn retain_execution(db: &Connection, id: ContestId, value: &TuneExecution) -> Result<()> {
    if let Some(old) = execution(db, id, &value.candidate_id, &value.case_label)? {
        if old != *value {
            return Err(StoreError::Domain(
                ErrorCode::INVALID_STATE_TRANSITION,
                "Execution already retained",
            ));
        }
        return Ok(());
    }
    db.execute(
        "INSERT INTO tune_executions VALUES(?,?,?,?)",
        params![
            id.to_string(),
            value.candidate_id,
            value.case_label,
            serde_json::to_string(value)?
        ],
    )?;
    Ok(())
}
pub fn executions(db: &Connection, id: ContestId) -> Result<Vec<TuneExecution>> {
    let mut q=db.prepare("SELECT execution_json FROM tune_executions WHERE contest_id=? ORDER BY candidate_id,case_label")?;
    q.query_map([id.to_string()], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}

/// Install after all registry migrations: later scaffold migrations rebuild work
/// and would otherwise drop a trigger declared in 0019.
pub fn ensure_holds(db:&Connection)->Result<()> {
    db.execute_batch(r#"CREATE TRIGGER IF NOT EXISTS tune_hold_ordinary AFTER INSERT ON work
WHEN NEW.kind='reply' AND NEW.step_id IS NULL AND NEW.held_by_contest_id IS NULL
 AND EXISTS(SELECT 1 FROM contests c JOIN tune_runs t USING(contest_id)
  WHERE c.active=1 AND c.kind='tune' AND json_extract(c.state_json,'$.state')<>'PREFLIGHT'
  AND EXISTS(SELECT 1 FROM json_each(t.run_json,'$.setup.participants') WHERE value=NEW.target))
BEGIN
 UPDATE work SET runnable=0,held_by_contest_id=(
  SELECT c.contest_id FROM contests c JOIN tune_runs t USING(contest_id)
  WHERE c.active=1 AND c.kind='tune' AND json_extract(c.state_json,'$.state')<>'PREFLIGHT'
  AND EXISTS(SELECT 1 FROM json_each(t.run_json,'$.setup.participants') WHERE value=NEW.target)
 ) WHERE work_id=NEW.work_id;
END;
"#)?;Ok(())
}
