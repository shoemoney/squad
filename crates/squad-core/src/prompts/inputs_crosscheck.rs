//! Crosscheck compositions accept only the step's typed authorized material.
use super::{Inputs, PromptError, PromptPack};
use serde_json::json;
use squad_protocol::*;
impl PromptPack {
    pub fn crosscheck_inputs(&self, work: &Work, material: &CrosscheckCaseMaterial, repair: Option<&JudgeRepairData>) -> Result<Inputs,PromptError> {
        work.validate().map_err(|_|PromptError::new("crosscheck",None,"invalid work"))?;
        let judge = matches!(material,CrosscheckCaseMaterial::ExtractGoal{..}|CrosscheckCaseMaterial::NormalizeInventory{..}|CrosscheckCaseMaterial::Rule{..});
        if (judge && work.kind!=WorkKind::CrosscheckJudge) || (!judge && work.kind!=WorkKind::CrosscheckMember) {return Err(PromptError::new("crosscheck",None,"task target mismatch"));}
        let mut values=Inputs::from([("contest_id".into(),json!(work.contest_id)),("mode_data_json".into(),json!(material)),("mode_schema_json".into(),material.response_schema()),("step_word_limit".into(),json!(2400))]);
        if let Some(repair)=repair {values.insert("invalid_output_json".into(),json!(repair.rejected_output));values.insert("validation_errors_json".into(),json!(repair.validation_errors));}
        Ok(values)
    }
}
