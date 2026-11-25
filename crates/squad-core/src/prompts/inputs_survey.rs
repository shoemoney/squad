//! Stage-specific mappings: member payloads cannot carry a provenance map.
use super::{Inputs, PromptError, PromptPack};
use serde_json::{Value, json};
use squad_protocol::*;
use std::collections::BTreeMap;
pub fn task_values(
    contest: ContestId,
    data: Value,
    kind: &WorkKind,
    words: u32,
) -> BTreeMap<String, String> {
    let mut schema = match kind {
        WorkKind::SurveyExtract => validation::schema::<SurveyExtraction>(),
        WorkKind::SurveyReport => validation::schema::<SurveyReportText>(),
        _ => validation::schema::<SurveyBallot>(),
    };
    match kind {
        WorkKind::SurveyExtract => {
            schema["properties"]["claims"]["maxItems"] = json!(25);
            schema["$defs"]["SurveyClaim"]["properties"]["sourceAnswerIds"]["minItems"] = json!(1);
            schema["$defs"]["SurveyClaim"]["properties"]["sourceAnswerIds"]["uniqueItems"] =
                json!(true);
            schema["$defs"]["SurveyClaim"]["properties"]["sourceAnswerIds"]["items"]["enum"] = json!(
                data["answers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| a["messageId"].clone())
                    .collect::<Vec<_>>()
            );
        }
        WorkKind::SurveyVote | WorkKind::SurveyRevote => {
            let claims = data["claims"].as_array().map(Vec::as_slice).unwrap_or(&[]);
            schema["properties"]["votes"]["minItems"] = json!(claims.len());
            schema["properties"]["votes"]["maxItems"] = json!(claims.len());
            schema["$defs"]["SurveyBallotItem"]["properties"]["claimId"]["enum"] = json!(
                claims
                    .iter()
                    .map(|c| c["claimId"].clone())
                    .collect::<Vec<_>>()
            );
            if *kind == WorkKind::SurveyRevote {
                schema["$defs"]["SurveyBallotItem"]["required"]
                    .as_array_mut()
                    .expect("object schema")
                    .push(json!("changeEvidence"));
                schema["$defs"]["SurveyBallotItem"]["properties"]["changeEvidence"] =
                    json!({"type":"string","minLength":1});
            } else {
                schema["$defs"]["SurveyBallotItem"]["properties"]
                    .as_object_mut()
                    .expect("object schema")
                    .remove("changeEvidence");
            }
        }
        _ => {}
    }
    BTreeMap::from([
        ("contest_id".into(), contest.to_string()),
        ("mode_data_json".into(), data.to_string()),
        ("mode_schema_json".into(), schema.to_string()),
        ("step_word_limit".into(), words.to_string()),
    ])
}
pub fn vote_data(setup: &SurveySetup, claims: &SurveyExtraction) -> Value {
    json!({"question":setup.question,"claims":claims.claims.iter().map(|c|json!({"claimId":c.id,"claim":crate::orchestrator::survey::blind_text(&c.claim,setup)})).collect::<Vec<_>>()})
}
pub fn revote_data(setup: &SurveySetup, selection: &SurveySelection) -> Value {
    json!({"question":setup.question,"claims":selection.claims})
}
impl PromptPack {
    pub fn survey_inputs(
        &self,
        config: &SquadConfig,
        work: &Work,
        binding: &ExecutorBinding,
        values: &BTreeMap<String, String>,
        repair: Option<&JudgeRepairData>,
    ) -> Result<(&'static str, Inputs), PromptError> {
        let error = || PromptError::new("survey", None, "invalid survey work inputs");
        work.validate().map_err(|_| error())?;
        let id = match work.kind {
            WorkKind::SurveyExtract => "survey.extract",
            WorkKind::SurveyVote => "survey.vote",
            WorkKind::SurveyRevote => "survey.revote",
            WorkKind::SurveyReport => "survey.report",
            _ => return Err(error()),
        };
        if values.get("contest_id") != work.contest_id.map(|id| id.to_string()).as_ref() {
            return Err(error());
        }
        let mut input = if work.target == Target::Judge {
            let mut input = super::inputs::base(config);
            input.insert("judge_model_slug".into(), json!(binding.model));
            input
        } else {
            super::inputs::member_inputs(config, work, binding)?
        };
        for (key, value) in values {
            input.insert(
                key.clone(),
                if key.ends_with("_json") {
                    serde_json::from_str(value).map_err(|_| error())?
                } else if key == "step_word_limit" {
                    json!(value.parse::<u32>().map_err(|_| error())?)
                } else {
                    json!(value)
                },
            );
        }
        Ok((id, self.selected(id, input, repair)?))
    }
}
