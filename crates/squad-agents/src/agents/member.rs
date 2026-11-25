//! Reply and debate-turn adapter; scheduling and transitions remain router-owned.
use super::*;
use crate::{
    context::{ContextError, MemberPrompt, member_messages},
    worker::{ExecutionFuture, WorkExecutor, start_attempt},
};

pub struct MemberExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub prompt: MemberPrompt,
    pub context_length: u64,
}
impl WorkExecutor for MemberExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        mut grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            if !matches!(grant.work.kind, WorkKind::Reply | WorkKind::DebateTurn) {
                return Err(ReaderError::Forbidden);
            }
            if self.request.model != grant.binding.model.as_str() {
                return Err(ReaderError::Forbidden);
            }
            let ordinary_deadline = tokio::time::Instant::now()
                + std::time::Duration::from_millis(constants::TOTAL_TIMEOUT_MS);
            loop {
                if cancel.is_cancelled() {
                    return Err(ReaderError::StaleWork);
                }
                let mut request = self.request.clone();
                request.model = grant.binding.model.to_string();
                request.reasoning_effort = grant.binding.effort;
                request.visible_ceiling =
                    grant
                        .work
                        .max_output_tokens
                        .unwrap_or(if grant.work.kind == WorkKind::Reply {
                            constants::REPLY_VISIBLE_TOKENS
                        } else {
                            constants::DEBATE_TURN_VISIBLE_TOKENS
                        });
                request.policy =
                    ExecutionPolicy::SingleAttempt(if grant.work.kind == WorkKind::Reply {
                        DeadlineClass::Ordinary
                    } else {
                        DeadlineClass::DebateTurn
                    });
                let body = request.body().map_err(|_| ReaderError::NotRunnable)?;
                let reservation = body["max_tokens"]
                    .as_u64()
                    .ok_or(ReaderError::NotRunnable)?;
                request.messages = match member_messages(
                    context,
                    &grant,
                    &self.prompt,
                    self.context_length,
                    reservation,
                )
                .await
                {
                    Ok(messages) => messages,
                    Err(error) => {
                        failed(
                            context,
                            &grant,
                            if matches!(error, ContextError::TooLarge { .. }) {
                                ErrorCode::CONTEXT_TOO_LARGE
                            } else {
                                ErrorCode::INTERNAL_ERROR
                            },
                        )
                        .await?;
                        return Err(ReaderError::NotRunnable);
                    }
                };
                let outcome = {
                    let attempt_cancel = cancel.child_token();
                    let call = stream_attempt(
                        context,
                        &grant,
                        &self.client,
                        request,
                        attempt_cancel.clone(),
                        true,
                    );
                    tokio::pin!(call);
                    if grant.work.kind == WorkKind::Reply {
                        tokio::select! {
                            biased;
                            _ = tokio::time::sleep_until(ordinary_deadline) => {
                                attempt_cancel.cancel();
                                let _ = (&mut call).await;
                                failed(context, &grant, ErrorCode::PROVIDER_TIMEOUT).await?;
                                return Err(ReaderError::NotRunnable);
                            },
                            outcome = &mut call => outcome,
                        }
                    } else {
                        call.await
                    }
                };
                let (outcome, retry_after) = match outcome {
                    Err(ExecutionError::RetryAfter(error, delay)) => {
                        (Err(ExecutionError::Transport(error)), delay)
                    }
                    other => (other, std::time::Duration::ZERO),
                };
                match outcome {
                    Ok(output) => {
                        let payload = match &grant.work.target {
                            Target::Member(speaker) if grant.work.kind == WorkKind::DebateTurn => {
                                EnvelopePayload::DebateTurn(DebateTurn {
                                    generation: identity(&grant),
                                    debate_id: grant
                                        .work
                                        .debate_id
                                        .ok_or(ReaderError::NotRunnable)?,
                                    round: grant.work.round.ok_or(ReaderError::NotRunnable)?,
                                    speaker: speaker.clone(),
                                    outcome: DebateTurnOutcome::Completed {
                                        content: output.content,
                                        model: grant.binding.model.clone(),
                                        finish_reason: output.finish_reason,
                                        usage: output.usage,
                                    },
                                })
                            }
                            Target::Member(_) => EnvelopePayload::AgentReply(Completion {
                                completed: CompletedGeneration {
                                    generation: identity(&grant),
                                    model: grant.binding.model.clone(),
                                    finish_reason: output.finish_reason,
                                    usage: output.usage,
                                },
                                in_reply_to_message_id: grant
                                    .work
                                    .in_reply_to_message_id
                                    .ok_or(ReaderError::NotRunnable)?,
                                content: output.content,
                            }),
                            _ => return Err(ReaderError::Forbidden),
                        };
                        return acknowledged(
                            context
                                .submit
                                .submit(ExecutorRequest::Result {
                                    work_id: grant.work.work_id,
                                    attempt: grant.work.attempt,
                                    fence: grant.fence,
                                    epoch: grant.epoch,
                                    output: payload,
                                })
                                .await,
                        );
                    }
                    Err(ExecutionError::Reader(error)) => return Err(error),
                    Err(ExecutionError::RetryAfter(_, _)) => unreachable!("normalized retry delay"),
                    Err(ExecutionError::Transport(error)) => {
                        if grant.work.kind == WorkKind::Reply
                            && grant.work.attempt < u32::from(constants::REPLY_ATTEMPTS)
                            && error.retryable()
                        {
                            let delay =
                                std::time::Duration::from_millis(rand::random_range(0..=1000))
                                    .max(retry_after);
                            if tokio::time::Instant::now() + delay >= ordinary_deadline {
                                failed(context, &grant, ErrorCode::PROVIDER_TIMEOUT).await?;
                                return Err(ReaderError::NotRunnable);
                            }
                            tokio::select! { biased; _ = cancel.cancelled() => return Err(ReaderError::StaleWork), _ = tokio::time::sleep(delay) => {} }
                            grant = start_attempt(context, &grant, grant.work.attempt + 1).await?;
                        } else {
                            let code = match error {
                                TransportError::Cancelled => ErrorCode::WORK_CANCELLED,
                                TransportError::Deadline
                                | TransportError::IdleTimeout
                                | TransportError::FirstTokenTimeout => ErrorCode::PROVIDER_TIMEOUT,
                                TransportError::Network => ErrorCode::PROVIDER_NETWORK_ERROR,
                                _ => ErrorCode::PROVIDER_ERROR,
                            };
                            failed(context, &grant, code).await?;
                            return Err(ReaderError::NotRunnable);
                        }
                    }
                }
            }
        })
    }
}
