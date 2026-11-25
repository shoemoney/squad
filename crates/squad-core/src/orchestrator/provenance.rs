//! Plan/case provenance consumes hashes from the effective prompt snapshot.
use super::types::{DebatePlan, FrozenPrompts};
use squad_protocol::*;

pub fn prompt_ref(snapshot: &FrozenPrompts, template: &str) -> Result<PromptRef, ErrorCode> {
    let hash = snapshot
        .composition_hashes
        .get(template)
        .ok_or(ErrorCode::DEBATE_PLAN_STALE)?;
    Ok(PromptRef {
        template_id: Text::new(template).map_err(|_| ErrorCode::CONFIG_INVALID)?,
        pack_version: Text::new(snapshot.pack_version.clone())
            .map_err(|_| ErrorCode::CONFIG_INVALID)?,
        template_hash: Text::new(hash.clone()).map_err(|_| ErrorCode::CONFIG_INVALID)?,
    })
}
pub fn judge(plan: &DebatePlan) -> Result<JudgeProvenance, ErrorCode> {
    let model = plan.judge.as_ref().ok_or(ErrorCode::MODEL_UNAVAILABLE)?;
    Ok(JudgeProvenance {
        model: model.resolved_slug.clone(),
        prompt_pack_version: plan.prompts.pack_version.clone(),
        rendered_template_hashes: plan.prompts.composition_hashes.clone(),
    })
}
pub fn encode_plan(plan: &DebatePlan) -> Result<serde_json::Value, ErrorCode> {
    plan.preflight
        .validate()
        .map_err(|_| ErrorCode::DEBATE_PLAN_STALE)?;
    serde_json::to_value(plan).map_err(|_| ErrorCode::INTERNAL_ERROR)
}
