//! Transactional crosscheck records. Callers must use the shared commit group.
use crate::{Result, error::{decode,encode,invalid}};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
pub fn state(db: &Connection, id: ContestId) -> Result<CrosscheckState> {
    let raw: Option<String> = db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='crosscheck' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0)).optional()?;
    decode(raw.ok_or_else(||invalid("Crosscheck not retained"))?)
}
pub fn save_state(db: &Connection, state: &CrosscheckState, expected: Option<u64>, seq: Cursor) -> Result<()> {
    state.validate().map_err(|_|invalid("Invalid crosscheck state"))?;
    let old:Option<i64> = db.query_row("SELECT state_version FROM contests WHERE contest_id=? AND kind='crosscheck'",[state.contest_id.to_string()],|r|r.get(0)).optional()?;
    if old != expected.map(|n|n as i64) {return Err(invalid("Crosscheck state version changed"));}
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'crosscheck',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![state.contest_id.to_string(),state.state_version as i64,encode(state)?,!state.status.is_terminal(),seq.get()])?;
    crate::contests::associate(db,state.contest_id,ContestKind::Crosscheck,seq)
}
pub fn save_grid(db: &Connection, grid: &CrosscheckGrid, seq: Cursor) -> Result<()> {
    let s=state(db,grid.contest_id)?;
    if s.status!=CrosscheckStatus::BuildingGrid || s.plan_id!=grid.plan_id || s.plan_version!=grid.plan_version || grid.cells.len()>200 {return Err(invalid("Grid does not match admitted plan"));}
    for cell in &grid.cells {
        if cell.reviewer.as_ref().is_some_and(|n|cell.originators.contains(n)) {return Err(invalid("Self-origin review forbidden"));}
        db.execute("INSERT INTO crosscheck_cells(contest_id,cell_id,grid_event_seq,items_json,originators_json,reviewer,status,evidence_json) VALUES (?,?,?,?,?,?,'pending','{}')",params![grid.contest_id.to_string(),cell.cell_id.to_string(),seq.get(),encode(&cell.items)?,encode(&cell.originators)?,cell.reviewer.as_ref().map(|n|n.as_str())])?;
    }
    crate::contests::associate(db,grid.contest_id,ContestKind::Crosscheck,seq)
}
pub fn validate_review(db: &Connection, review: &CrosscheckReview, plan: &str) -> Result<()> {
    let s=state(db,review.contest_id)?;
    if s.status!=CrosscheckStatus::Reviewing {return Err(invalid("Reviews are fenced"));}
    let mut q=db.prepare("SELECT cell_id FROM crosscheck_cells WHERE contest_id=? AND reviewer=? AND review_event_seq IS NULL")?;
    let assigned=q.query_map(params![review.contest_id.to_string(),review.reviewer.as_str()],|r|r.get::<_,String>(0))?.map(|r|r?.parse().map_err(|_|rusqlite::Error::InvalidQuery)).collect::<rusqlite::Result<Vec<CellId>>>()?;
    review.response.validate(&assigned,plan).map_err(|_|invalid("Invalid assigned-cell evidence"))
}
pub fn save_review(db: &Connection, review: &CrosscheckReview, seq: Cursor) -> Result<()> {
    for cell in &review.response.cells {
        let status=serde_json::to_value(cell.status)?;
        if db.execute("UPDATE crosscheck_cells SET status=?,evidence_json=?,review_event_seq=? WHERE contest_id=? AND cell_id=? AND reviewer=? AND review_event_seq IS NULL",params![status.as_str(),encode(cell)?,seq.get(),review.contest_id.to_string(),cell.cell_id.to_string(),review.reviewer.as_str()])?!=1 {return Err(invalid("Review already accepted or not assigned"));}
    } Ok(())
}
pub fn freeze_samples(db: &Connection, step: StepId, samples: &[(MemberNick,Vec<CellId>)]) -> Result<()> {
    if samples.len()!=2 || samples[0].0==samples[1].0 {return Err(invalid("Two distinct auditors required"));}
    for (auditor,sample) in samples {
        if sample.len()>5 || sample.iter().collect::<std::collections::BTreeSet<_>>().len()!=sample.len() {return Err(invalid("Invalid audit sample"));}
        db.execute("INSERT INTO crosscheck_audit_samples(step_id,auditor,sample_json) VALUES (?,?,?)",params![step.to_string(),auditor.as_str(),encode(sample)?])?;
    } Ok(())
}
pub fn accept(db: &Connection, actor: &Actor, acceptance: &CrosscheckAcceptance, seq: Cursor) -> Result<()> {
    let s=state(db,acceptance.contest_id)?;
    if *actor!=Actor::Me || s.status!=CrosscheckStatus::Complete || s.plan_id!=acceptance.plan_id || s.plan_version!=acceptance.plan_version || s.grid_version!=acceptance.grid_version || s.state_version!=acceptance.expected_state_version || (acceptance.cell_ids.is_empty() && acceptance.residual_risk_ids.is_empty()) {return Err(invalid("Acceptance is stale or not human"));}
    let mut ids=std::collections::BTreeSet::new();
    for cell in &acceptance.cell_ids {
        if !ids.insert(cell) {return Err(invalid("Duplicate acceptance cell"));}
        if db.execute("UPDATE crosscheck_cells SET status='accepted',acceptance_event_seq=? WHERE contest_id=? AND cell_id=? AND status IN ('gap','pending')",params![seq.get(),s.contest_id.to_string(),cell.to_string()])?!=1 {return Err(invalid("Cell cannot be accepted"));}
    }
    crate::contests::associate(db,s.contest_id,ContestKind::Crosscheck,seq)
}
pub fn release_plan(db:&Connection,contest:ContestId) -> Result<()> {
    let mut q=db.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
    let rows=q.query_map([contest.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id,day,amount) in rows {db.execute("UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",params![amount,day])?;db.execute("DELETE FROM reservations WHERE reservation_id=?",[id])?;}
    Ok(())
}
