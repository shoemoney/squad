//! tune authorized prompt mapping belongs here.
use squad_protocol::*;
pub fn inputs(
    _pack: &super::PromptPack,
    _work: &Work,
    _data: &serde_json::Value,
    _repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    Err(super::PromptError::new("tune", None, "mode is disabled"))
}
