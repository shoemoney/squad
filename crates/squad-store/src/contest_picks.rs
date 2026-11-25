//! Durable question selection snapshots. Answer content stays in ordinary events;
//! picks freeze the selected message identities without changing their histories.
use crate::{Result, error::encode};
use rusqlite::{Connection, params};
use squad_protocol::*;

pub fn save_selection(db: &Connection, state: &ContestSelectionState, seq: Cursor) -> Result<()> {
    let active = matches!(
        state.state,
        ContestSelectionStatus::CollectingAnswers | ContestSelectionStatus::AwaitingPick
    );
    db.execute("UPDATE contests SET state_json=?,state_version=?,active=?,updated_seq=? WHERE contest_id=?",params![encode(state)?,state.state_version as i64,active,seq.get(),state.contest_id.to_string()])?;
    Ok(())
}
pub fn settle_answers(db: &Connection, contest: ContestId) -> Result<()> {
    db.execute("UPDATE contest_answers SET outcome=CASE (SELECT status FROM work WHERE work_id=contest_answers.work_id) WHEN 'completed' THEN 'answered' WHEN 'failed' THEN 'failed' ELSE 'stopped' END,answer_event_seq=CASE WHEN (SELECT status FROM work WHERE work_id=contest_answers.work_id)='completed' THEN (SELECT completed_event_seq FROM work WHERE work_id=contest_answers.work_id) ELSE NULL END WHERE contest_id=? AND outcome='pending' AND (SELECT status FROM work WHERE work_id=contest_answers.work_id) IN ('completed','failed','cancelled')",[contest.to_string()])?;
    Ok(())
}
pub fn request_pick(
    db: &Connection,
    contest: ContestId,
    seq: Cursor,
    eligible: &[MemberNick],
    required: u8,
) -> Result<()> {
    db.execute("INSERT INTO contest_picks(contest_id,requested_event_seq,eligible_json,required_count) VALUES (?,?,?,?)",params![contest.to_string(),seq.get(),encode(&eligible)?,required])?;
    Ok(())
}
pub fn accept(db: &Connection, picked: &ContestPicked, seq: Cursor) -> Result<()> {
    if db.execute("UPDATE contest_picks SET picked_json=?,picked_event_seq=? WHERE contest_id=? AND picked_json IS NULL",params![encode(&picked.nicks)?,seq.get(),picked.contest_id.to_string()])?!=1 {
        return Err(crate::StoreError::Domain(ErrorCode::CONTEST_PICK_INVALID,"Contest selection is already settled"));
    }
    Ok(())
}
