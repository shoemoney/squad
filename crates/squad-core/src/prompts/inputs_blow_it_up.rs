//! blow_it_up authorized prompt mapping belongs here.
use squad_protocol::*;
pub fn inputs(
    _pack: &super::PromptPack,
    _work: &Work,
    _data: &serde_json::Value,
    _repair: Option<&JudgeRepairData>,
) -> Result<super::Inputs, super::PromptError> {
    Err(super::PromptError::new(
        "blow_it_up",
        None,
        "mode is disabled",
    ))
}
