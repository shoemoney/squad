//! Frozen generic assignments; this layer confers no admission authority for unbuilt modes.
use crate::{
    Result,
    error::{encode, invalid},
};
use rusqlite::{Connection, params};
use squad_protocol::*;
pub fn insert(db: &Connection, step: &ContestStep, seq: Cursor) -> Result<()> {
    if step.ordinal == 0
        || step.assignments.iter().enumerate().any(|(i, a)| {
            step.assignments[..i]
                .iter()
                .any(|old| old.target == a.target)
        })
    {
        return Err(invalid("Invalid frozen step assignments"));
    }
    let move_name = serde_json::to_value(&step.move_name)?;
    db.execute("INSERT INTO contest_steps(step_id,contest_id,ordinal,script_version,move,state_json,inputs_json,updated_seq) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![step.step_id.to_string(),step.contest_id.to_string(),step.ordinal,step.script_version.as_str(),move_name.as_str(),encode(step)?,encode(&step.inputs)?,seq.get()])?;
    for assignment in &step.assignments {
        if let Target::Member(nick) = &assignment.target {
            let expected = format!("contest:{}:{}:{}", step.contest_id, step.step_id, nick);
            if assignment.context_id.0 != expected {
                return Err(invalid("Assignment context is not step-bound"));
            }
        }
        // work_id is attached after insertion to preserve the circular FK relationship.
        db.execute("INSERT INTO contest_assignments(step_id,target,context_id,payload_refs_json,assignment_json,outcome) VALUES (?1,?2,?3,?4,?5,'pending')",params![step.step_id.to_string(),assignment.target.to_string(),assignment.context_id.0,encode(&assignment.payload_refs)?,encode(assignment)?])?;
    }
    Ok(())
}
pub fn attach_work(db: &Connection, step: StepId, work: &Work) -> Result<()> {
    if work.step_id != Some(step) {
        return Err(invalid("Assignment work step mismatch"));
    }
    if db.execute("UPDATE contest_assignments SET work_id=?1 WHERE step_id=?2 AND target=?3 AND work_id IS NULL AND context_id=?4",params![work.work_id.to_string(),step.to_string(),work.target.to_string(),work.context_id.0])?!=1 {return Err(invalid("Assignment has no available work slot"));}
    Ok(())
}
pub fn private_map(
    db: &Connection,
    contest: ContestId,
    scope: &str,
    value: &serde_json::Value,
) -> Result<()> {
    if !value.is_object() {
        return Err(invalid("Private map must be an object"));
    }
    db.execute(
        "INSERT INTO contest_private_maps VALUES (?1,?2,?3)",
        params![contest.to_string(), scope, encode(value)?],
    )?;
    Ok(())
}
pub fn settle(
    db: &Connection,
    step: StepId,
    target: &Target,
    skipped: Option<&ErrorPayload>,
) -> Result<()> {
    let outcome = if skipped.is_some() {
        "skipped"
    } else {
        "completed"
    };
    if db.execute("UPDATE contest_assignments SET outcome=?1,error_json=?2 WHERE step_id=?3 AND target=?4 AND outcome='pending'",params![outcome,skipped.map(encode).transpose()?,step.to_string(),target.to_string()])?!=1 {return Err(invalid("Assignment already settled"));}
    Ok(())
}
pub fn barrier(db: &Connection, step: StepId, seq: Cursor) -> Result<()> {
    if db.execute("UPDATE contest_steps SET committed=1,updated_seq=?1 WHERE step_id=?2 AND committed=0 AND NOT EXISTS(SELECT 1 FROM contest_assignments WHERE step_id=?2 AND outcome='pending')",params![seq.get(),step.to_string()])?!=1 {return Err(invalid("Barrier is not ready or already committed"));}
    Ok(())
}
