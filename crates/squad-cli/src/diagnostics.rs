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

pub fn census_passes(
    data_dir: &Path,
    seed: Option<u8>,
    seed_source: &str,
) -> Result<String, String> {
    use rusqlite::OptionalExtension;
    let stored = if data_dir.join("squad.db").exists() {
        let db = open_readonly(data_dir).map_err(|_| "Could not read CENSUS_PASSES.".to_owned())?;
        db.query_row(
            "SELECT value FROM vars WHERE name='CENSUS_PASSES'",
            [],
            |r| r.get::<_, u8>(0),
        )
        .optional()
        .map_err(|_| "Could not read CENSUS_PASSES.".to_owned())?
    } else {
        None
    };
    let effective = stored.or(seed).map_or_else(
        || "unavailable (configuration invalid)".into(),
        |v| v.to_string(),
    );
    let source = if stored.is_some() {
        "persisted room value"
    } else {
        seed_source
    };
    Ok(format!(
        "CENSUS_PASSES: effective {effective}; source {source}; SQUAD_CENSUS_PASSES seed {}; default 2; range 1..=5; persisted room value takes precedence.\n",
        seed.map_or_else(|| "unavailable".into(), |v| v.to_string())
    ))
}

/// Storage ceiling report: used and capacity bytes, with a warning near or at the ceiling.
pub fn storage(data_dir: &Path, capacity: u64) -> Result<String, String> {
    let used = if data_dir.join("squad.db").exists() {
        let db = open_readonly(data_dir).map_err(|_| "Could not read storage use.".to_owned())?;
        squad_store::storage::used_bytes(&db)
            .map_err(|_| "Could not read storage use.".to_owned())?
    } else {
        0
    };
    let mib = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    let percent = used as f64 * 100.0 / capacity.max(1) as f64;
    let note = if used >= capacity {
        "; WARNING - at the ceiling: new generation is refused (STORAGE_LIMIT); history, export and cleanup stay available"
    } else if percent >= 90.0 {
        "; WARNING - near the ceiling"
    } else {
        ""
    };
    Ok(format!(
        "Storage: {:.1} MiB used of {:.1} MiB ({percent:.1}%){note}\n",
        mib(used),
        mib(capacity)
    ))
}
