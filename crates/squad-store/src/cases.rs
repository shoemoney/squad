use crate::{
    Result,
    error::{encode, invalid},
};
use rusqlite::{Connection, params};
use squad_protocol::*;
pub fn open(db: &Connection, opened: &CaseOpened, seq: Cursor) -> Result<()> {
    let contest = match &opened.material {
        CaseMaterial::Debate { setup, .. } => Some(setup.debate_id.to_string()),
        _ => None,
    };
    db.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?1,?2,(SELECT epoch FROM room_meta),?3,?4,?5,?6,?7)",params![opened.case_id.to_string(),format!("judge:{}",opened.case_id),contest,opened.judge_model.as_str(),encode(&opened.material)?,seq.get(),serde_json::json!({"judgeInvocationId":opened.judge_invocation_id,"priorTerminalStatus":null}).to_string()])?;
    associate(db, opened.case_id, seq)
}
pub fn associate(db: &Connection, id: CaseId, seq: Cursor) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO case_events VALUES (?,?)",
        params![id.to_string(), seq.get()],
    )?;
    Ok(())
}
pub fn seal(db: &Connection, id: CaseId, seq: Cursor) -> Result<()> {
    if db.execute(
        "UPDATE cases SET decision_event_seq=? WHERE case_id=? AND decision_event_seq IS NULL",
        params![seq.get(), id.to_string()],
    )? != 1
    {
        return Err(invalid("Case already sealed"));
    }
    associate(db, id, seq)
}
