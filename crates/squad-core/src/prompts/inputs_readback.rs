//! Typed slots for authorized readback data; no timeline lookup.
use squad_protocol::*;
pub fn inputs(
    pack: &super::PromptPack,
    work: &Work,
    data: &serde_json::Value,
    repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    let name = readback::composition(&work.kind)
        .ok_or_else(|| super::PromptError::new("readback", None, "invalid readback work"))?;
    pack.selected(
        name,
        super::Inputs::from([
            ("mode_data_json".into(), data.clone()),
            (
                "mode_schema_json".into(),
                readback::output_schema(&work.kind).expect("schema"),
            ),
            ("member_nicks_json".into(), serde_json::json!([])),
            ("user_name_json".into(), serde_json::json!("the user")),
            ("persona_json".into(), serde_json::json!("")),
            ("room_word_limit".into(), serde_json::json!(500)),
            ("nick".into(), serde_json::json!(work.target.to_string())),
            ("model_slug".into(), serde_json::json!("anonymous")),
            ("judge_model_slug".into(), serde_json::json!("anonymous")),
        ]),
        repair,
    )
}
