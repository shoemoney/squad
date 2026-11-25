//! Crosscheck compositions accept only the step's typed authorized material.
use super::{Inputs, PromptError, PromptPack};
use serde_json::json;
use squad_protocol::*;
impl PromptPack {
    pub fn crosscheck_inputs(
        &self,
        config: &SquadConfig,
        binding: &ExecutorBinding,
        work: &Work,
        material: &CrosscheckCaseMaterial,
        repair: Option<&JudgeRepairData>,
    ) -> Result<Inputs, PromptError> {
        work.validate()
            .map_err(|_| PromptError::new("crosscheck", None, "invalid work"))?;
        let judge = matches!(
            material,
            CrosscheckCaseMaterial::ExtractGoal { .. }
                | CrosscheckCaseMaterial::NormalizeInventory { .. }
                | CrosscheckCaseMaterial::Rule { .. }
        );
        if work.kind != material.work_kind() {
            return Err(PromptError::new("crosscheck", None, "task target mismatch"));
        }
        let mut values = Inputs::from([
            ("contest_id".into(), json!(work.contest_id)),
            ("mode_data_json".into(), json!(material)),
            ("mode_schema_json".into(), material.response_schema()),
            (
                "step_word_limit".into(),
                json!(if judge {
                    config.prompts.limits.plan_words
                } else {
                    config.prompts.limits.room_words
                }),
            ),
        ]);
        values.insert(
            "member_nicks_json".into(),
            json!(
                config
                    .agents
                    .iter()
                    .map(|a| a.nick.as_str())
                    .collect::<Vec<_>>()
            ),
        );
        values.insert("user_name_json".into(), json!(config.user.name));
        values.insert("judge_model_slug".into(), json!(binding.model));
        if let Target::Member(nick) = &work.target {
            values.insert("nick".into(), json!(nick));
            values.insert("model_slug".into(), json!(binding.model));
            values.insert(
                "room_word_limit".into(),
                json!(config.prompts.limits.room_words),
            );
            values.insert(
                "persona_json".into(),
                json!(
                    config
                        .agents
                        .iter()
                        .find(|a| a.nick == *nick)
                        .map(|a| a.persona.as_str())
                        .unwrap_or("")
                ),
            );
        }
        if let Some(repair) = repair {
            values.insert("mode_step".into(), json!(material.composition()));
            values.insert("invalid_output_json".into(), json!(repair.rejected_output));
            values.insert(
                "validation_errors_json".into(),
                json!(repair.validation_errors),
            );
        }
        let (system, user) = self.composition_source(material.composition(), repair.is_some())?;
        let slots =
            super::render::placeholders(material.composition(), &format!("{system}\n{user}"))?;
        values.retain(|k, _| slots.contains(k));
        Ok(values)
    }
}
