//! Typed, authorized tune projections; blind compositions receive no identities.
use serde_json::json;
use squad_protocol::tune::*;
use squad_protocol::*;
pub fn inputs(
    pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let id = work_name(&work.kind);
    let schema = tune::output_schema(&work.kind)
        .ok_or_else(|| super::PromptError::new("tune", None, "executor has no template"))?;
    let values = super::Inputs::from([
        ("nick".into(), json!("participant")),
        ("model_slug".into(), json!("anonymous")),
        ("persona_json".into(), json!("")),
        ("member_nicks_json".into(), json!([])),
        ("user_name_json".into(), json!("human")),
        ("judge_model_slug".into(), json!("anonymous")),
        ("room_word_limit".into(), json!(300)),
        ("step_word_limit".into(), json!(600)),
        ("mode_step".into(), json!(id)),
        ("contest_id".into(), json!(work.contest_id)),
        ("mode_data_json".into(), data.clone()),
        ("mode_schema_json".into(), schema),
    ]);
    pack.selected(id, values, repair)
}
