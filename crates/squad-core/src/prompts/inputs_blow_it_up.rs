//! Only step-authorized data enters these inputs; no roster, persona or history lookup.
use serde_json::json;
use squad_protocol::*;
fn values(work: &Work, data: &serde_json::Value) -> Result<super::Inputs, super::PromptError> {
    let error = || super::PromptError::new("blow_it_up", None, "invalid mode input");
    let id = squad_protocol::blow_it_up::composition(&work.kind).ok_or_else(error)?;
    let schema = squad_protocol::blow_it_up::output_schema(&work.kind).ok_or_else(error)?;
    let values = super::Inputs::from([
        ("nick".into(), json!("participant")),
        ("model_slug".into(), json!("anonymous")),
        ("judge_model_slug".into(), json!("anonymous")),
        ("persona_json".into(), json!("")),
        ("member_nicks_json".into(), json!([])),
        ("user_name_json".into(), json!("human")),
        ("room_word_limit".into(), json!(data["wordLimit"])),
        ("step_word_limit".into(), json!(data["wordLimit"])),
        ("mode_step".into(), json!(id)),
        ("mode_data_json".into(), data.clone()),
        ("mode_schema_json".into(), schema),
    ]);
    Ok(values)
}

pub fn inputs(
    pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let id = squad_protocol::blow_it_up::composition(&work.kind)
        .ok_or_else(|| super::PromptError::new("blow_it_up", None, "wrong work"))?;
    pack.selected(id, values(work, data)?, repair)
}
pub fn frozen_inputs(
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
    system: &str,
    user: &str,
) -> Result<super::Inputs, super::PromptError> {
    let mut values = values(work, data)?;
    if let Some(r) = repair {
        values.insert("invalid_output_json".into(), json!(r.rejected_output));
        values.insert("validation_errors_json".into(), json!(r.validation_errors));
    }
    let required = super::render::placeholders("blow_it_up", &format!("{system}\n{user}"))?;
    values.retain(|k, _| required.contains(k));
    Ok(values)
}
