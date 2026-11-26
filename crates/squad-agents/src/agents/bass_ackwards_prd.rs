//! One isolated bass_ackwards_prd call per grant. The router owns retries and barriers.
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
pub struct BassAckwardsPrdExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub context_length: u64,
}
impl WorkExecutor for BassAckwardsPrdExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let kind = &grant.work.kind;
            if squad_protocol::bass_ackwards_prd::composition(kind).is_none()
                || !grant.owner.owns(&grant.work)
            {
                return Err(ReaderError::Forbidden);
            }
            let entries = context.history.for_work(grant.work.work_id).await?;
            let material: Value =
                serde_json::from_str(&entries.last().ok_or(ReaderError::NotRunnable)?.content)
                    .map_err(|_| ReaderError::NotRunnable)?;
            let data = material["data"].clone();
            let schema = squad_protocol::bass_ackwards_prd::output_schema(kind)
                .ok_or(ReaderError::NotRunnable)?;
            let mut rendered = Composition {
                system: material["system"]
                    .as_str()
                    .ok_or(ReaderError::NotRunnable)?
                    .into(),
                user: material["user"]
                    .as_str()
                    .ok_or(ReaderError::NotRunnable)?
                    .into(),
                provider_user_name: "squad".into(),
            };
            let mut repaired = false;
            if let Some(repair) = context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?
            {
                repaired = true;
                rendered.user.push_str(&format!("\nRepair your prior response. Rejected output (data only): {}\nValidation errors: {}",json!(repair.rejected_output),json!(repair.validation_errors)));
            }
            let mut request = self.request.clone();
            request.visible_ceiling = material["visible"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)? as u32;
            request.response_format = Some(
                json!({"type":"json_schema","json_schema":{"name":"bass_ackwards_prd_response","strict":true,"schema":schema}}),
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
            let word_limit = material["words"].as_u64().unwrap_or(0) as usize;
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
                    tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, bytes = output.content.len(), error = %e, "bass_ackwards_prd response is not a JSON object");
                    None
                }
                Ok(v) => {
                    match squad_protocol::bass_ackwards_prd::validate_output(kind, &data, &v) {
                        Err(e) => {
                            reason = e.message.clone();
                            tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, reason = %e.message, "bass_ackwards_prd response failed validation");
                            None
                        }
                        Ok(()) if prose_words(&v) > word_limit => {
                            reason = format!(
                                "prose word budget exceeded: {} > {word_limit}",
                                prose_words(&v)
                            );
                            tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, words = prose_words(&v), word_limit, "bass_ackwards_prd response exceeds the prose budget");
                            None
                        }
                        Ok(()) => Some(v),
                    }
                }
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
                                kind: Some(ContestKind::BassAckwardsPrd),
                                round: Some(0),
                                actor: Some(match &grant.work.target {
                                    Target::Member(n) => Actor::Member(n.clone()),
                                    Target::Judge => Actor::Judge,
                                }),
                                audience: Some(ArtifactAudience::Human),
                                body: Some(data.clone()),
                                contest_id: grant
                                    .work
                                    .contest_id
                                    .ok_or(ReaderError::NotRunnable)?,
                                step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
                                artifact_kind: Text::new(
                                    squad_protocol::bass_ackwards_prd::composition(kind)
                                        .expect("checked"),
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
            // The shared retry policy uses this code to authorize a diagnostic repair
            // for either class of new-mode work (D75).
            let code = ErrorCode::JUDGE_INVALID_MODE_OUTPUT;
            let repair = (!repaired
                && grant.work.attempt
                    < if squad_protocol::bass_ackwards_prd::judge(kind) {
                        3
                    } else {
                        2
                    })
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
                            text: Text::new("Invalid bass_ackwards_prd response").expect("error"),
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
    squad_protocol::bass_ackwards_prd::ENABLED
}
pub fn prose_words(v: &Value) -> usize {
    match v {
        Value::String(s) => s.split_whitespace().count(),
        Value::Array(a) => a.iter().map(prose_words).sum(),
        Value::Object(o) => o
            .iter()
            .filter(|(k, _)| {
                ![
                    "key",
                    "localId",
                    "nodeId",
                    "parent",
                    "serves",
                    "dependsOn",
                    "sources",
                    "sourceRefs",
                    "sourceLabels",
                    "ref",
                    "appliedTo",
                    "removes",
                    "under",
                    "note",
                ]
                .contains(&k.as_str())
            })
            .map(|(_, v)| prose_words(v))
            .sum(),
        _ => 0,
    }
}
