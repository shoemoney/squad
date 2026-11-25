//! A single structured grill attempt against frozen, capability-bound material.
use super::*;
use crate::worker::{ExecutionFuture, WorkExecutor};
pub struct GrillExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub context_length: u64,
}
impl WorkExecutor for GrillExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            if grant.work.kind.grill_composition().is_none() || !grant.owner.owns(&grant.work) {
                return Err(ReaderError::Forbidden);
            }
            let values = context
                .work
                .prompt_values_for_work(grant.work.work_id)
                .await?;
            let input: GrillWorkInput =
                serde_json::from_str(values.get("grill_input").ok_or(ReaderError::NotRunnable)?)
                    .map_err(|_| ReaderError::NotRunnable)?;
            let rendered: serde_json::Value = serde_json::from_str(
                values
                    .get("grill_rendered")
                    .ok_or(ReaderError::NotRunnable)?,
            )
            .map_err(|_| ReaderError::NotRunnable)?;
            let mut request = self.request.clone();
            request.messages = vec![ChatMessage {
                role: "system".into(),
                name: None,
                content: rendered["system"]
                    .as_str()
                    .ok_or(ReaderError::NotRunnable)?
                    .into(),
            }];
            for entry in context.history.for_work(grant.work.work_id).await? {
                request.messages.push(ChatMessage {
                    role: "user".into(),
                    name: None,
                    content: entry.content,
                });
            }
            let format = match grant.work.kind {
                WorkKind::GrillQuestion => {
                    "Return exactly PASS, or a JSON object with only questions: an array of one to three question strings."
                }
                WorkKind::GrillAnswer => {
                    "Return a JSON object with only answers: an array of objects with questionIndex (zero based), answer, and planChanges. Cover each question exactly once. Both prose fields must be nonempty."
                }
                _ => "Return only the complete standalone revised plan.",
            };
            request.messages.push(ChatMessage {
                role: "user".into(),
                name: Some("squad".into()),
                content: format!(
                    "{}\n{format}",
                    rendered["user"].as_str().ok_or(ReaderError::NotRunnable)?
                ),
            });
            let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)?;
            if crate::context::estimate(&request.messages)
                > crate::context::input_budget(self.context_length, reservation)
            {
                return failed(context, &grant, ErrorCode::CONTEXT_TOO_LARGE).await;
            }
            let output =
                match stream_attempt(context, &grant, &self.client, request, cancel, false).await {
                    Ok(v) => v,
                    Err(ExecutionError::Reader(e)) => return Err(e),
                    Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
                        return failed(
                            context,
                            &grant,
                            match e {
                                TransportError::FirstTokenTimeout
                                | TransportError::IdleTimeout
                                | TransportError::Deadline => ErrorCode::PROVIDER_TIMEOUT,
                                TransportError::Cancelled => ErrorCode::WORK_CANCELLED,
                                _ => ErrorCode::PROVIDER_ERROR,
                            },
                        )
                        .await;
                    }
                };
            if output.finish_reason != FinishReason::Stop
                || input
                    .parse_output(&grant.work.kind, &output.content)
                    .is_err()
            {
                return failed(context, &grant, ErrorCode::MODE_INVALID_OUTPUT).await;
            }
            acknowledged(context.submit.submit(ExecutorRequest::Result{work_id:grant.work.work_id,attempt:grant.work.attempt,fence:grant.fence,epoch:grant.epoch,output:EnvelopePayload::ContestArtifact(ContestArtifact{contest_id:input.state.contest_id,step_id:grant.work.step_id.ok_or(ReaderError::NotRunnable)?,artifact_kind:Text::new(grant.work.kind.grill_composition().expect("checked")).expect("literal"),work_id:Some(grant.work.work_id),source_refs:vec![],data:serde_json::json!({"generation":identity(&grant),"raw":output.content,"finishReason":output.finish_reason})})}).await)
        })
    }
}
