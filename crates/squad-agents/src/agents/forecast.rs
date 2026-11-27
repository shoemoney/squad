//! One isolated forecast call per grant. The router owns retries and barriers.
use super::*;
use crate::worker::{ExecutionFuture, WorkExecutor};
use serde::Deserialize;
use serde_json::{Value, json};
use squad_protocol::{Actor, ArtifactAudience, ContestKind, Target};
#[derive(Deserialize)]
struct Composition {
    system: String,
    user: String,
    provider_user_name: String,
}
pub struct ForecastExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub context_length: u64,
}
impl WorkExecutor for ForecastExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let kind = &grant.work.kind;
            if kind.new_mode_composition().is_none() || !grant.owner.owns(&grant.work) {
                return Err(ReaderError::Forbidden);
            }
            let history = context.history.for_work(grant.work.work_id).await?;
            let frozen: Value =
                serde_json::from_str(&history.last().ok_or(ReaderError::NotRunnable)?.content)
                    .map_err(|_| ReaderError::NotRunnable)?;
            let data = frozen["data"].clone();
            let schema =
                squad_protocol::forecast::output_schema(kind).ok_or(ReaderError::NotRunnable)?;
            let mut rendered: Composition = serde_json::from_value(frozen["rendered"].clone())
                .map_err(|_| ReaderError::NotRunnable)?;
            let mut repaired = false;
            if let Some(repair) = context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?
            {
                repaired = true;
                rendered.user.push('\n');
                rendered.user.push_str(&repair_prompt(
                    frozen["repairTemplate"]
                        .as_str()
                        .ok_or(ReaderError::NotRunnable)?,
                    &repair,
                    kind.new_mode_composition()
                        .ok_or(ReaderError::NotRunnable)?,
                )?);
            }
            let mut request = self.request.clone();
            request.visible_ceiling = if kind.new_mode_judge() {
                squad_protocol::constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            } else {
                2048
            };
            request.response_format = Some(
                json!({"type":"json_schema","json_schema":{"name":"forecast_response","strict":true,"schema":schema}}),
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
            let word_limit = data["wordLimit"].as_u64().unwrap_or(500) as usize;
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
                    tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, bytes = output.content.len(), error = %e, "forecast response is not a JSON object");
                    None
                }
                Ok(v) => match squad_protocol::forecast::validate_output(kind, &data, &v) {
                    Err(e) => {
                        reason = e.message.clone();
                        tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, reason = %e.message, "forecast response failed validation");
                        None
                    }
                    Ok(()) if squad_protocol::forecast::prose_words(&v) > word_limit => {
                        reason = format!(
                            "prose word budget exceeded: {} > {word_limit}",
                            squad_protocol::forecast::prose_words(&v)
                        );
                        tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, words = squad_protocol::forecast::prose_words(&v), word_limit, "forecast response exceeds the prose budget");
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
                                kind: Some(ContestKind::Forecast),
                                round: Some(0),
                                actor: Some(match &grant.work.target {
                                    Target::Judge => Actor::Judge,
                                    Target::Member(n) => Actor::Member(n.clone()),
                                }),
                                audience: Some(ArtifactAudience::Human),
                                body: Some(data.clone()),
                                contest_id: grant
                                    .work
                                    .contest_id
                                    .ok_or(ReaderError::NotRunnable)?,
                                step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
                                artifact_kind: Text::new(
                                    kind.new_mode_composition().expect("checked"),
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
            let code = if kind.new_mode_judge() {
                ErrorCode::JUDGE_INVALID_MODE_OUTPUT
            } else {
                ErrorCode::MODE_INVALID_OUTPUT
            };
            let repair = (grant.work.attempt < if kind.new_mode_judge() { 3 } else { 2 }
                && !repaired)
                .then(|| JudgeRepairData {
                    attempt: grant.work.attempt + 1,
                    rejected_output: output.content,
                    validation_errors: vec![reason.clone()],
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
                            text: Text::new("Invalid forecast response").expect("error"),
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
pub fn enabled() -> bool {
    squad_protocol::forecast::ENABLED
}

/// Render frozen repair source exactly once; rejected data cannot inject slots.
pub fn repair_prompt(
    source: &str,
    repair: &JudgeRepairData,
    step: &str,
) -> Result<String, ReaderError> {
    let mut out = String::new();
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let (key, tail) = rest[start + 2..]
            .split_once("}}")
            .ok_or(ReaderError::NotRunnable)?;
        match key {
            "mode_step" => out.push_str(step),
            "invalid_output_json" => out.push_str(&json!(repair.rejected_output).to_string()),
            "validation_errors_json" => out.push_str(&json!(repair.validation_errors).to_string()),
            _ => return Err(ReaderError::NotRunnable),
        }
        rest = tail;
    }
    out.push_str(rest);
    Ok(out)
}
