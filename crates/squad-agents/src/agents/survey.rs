//! One isolated survey invocation. All scheduling and tallying remain in core.
use super::*;
use crate::{
    accounting::acknowledged,
    context::judge_messages,
    worker::{ExecutionFuture, WorkExecutor},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub struct SurveyExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub system: PromptRef,
    pub system_values: BTreeMap<String, String>,
    pub context_length: u64,
}
impl WorkExecutor for SurveyExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let values = context
                .work
                .prompt_values_for_work(grant.work.work_id)
                .await?;
            let data: Value =
                serde_json::from_str(values.get("mode_data_json").ok_or(ReaderError::Forbidden)?)
                    .map_err(|_| ReaderError::Forbidden)?;
            let judge = matches!(
                grant.work.kind,
                WorkKind::SurveyExtract | WorkKind::SurveyReport
            );
            let mut values = values;
            // Render the task and repair suffix together through the registered composition.
            if judge
                && let Some(repair) = context
                    .work
                    .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                    .await?
            {
                values.insert(
                    "mode_step".into(),
                    grant
                        .work
                        .prompt_ref
                        .as_ref()
                        .ok_or(ReaderError::Forbidden)?
                        .template_id
                        .as_str()
                        .trim_end_matches(".md")
                        .to_owned(),
                );
                values.insert(
                    "invalid_output_json".into(),
                    json!(repair.rejected_output).to_string(),
                );
                values.insert(
                    "validation_errors_json".into(),
                    json!(repair.validation_errors).to_string(),
                );
            }
            let mut sysvalues = self.system_values.clone();
            sysvalues.insert(
                if judge {
                    "judge_model_slug"
                } else {
                    "model_slug"
                }
                .into(),
                grant.binding.model.to_string(),
            );
            let system = context.render.render(&self.system, &sysvalues)?;
            let template = grant
                .work
                .prompt_ref
                .as_ref()
                .ok_or(ReaderError::Forbidden)?;
            if judge {
                values.extend(sysvalues);
            }
            let rendered = context.render.render(template, &values)?;
            let user = serde_json::from_str::<Value>(&rendered)
                .ok()
                .and_then(|v| v["user"].as_str().map(String::from))
                .unwrap_or(rendered);
            let mut request = self.request.clone();
            request.messages = match judge_messages(
                system,
                user,
                self.context_length,
                u64::from(request.visible_ceiling) + 8192,
            ) {
                Ok(messages) => messages,
                Err(_) => return failed(context, &grant, ErrorCode::CONTEXT_TOO_LARGE).await,
            };
            request.response_format = Some(
                json!({"type":"json_schema","json_schema":{"name":"survey","strict":true,"schema":serde_json::from_str::<Value>(values.get("mode_schema_json").ok_or(ReaderError::Forbidden)?).map_err(|_|ReaderError::Forbidden)?}}),
            );
            let output =
                match stream_attempt(context, &grant, &self.client, request, cancel, false).await {
                    Ok(out) => out,
                    Err(ExecutionError::Reader(e)) => return Err(e),
                    Err(_) => return failed(context, &grant, ErrorCode::PROVIDER_ERROR).await,
                };
            let completed = CompletedGeneration {
                generation: identity(&grant),
                model: grant.binding.model.clone(),
                finish_reason: output.finish_reason,
                usage: output.usage,
            };
            let word_limit = values
                .get("step_word_limit")
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or(ReaderError::Forbidden)?;
            let validated = validate(&grant.work, &data, &output.content, completed, word_limit);
            match validated {
                Ok(output) => acknowledged(
                    context
                        .submit
                        .submit(ExecutorRequest::Result {
                            work_id: grant.work.work_id,
                            attempt: grant.work.attempt,
                            fence: grant.fence,
                            epoch: grant.epoch,
                            output,
                        })
                        .await,
                ),
                Err(validation_error) => {
                    let repair = if judge && grant.work.attempt < 3 {
                        Some(JudgeRepairData {
                            attempt: grant.work.attempt + 1,
                            rejected_output: output.content,
                            validation_errors: vec![validation_error.to_string()],
                        })
                    } else {
                        None
                    };
                    let error = ErrorPayload {
                        code: ErrorCode::JUDGE_INVALID_VERDICT,
                        text: Text::new(format!(
                            "Survey output failed validation: {validation_error}"
                        ))
                        .expect("safe diagnostic"),
                        retryable: false,
                        related_event_id: None,
                        work_id: Some(grant.work.work_id),
                        debate_id: None,
                        case_id: grant.work.case_id,
                        earliest_cursor: None,
                        retry_action: None,
                        contest_id: grant.work.contest_id,
                        provider_status: None,
                        provider_message: None,
                    };
                    acknowledged(
                        context
                            .submit
                            .submit(ExecutorRequest::Failed {
                                work_id: grant.work.work_id,
                                attempt: grant.work.attempt,
                                fence: grant.fence,
                                epoch: grant.epoch,
                                error,
                                repair,
                            })
                            .await,
                    )
                }
            }
        })
    }
}
#[derive(Debug, Clone)]
pub struct InvalidSurveyOutput(String);
impl std::fmt::Display for InvalidSurveyOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for InvalidSurveyOutput {}
pub fn validate(
    work: &Work,
    data: &Value,
    raw: &str,
    completed: CompletedGeneration,
    word_limit: usize,
) -> Result<EnvelopePayload, InvalidSurveyOutput> {
    let value: Value =
        serde_json::from_str(raw).map_err(|_| InvalidSurveyOutput("invalid JSON object".into()))?;
    fn words(v: &Value) -> usize {
        match v {
            Value::Array(a) => a.iter().map(words).sum(),
            Value::Object(o) => o
                .iter()
                .map(|(k, v)| {
                    if matches!(
                        k.as_str(),
                        "claim"
                            | "reason"
                            | "changeEvidence"
                            | "explanation"
                            | "settlingQuestion"
                            | "limitations"
                    ) {
                        v.as_str().map_or(0, |s| s.split_whitespace().count())
                    } else {
                        words(v)
                    }
                })
                .sum(),
            _ => 0,
        }
    }
    if words(&value) > word_limit {
        return Err(InvalidSurveyOutput(format!(
            "prose word budget exceeded: {} > {word_limit}",
            words(&value)
        )));
    }
    let contest_id = work
        .contest_id
        .ok_or(InvalidSurveyOutput("invalid survey work inputs".into()))?;
    let step_id = work
        .step_id
        .ok_or(InvalidSurveyOutput("invalid survey work inputs".into()))?;
    match work.kind {
        WorkKind::SurveyExtract => {
            let extraction: SurveyExtraction =
                validation::decode(&value).map_err(|e| InvalidSurveyOutput(e.to_string()))?;
            let answers: Vec<ContestAnswer> = serde_json::from_value(data["answers"].clone())
                .map_err(|_| InvalidSurveyOutput("invalid frozen answers".into()))?;
            extraction
                .validate(&answers)
                .map_err(|e| InvalidSurveyOutput(e.to_string()))?;
            Ok(EnvelopePayload::SurveyClaims(SurveyClaims {
                contest_id,
                step_id,
                completed,
                extraction,
            }))
        }
        WorkKind::SurveyVote | WorkKind::SurveyRevote => {
            let ballot: SurveyBallot =
                validation::decode(&value).map_err(|e| InvalidSurveyOutput(e.to_string()))?;
            let ids = data["claims"]
                .as_array()
                .ok_or(InvalidSurveyOutput("invalid survey work inputs".into()))?
                .iter()
                .map(|v| {
                    v["claimId"]
                        .as_str()
                        .map(String::from)
                        .ok_or(InvalidSurveyOutput("missing claimId".into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            ballot
                .validate(&ids, work.kind == WorkKind::SurveyRevote)
                .map_err(|e| InvalidSurveyOutput(e.to_string()))?;
            let Target::Member(voter) = &work.target else {
                return Err(InvalidSurveyOutput("invalid survey work inputs".into()));
            };
            let vote = SurveyVote {
                contest_id,
                step_id,
                completed,
                voter: voter.clone(),
                ballot,
            };
            Ok(if work.kind == WorkKind::SurveyVote {
                EnvelopePayload::SurveyVote(vote)
            } else {
                EnvelopePayload::SurveyRevote(vote)
            })
        }
        WorkKind::SurveyReport => {
            let report: SurveyReportText =
                validation::decode(&value).map_err(|e| InvalidSurveyOutput(e.to_string()))?;
            let map: Vec<SurveyClaimMap> = serde_json::from_value(data["map"].clone())
                .map_err(|_| InvalidSurveyOutput("invalid authoritative map".into()))?;
            report
                .validate(&map)
                .map_err(|e| InvalidSurveyOutput(e.to_string()))?;
            let setup: SurveySetup = serde_json::from_value(data["setup"].clone())
                .map_err(|_| InvalidSurveyOutput("invalid frozen setup".into()))?;
            let claims: SurveyExtraction = serde_json::from_value(data["claims"].clone())
                .map_err(|_| InvalidSurveyOutput("invalid frozen claims".into()))?;
            let first: Vec<SurveyVote> = serde_json::from_value(data["first"].clone())
                .map_err(|_| InvalidSurveyOutput("invalid frozen first ballots".into()))?;
            let completeness =
                SurveyCompleteness::from_report_inputs(&setup, &claims, &first, &map);
            Ok(EnvelopePayload::SurveyReport(SurveyReport {
                contest_id,
                step_id,
                completed,
                map,
                report,
                completeness: Some(completeness),
            }))
        }
        _ => Err(InvalidSurveyOutput("invalid survey work inputs".into())),
    }
}
