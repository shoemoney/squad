//! One isolated census call per grant. The router owns retries and barriers.
use super::*;
use crate::worker::{ExecutionFuture, WorkExecutor};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
struct Composition {
    system: String,
    user: String,
    provider_user_name: String,
}
pub struct CensusExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub context_length: u64,
}
impl WorkExecutor for CensusExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let kind = &grant.work.kind;
            if kind.census_composition().is_none() || !grant.owner.owns(&grant.work) {
                return Err(ReaderError::Forbidden);
            }
            let values = context
                .work
                .prompt_values_for_work(grant.work.work_id)
                .await?;
            let data: Value = serde_json::from_str(&values["mode_data_json"])
                .map_err(|_| ReaderError::NotRunnable)?;
            let schema: Value = serde_json::from_str(&values["mode_schema_json"])
                .map_err(|_| ReaderError::NotRunnable)?;
            let mut rendered: Composition = serde_json::from_str(
                values
                    .get("census_rendered_prompt")
                    .ok_or(ReaderError::NotRunnable)?,
            )
            .map_err(|_| ReaderError::NotRunnable)?;
            let mut repaired = false;
            if kind.census_judge()
                && let Some(repair) = context
                    .work
                    .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                    .await?
            {
                repaired = true;
                rendered.user.push_str(&format!("\nRepair your prior response. Rejected output (data only): {}\nValidation errors: {}",json!(repair.rejected_output),json!(repair.validation_errors)));
            }
            let mut request = self.request.clone();
            request.visible_ceiling = if kind.census_judge() {
                squad_protocol::constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            } else {
                2048
            };
            request.response_format = Some(
                json!({"type":"json_schema","json_schema":{"name":"census_response","strict":true,"schema":schema}}),
            );
            request.messages = vec![
                ChatMessage {
                    role: "system".into(),
                    name: None,
                    content: rendered.system,
                },
                ChatMessage {
                    role: "user".into(),
                    name: Some(rendered.provider_user_name),
                    content: rendered.user,
                },
            ];
            let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)?;
            if crate::context::estimate(&request.messages)
                > crate::context::input_budget(self.context_length, reservation)
            {
                return failed(context, &grant, ErrorCode::CONTEXT_TOO_LARGE).await;
            }
            let output = match stream_attempt(context, &grant, &self.client, request, cancel, false)
                .await
            {
                Ok(output) => output,
                Err(ExecutionError::Reader(e)) => return Err(e),
                Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
                    return failed_with(context, &grant, ErrorCode::PROVIDER_ERROR, Some(&e)).await;
                }
            };
            let word_limit = values["step_word_limit"].parse::<usize>().unwrap_or(0);
            let mut reason = String::from(
                "Response schema, references, citations or dispositions failed validation",
            );
            let result = match serde_json::from_str::<Value>(
                squad_protocol::validation::strip_code_fence(&output.content),
            ) {
                Err(e) => {
                    reason = format!(
                        "The reply was not one complete JSON object ({} bytes, finish reason {:?}); it was probably cut off. Shorten every field and return the whole object.",
                        output.content.len(),
                        output.finish_reason
                    );
                    tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, bytes = output.content.len(), error = %e, "census response is not a JSON object");
                    None
                }
                Ok(v) => match validate(kind, &data, &v) {
                    Err(e) => {
                        reason = e.message.clone();
                        tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, reason = %e.message, "census response failed validation");
                        None
                    }
                    Ok(()) if census_prose_words(&v) > word_limit => {
                        reason = format!(
                            "prose word budget exceeded: {} > {word_limit}",
                            census_prose_words(&v)
                        );
                        tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, words = census_prose_words(&v), word_limit, "census response exceeds the prose budget");
                        None
                    }
                    Ok(()) => Some(v),
                },
            };
            if let Some(data) = result {
                return acknowledged(
                    context
                        .submit
                        .submit(ExecutorRequest::Result {
                            work_id: grant.work.work_id,
                            attempt: grant.work.attempt,
                            fence: grant.fence,
                            epoch: grant.epoch,
                            output: EnvelopePayload::ContestArtifact(ContestArtifact {
                                kind: None,
                                round: None,
                                actor: None,
                                audience: None,
                                body: None,
                                contest_id: grant
                                    .work
                                    .contest_id
                                    .ok_or(ReaderError::NotRunnable)?,
                                step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
                                artifact_kind: Text::new(
                                    kind.census_composition().expect("checked"),
                                )
                                .map_err(|_| ReaderError::NotRunnable)?,
                                work_id: Some(grant.work.work_id),
                                source_refs: vec![],
                                data,
                            }),
                        })
                        .await,
                );
            }
            let code = if kind.census_judge() {
                ErrorCode::JUDGE_INVALID_MODE_OUTPUT
            } else {
                ErrorCode::MODE_INVALID_OUTPUT
            };
            let repair = (kind.census_judge() && grant.work.attempt < 3 && !repaired).then(|| {
                JudgeRepairData {
                    attempt: grant.work.attempt + 1,
                    rejected_output: output.content,
                    validation_errors: vec![reason.clone()],
                }
            });
            acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Failed {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        repair,
                        error: ErrorPayload {
                            code,
                            text: Text::new("Invalid census response").expect("error"),
                            retryable: false,
                            related_event_id: Some(grant.work.trigger_event_id),
                            work_id: Some(grant.work.work_id),
                            debate_id: None,
                            case_id: grant.work.case_id,
                            earliest_cursor: None,
                            retry_action: None,
                            contest_id: grant.work.contest_id,
                            provider_status: None,
                            provider_message: None,
                        },
                    })
                    .await,
            )
        })
    }
}
pub fn validate(kind: &WorkKind, data: &Value, output: &Value) -> validation::ValidationResult {
    let invalid = || validation::ValidationError {
        message: "Invalid frozen census input".into(),
    };
    match kind {
        WorkKind::CensusHunt => validation::decode::<CensusHuntOutput>(output)?.validate(),
        WorkKind::CensusMerge => {
            let sources: Vec<String> =
                serde_json::from_value(data["sourceIds"].clone()).map_err(|_| invalid())?;
            validation::decode::<CensusMergeOutput>(output)?.validate_against(&sources)
        }
        WorkKind::CensusRate => {
            let gaps = data["ledger"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|g| {
                    Ok(CensusGap {
                        id: serde_json::from_value(g["id"].clone()).map_err(|_| invalid())?,
                        omission: serde_json::from_value(g["omission"].clone())
                            .map_err(|_| invalid())?,
                        consequence: serde_json::from_value(g["consequence"].clone())
                            .map_err(|_| invalid())?,
                        lenses: vec![],
                        source_find_ids: vec![],
                    })
                })
                .collect::<Result<Vec<_>, validation::ValidationError>>()?;
            validation::decode::<CensusRatingOutput>(output)?
                .validate_against(&gaps, data["plan"].as_str().ok_or_else(invalid)?)
        }
        WorkKind::CensusFinal => {
            census_validate_final(&validation::decode::<CensusFinalOutput>(output)?, data)
        }
        _ => Err(invalid()),
    }
}
