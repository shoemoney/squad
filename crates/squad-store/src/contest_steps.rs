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
    for source in &step.inputs {
        if crate::events::by_id(db, *source)?.is_none() {
            return Err(invalid("Step source is not retained"));
        }
    }
    if step
        .assignments
        .iter()
        .any(|a| a.payload_refs.iter().any(|r| !step.inputs.contains(r)))
    {
        return Err(invalid("Assignment source is not a frozen step input"));
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

/// Validate both identities before reading or persisting a step artifact.
pub fn validate_step(db: &Connection, contest: ContestId, step: StepId) -> Result<()> {
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM contest_steps s JOIN contests c ON c.contest_id=s.contest_id WHERE s.step_id=? AND s.contest_id=? AND c.epoch=(SELECT epoch FROM room_meta))",params![step.to_string(),contest.to_string()],|r|r.get(0))?;
    if !valid {
        return Err(invalid("Step does not belong to contest"));
    }
    Ok(())
}
pub fn validate_artifact(db: &Connection, artifact: &ContestArtifact) -> Result<()> {
    validate_step(db, artifact.contest_id, artifact.step_id)?;
    if let Some(id) = artifact.work_id {
        let (work, _) =
            crate::work::template(db, id)?.ok_or_else(|| invalid("Unknown artifact work"))?;
        if work.contest_id != Some(artifact.contest_id) || work.step_id != Some(artifact.step_id) {
            return Err(invalid("Artifact assignment mismatch"));
        }
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM contest_assignments WHERE step_id=? AND work_id=? AND target=? AND outcome='pending')",params![artifact.step_id.to_string(),id.to_string(),work.target.to_string()],|r|r.get(0))?;
        if !valid {
            return Err(invalid("Artifact assignment is not pending"));
        }
        let raw: String = db.query_row(
            "SELECT assignment_json FROM contest_assignments WHERE step_id=? AND work_id=?",
            params![artifact.step_id.to_string(), id.to_string()],
            |r| r.get(0),
        )?;
        let assignment: ContestAssignment = crate::error::decode(raw)?;
        if artifact
            .source_refs
            .iter()
            .any(|r| !assignment.payload_refs.contains(r))
        {
            return Err(invalid("Artifact source is not assigned to work"));
        }
    }
    for id in &artifact.source_refs {
        if crate::events::by_id(db, *id)?.is_none() {
            return Err(invalid("Unknown artifact source"));
        }
    }
    Ok(())
}
pub fn artifact(db: &Connection, artifact: &ContestArtifact, seq: Cursor) -> Result<()> {
    validate_artifact(db, artifact)?;
    db.execute(
        "INSERT INTO mode_artifacts VALUES (?,?,?,?,?,?)",
        params![
            seq.get(),
            artifact.contest_id.to_string(),
            artifact.step_id.to_string(),
            artifact.artifact_kind.as_str(),
            artifact.work_id.map(|id| id.to_string()),
            encode(&artifact.source_refs)?
        ],
    )?;
    if let Some(id) = artifact.work_id {
        let (work, _) =
            crate::work::template(db, id)?.ok_or_else(|| invalid("Unknown artifact work"))?;
        settle(db, artifact.step_id, &work.target, None)?;
        crate::work::finish(db, id, WorkStatus::Completed, seq)?;
    }
    let kind: String = db.query_row("SELECT kind FROM contests WHERE contest_id=?", [artifact.contest_id.to_string()], |r|r.get(0))?;
    let kind: ContestKind = serde_json::from_value(serde_json::Value::String(kind))?;
    crate::contests::associate(db, artifact.contest_id, kind, seq)
}

/// A human report may reference only retained artifacts from its own contest.
pub fn validate_report(db: &Connection, report: &ContestReport) -> Result<()> {
    validate_step(db, report.contest_id, report.step_id)?;
    for id in &report.artifact_event_ids {
        let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM mode_artifacts a JOIN events e ON e.seq=a.event_seq WHERE a.contest_id=? AND e.id=?)",params![report.contest_id.to_string(),id.to_string()],|r|r.get(0))?;
        if !valid {
            return Err(invalid("Report artifact is not in this contest"));
        }
    }
    Ok(())
}
