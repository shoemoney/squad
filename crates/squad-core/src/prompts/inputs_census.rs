//! Census-only authorized prompt mapping; no history or private-map lookups.
use squad_protocol::*;
use squad_store::{Result, StoreError};
use std::collections::BTreeMap;
pub fn values(
    state: &CensusState,
    kind: &WorkKind,
    data: &serde_json::Value,
    words: u32,
) -> Result<BTreeMap<String, String>> {
    let schema = match kind {
        WorkKind::CensusHunt => validation::schema::<CensusHuntOutput>(),
        WorkKind::CensusMerge => validation::schema::<CensusMergeOutput>(),
        WorkKind::CensusRate => validation::schema::<CensusRatingOutput>(),
        WorkKind::CensusFinal => validation::schema::<CensusFinalOutput>(),
        _ => {
            return Err(StoreError::Domain(
                ErrorCode::INVALID_COMMAND,
                "Not census work",
            ));
        }
    };
    Ok(BTreeMap::from([
        ("contest_id".into(), state.contest_id.to_string()),
        ("census_pass".into(), state.pass.to_string()),
        ("plan_version".into(), state.plan_version.to_string()),
        ("step_word_limit".into(), words.to_string()),
        ("mode_schema_json".into(), serde_json::to_string(&schema)?),
        ("mode_data_json".into(), serde_json::to_string(data)?),
        (
            "mode_context_json".into(),
            serde_json::json!({"kind":"census","pass":state.pass,"planVersion":state.plan_version})
                .to_string(),
        ),
        ("member_nicks_json".into(), "[]".into()),
        ("user_name_json".into(), "\"the user\"".into()),
        ("persona_json".into(), "\"\"".into()),
        ("room_word_limit".into(), words.to_string()),
    ]))
}

/// Substitute source tokens once. Inserted source data is never scanned again.
pub fn render_source(source: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let invalid = || StoreError::Domain(ErrorCode::CONFIG_INVALID, "Invalid frozen census prompt");
    let mut out = String::new();
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let (key, tail) = after.split_once("}}").ok_or_else(invalid)?;
        out.push_str(values.get(key).ok_or_else(invalid)?);
        rest = tail;
    }
    out.push_str(rest);
    Ok(out)
}
