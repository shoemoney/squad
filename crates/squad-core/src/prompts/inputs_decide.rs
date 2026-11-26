//! Mapping accepts only an authorized, step-specific projection.
use squad_protocol::*;
pub fn inputs(
    _pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let schema = squad_protocol::decide::output_schema(&work.kind)
        .ok_or_else(|| super::PromptError::new("decide", None, "invalid decide kind"))?;
    let mut values = super::Inputs::from([
        ("mode_data_json".into(), data.clone()),
        ("mode_schema_json".into(), schema),
    ]);
    if let Some(r) = repair {
        values.insert(
            "rejected_output_json".into(),
            serde_json::json!(r.rejected_output),
        );
        values.insert(
            "validation_errors_json".into(),
            serde_json::json!(r.validation_errors),
        );
    }
    Ok(values)
}
