//! trace router entry points. Filled by the mode worker.
use rusqlite::{Connection,Transaction};
use squad_protocol::*;
use squad_store::{Result,StoreError,commit::CommitGroup};
fn disabled<T>() -> Result<T> {Err(StoreError::Domain(ErrorCode::COMMAND_NOT_IMPLEMENTED,"trace is disabled"))}
pub fn prepare(_tx: &Transaction<'_>, _group: &mut CommitGroup, _id: EventId, _input: &UserMessageInput, _parsed: &ParsedInput) -> Result<RequestResult> { disabled() }
pub fn accepted_work(_tx: &Connection, _work: &Work, _artifact: &ContestArtifact, _seq: Cursor) -> Result<()> { disabled() }
pub fn human_answer(_tx: &Connection, _pending: &parser::PendingHuman, _complete: bool) -> Result<()> { Ok(()) }
pub fn stop(tx: &Connection, _group: &mut CommitGroup, _id: EventId, contest: ContestId) -> Result<()> {
 tx.execute("UPDATE contests SET active=0,state_version=state_version+1,state_json=json_set(state_json,'$.state','STOPPED','$.terminalReason','stopped','$.stateVersion',state_version+1) WHERE contest_id=?",[contest.to_string()])?;
 tx.execute("UPDATE contest_human_steps SET closed=1 WHERE contest_id=?",[contest.to_string()])?;
 squad_store::history::release(tx,contest)
}
pub fn recover(_tx: &Connection, _contest: ContestId) -> Result<()> { Ok(()) }
pub fn load_state(tx: &Connection, contest: ContestId) -> Result<TraceState> {
 let raw:String=tx.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind=?",rusqlite::params![contest.to_string(),"trace"],|r|r.get(0))?;
 Ok(serde_json::from_str(&raw)?)
}
