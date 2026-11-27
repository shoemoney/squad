//! Mapping from one authorized step projection; no ambient room access.
use squad_protocol::*;
pub fn inputs(
    _pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    _repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let schema = squad_protocol::bass_ackwards_prd::output_schema(&work.kind)
        .ok_or_else(|| super::PromptError::new("bass_ackwards_prd", None, "not PRD work"))?;
    Ok(std::collections::BTreeMap::from([
        ("mode_data_json".into(), data.clone()),
        ("mode_schema_json".into(), schema),
    ]))
}

/// Select one lens block before sending a review request. Other lenses never
/// enter that member's prompt, including the instruction text.
pub fn select_lens(source: &str, lens: &str) -> squad_store::Result<String> {
    let invalid = || {
        squad_store::StoreError::Domain(
            ErrorCode::CONFIG_INVALID,
            "Invalid PRD review lens template",
        )
    };
    if !["GAPS", "ATTACK", "READBACK"].contains(&lens) {
        return Err(invalid());
    }
    let mut out = source.to_owned();
    for name in ["GAPS", "ATTACK", "READBACK"] {
        let start = format!("[{name}]");
        let end = format!("[/{name}]");
        if out.matches(&start).count() != 1 || out.matches(&end).count() != 1 {
            return Err(invalid());
        }
        let a = out.find(&start).ok_or_else(invalid)?;
        let b = out.find(&end).ok_or_else(invalid)?;
        if b < a {
            return Err(invalid());
        }
        let selected = if name == lens {
            out[a + start.len()..b].to_owned()
        } else {
            String::new()
        };
        out.replace_range(a..b + end.len(), &selected);
    }
    Ok(out)
}
