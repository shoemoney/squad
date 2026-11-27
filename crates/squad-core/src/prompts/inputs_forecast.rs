//! Map only the frozen, work-authorized step projection.
use squad_protocol::*;
pub fn inputs(
    _pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    _repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let schema = forecast::output_schema(&work.kind)
        .ok_or_else(|| super::PromptError::new("forecast", None, "not forecast work"))?;
    Ok(std::collections::BTreeMap::from([
        ("mode_data_json".into(), data.clone()),
        ("mode_schema_json".into(), schema),
    ]))
}
