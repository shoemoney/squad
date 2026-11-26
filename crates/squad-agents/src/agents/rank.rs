//! One isolated rank call per grant. The router owns retries and barriers.
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
pub struct RankExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub context_length: u64,
    pub validate: fn(&WorkKind, &Value, &Value) -> validation::ValidationResult,
}
impl WorkExecutor for RankExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            let kind = &grant.work.kind;
            if squad_protocol::rank::composition(kind).is_none() || !grant.owner.owns(&grant.work) {
                return Err(ReaderError::Forbidden);
            }
            let history = context.history.for_work(grant.work.work_id).await?;
            let bundle: Value =
                serde_json::from_str(&history.last().ok_or(ReaderError::NotRunnable)?.content)
                    .map_err(|_| ReaderError::NotRunnable)?;
            let data = bundle["data"].clone();
            let schema =
                squad_protocol::rank::output_schema(kind).ok_or(ReaderError::NotRunnable)?;
            let mut rendered: Composition = serde_json::from_value(bundle["rendered"].clone())
                .map_err(|_| ReaderError::NotRunnable)?;
            let mut repaired = false;
            if let Some(repair) = context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?
            {
                repaired = true;
                let template = bundle["repairTemplate"]
                    .as_str()
                    .ok_or(ReaderError::NotRunnable)?;
                let values = std::collections::BTreeMap::from([
                    (
                        "mode_step",
                        json!(squad_protocol::rank::composition(kind).expect("checked"))
                            .to_string(),
                    ),
                    (
                        "invalid_output_json",
                        json!(repair.rejected_output).to_string(),
                    ),
                    (
                        "validation_errors_json",
                        json!(repair.validation_errors).to_string(),
                    ),
                ]);
                let mut rest = template;
                rendered.user.push('\n');
                while let Some(start) = rest.find("{{") {
                    rendered.user.push_str(&rest[..start]);
                    let (key, tail) = rest[start + 2..]
                        .split_once("}}")
                        .ok_or(ReaderError::NotRunnable)?;
                    rendered
                        .user
                        .push_str(values.get(key).ok_or(ReaderError::NotRunnable)?);
                    rest = tail;
                }
                rendered.user.push_str(rest);
            }
            let mut request = self.request.clone();
            request.visible_ceiling = if kind != &WorkKind::RankCompare {
                squad_protocol::constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            } else {
                4096
            };
            request.response_format = Some(
                json!({"type":"json_schema","json_schema":{"name":"rank_response","strict":true,"schema":schema}}),
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
            let word_limit = bundle["wordLimit"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)? as usize;
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
                    tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, bytes = output.content.len(), error = %e, "rank response is not a JSON object");
                    None
                }
                Ok(v) => match (self.validate)(kind, &data, &v) {
                    Err(e) => {
                        reason = e.message.clone();
                        tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, reason = %e.message, "rank response failed validation");
                        None
                    }
                    Ok(()) if squad_protocol::rank::prose_words(&v) > word_limit => {
                        reason = format!(
                            "prose word budget exceeded: {} > {word_limit}",
                            squad_protocol::rank::prose_words(&v)
                        );
                        tracing::warn!(work_id = %grant.work.work_id, kind = ?kind, words = squad_protocol::rank::prose_words(&v), word_limit, "rank response exceeds the prose budget");
                        None
                    }
                    Ok(()) => Some(v),
                },
            };
            if let Some(data) = result {
                let response = context
                    .submit
                    .submit(ExecutorRequest::Result {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        output: EnvelopePayload::ContestArtifact(ContestArtifact {
                            kind: Some(ContestKind::Rank),
                            round: Some(
                                bundle["round"].as_u64().ok_or(ReaderError::NotRunnable)? as u32
                            ),
                            actor: Some(match &grant.work.target {
                                Target::Judge => Actor::Judge,
                                Target::Member(n) => Actor::Member(n.clone()),
                            }),
                            audience: Some(ArtifactAudience::Human),
                            body: Some(data.clone()),
                            contest_id: grant.work.contest_id.ok_or(ReaderError::NotRunnable)?,
                            step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
                            artifact_kind: Text::new(
                                squad_protocol::rank::composition(kind).expect("checked"),
                            )
                            .map_err(|_| ReaderError::NotRunnable)?,
                            work_id: Some(grant.work.work_id),
                            source_refs: vec![],
                            data,
                        }),
                    })
                    .await;
                if let RequestResult::Rejected { error, .. } = &response {
                    tracing::error!(?error, "rank result rejected");
                }
                return acknowledged(response);
            }
            let code = if kind != &WorkKind::RankCompare {
                ErrorCode::JUDGE_INVALID_MODE_OUTPUT
            } else {
                ErrorCode::MODE_INVALID_OUTPUT
            };
            let repair = (grant.work.attempt < if kind == &WorkKind::RankCompare { 2 } else { 3 }
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
                            text: Text::new("Invalid rank response").expect("error"),
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
    squad_protocol::rank::ENABLED
}
