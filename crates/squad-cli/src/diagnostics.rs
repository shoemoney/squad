//! Read-only diagnostic projections; callers run these independently.
use crate::export::open_readonly;
use std::{fmt::Write, path::Path};

pub fn accounting(data_dir: &Path, day: &str, limit: f64) -> Result<String, String> {
    if !data_dir.join("squad.db").exists() {
        return Ok(format!(
            "Daily accounting: {day}; actual $0.00; reserved $0.00; effective max ${limit:.2}; unknown-cost calls: 0 (no ledger yet)\n"
        ));
    }
    let db = open_readonly(data_dir).map_err(|_| "Could not read daily accounting.".to_owned())?;
    let spend = squad_store::accounting::daily(&db, day)
        .map_err(|_| "Could not read daily accounting.".to_owned())?;
    Ok(format!(
        "Daily accounting: {day}; actual ${:.2}; reserved ${:.2}; effective max ${limit:.2}; unknown-cost calls: {}{}\n",
        spend.spent_micros as f64 / 1e6,
        spend.reserved_micros as f64 / 1e6,
        spend.unknown_calls,
        if spend.unknown_calls > 0 {
            "; unreconciled charges (~)"
        } else {
            ""
        }
    ))
}

pub fn identities(data_dir: &Path) -> Result<String, String> {
    if !data_dir.join("squad.db").exists() {
        return Ok("Model identity: no recorded attempts.\n".into());
    }
    let db = open_readonly(data_dir).map_err(|_| "Could not read model identities.".to_owned())?;
    let mut query = db.prepare("SELECT work_id,attempt,model,response_model FROM attempts WHERE response_model IS NOT NULL AND response_model != model ORDER BY work_id,attempt").map_err(|_| "Could not read model identities.".to_owned())?;
    let rows = query
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| "Could not read model identities.".to_owned())?;
    let mut text = String::new();
    for row in rows {
        let (work, attempt, requested, response) =
            row.map_err(|_| "Could not read model identities.".to_owned())?;
        let _ = writeln!(
            text,
            "Model identity: WARNING - work {work}, attempt {attempt}: selected {requested}; response {response}"
        );
    }
    if text.is_empty() {
        text.push_str("Model identity: no recorded response-model mismatches.\n");
    }
    Ok(text)
}
