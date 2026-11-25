//! Probe compositions accept an already-authorized typed input, never a timeline.
use super::{Inputs, PromptError, PromptPack};
use serde_json::json;
use squad_protocol::*;
impl PromptPack {
    pub fn probe_inputs(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        input: &ProbeInput,
        repair: Option<&JudgeRepairData>,
    ) -> Result<(&'static str, Inputs), PromptError> {
        let error = || PromptError::new("probe", None, "invalid probe work or input");
        work.validate().map_err(|_| error())?;
        let id = work.kind.probe_composition().ok_or_else(error)?;
        let schema = match (&work.kind, input) {
            (WorkKind::ProbeExplain, ProbeInput::Propose { .. }) => {
                validation::schema::<ProbeExplanation>()
            }
            (WorkKind::ProbeCheck, ProbeInput::Propose { .. }) => {
                validation::schema::<ProbeCheck>()
            }
            (WorkKind::ProbePredict, ProbeInput::Predict { .. }) => {
                validation::schema::<ProbePrediction>()
            }
            (WorkKind::ProbeReport, ProbeInput::Report { .. }) => {
                validation::schema::<ProbeReport>()
            }
            _ => return Err(error()),
        };
        if repair.is_some_and(|r| work.kind != WorkKind::ProbeReport || r.attempt != work.attempt) {
            return Err(error());
        }
        // Blind contexts use no roster, slug or persona even though room.system
        // declares these shared slots. The human retains the original records.
        let values = Inputs::from([
            ("nick".into(), json!("participant")),
            ("model_slug".into(), json!("anonymous")),
            ("persona_json".into(), json!("")),
            ("member_nicks_json".into(), json!([])),
            ("user_name_json".into(), json!("human")),
            ("judge_model_slug".into(), json!(binding.model)),
            (
                "room_word_limit".into(),
                json!(config.prompts.limits.room_words),
            ),
            (
                "step_word_limit".into(),
                json!(if work.kind == WorkKind::ProbeReport {
                    config.prompts.limits.plan_words
                } else {
                    config.prompts.limits.room_words
                }),
            ),
            ("mode_step".into(), json!(id)),
            ("contest_id".into(), json!(work.contest_id)),
            ("mode_data_json".into(), json!(input)),
            ("mode_schema_json".into(), schema),
        ]);
        Ok((id, self.selected(id, values, repair)?))
    }
}
