use crate::{
    Result,
    error::{decode, encode, invalid},
};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
pub fn save(db: &Connection, id: WorkId, case: CaseId, data: &JudgeRepairData) -> Result<()> {
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE work_id=? AND case_id=? AND attempt=? AND kind IN ('judge','judge.round','judge.tournament','census.merge','census.final'))",params![id.to_string(),case.to_string(),data.attempt],|r|r.get(0))?;
    if !valid {
        return Err(invalid("Repair does not belong to case work"));
    }
    db.execute(
        "INSERT INTO judge_repairs VALUES (?1,?2,?3,?4,?5)",
        params![
            id.to_string(),
            data.attempt,
            case.to_string(),
            data.rejected_output,
            encode(&data.validation_errors)?
        ],
    )?;
    Ok(())
}
pub fn get(
    db: &Connection,
    id: WorkId,
    attempt: u32,
    case: CaseId,
) -> Result<Option<JudgeRepairData>> {
    let row:Option<(String,String)>=db.query_row("SELECT rejected_output,validation_errors_json FROM judge_repairs WHERE work_id=? AND attempt=? AND case_id=?",params![id.to_string(),attempt,case.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    row.map(|(rejected_output, errors)| {
        Ok(JudgeRepairData {
            attempt,
            rejected_output,
            validation_errors: decode(errors)?,
        })
    })
    .transpose()
}
