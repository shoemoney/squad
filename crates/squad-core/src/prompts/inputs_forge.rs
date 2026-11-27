//! Mapping consumes only a work's frozen anonymous projection.
use serde_json::json;
use squad_protocol::*;
pub fn inputs(
    pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let name = squad_store::forge::composition(&work.kind)
        .ok_or_else(|| super::PromptError::new("forge", None, "invalid work"))?;
    let values = values(work, data);
    pack.selected(name, values, repair)
}
fn values(work: &Work, data: &serde_json::Value) -> super::Inputs {
    super::Inputs::from([
        (
            "mode_step".into(),
            json!(squad_store::forge::composition(&work.kind)),
        ),
        ("contest_id".into(), json!(work.contest_id)),
        ("mode_data_json".into(), data.clone()),
        (
            "mode_schema_json".into(),
            forge::output_schema(&work.kind).expect("schema"),
        ),
        ("step_word_limit".into(), data["wordLimit"].clone()),
        ("nick".into(), json!("anonymous")),
        ("model_slug".into(), json!("anonymous")),
        ("judge_model_slug".into(), json!("anonymous")),
        ("persona_json".into(), json!("")),
        ("user_name_json".into(), json!("the user")),
        ("member_nicks_json".into(), json!([])),
        ("room_word_limit".into(), data["wordLimit"].clone()),
    ])
}
/// Filter against admitted source text, even after a prompt override changes on disk.
pub fn frozen(
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
    system: &str,
    user: &str,
) -> Result<super::Inputs, super::PromptError> {
    let mut values = values(work, data);
    if let Some(r) = repair {
        values.insert("invalid_output_json".into(), json!(r.rejected_output));
        values.insert("validation_errors_json".into(), json!(r.validation_errors));
    }
    let required = super::render::placeholders("forge", &format!("{system}\n{user}"))?;
    values.retain(|k, _| required.contains(k));
    Ok(values)
}
