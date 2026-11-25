//! Per-kind executors share attempt-scoped streaming and ingress fencing.
pub mod census;
pub mod member;
use crate::{
    accounting::{AttemptAccounting, acknowledged},
    openrouter::*,
};
use futures::StreamExt;
use squad_protocol::*;
use tokio_util::sync::CancellationToken;

pub struct StreamOutput {
    pub content: String,
    pub finish_reason: FinishReason,
    pub usage: Option<squad_protocol::Usage>,
}
#[derive(Debug)]
pub enum ExecutionError {
    Reader(ReaderError),
    Transport(TransportError),
    RetryAfter(TransportError, std::time::Duration),
}
impl From<ReaderError> for ExecutionError {
    fn from(e: ReaderError) -> Self {
        Self::Reader(e)
    }
}
pub fn identity(grant: &ExecutionGrant) -> GenerationIdentity {
    GenerationIdentity {
        message_id: grant.work.message_id,
        work_id: grant.work.work_id,
        trigger_event_id: grant.work.trigger_event_id,
        attempt: grant.work.attempt,
    }
}
/// One durable attempt, exactly one provider call. Reasoning never crosses the
/// transport boundary. Structured judge output may disable provisional deltas.
pub async fn stream_attempt(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    mut request: ChatRequest,
    cancel: CancellationToken,
    forward_deltas: bool,
) -> Result<StreamOutput, ExecutionError> {
    request.policy = ExecutionPolicy::SingleAttempt(match grant.work.kind {
        WorkKind::CensusHunt | WorkKind::CensusRate => DeadlineClass::DebateTurn,
        WorkKind::CensusMerge | WorkKind::CensusFinal => DeadlineClass::FinalJudge,
        WorkKind::Reply => DeadlineClass::Ordinary,
        WorkKind::DebateTurn => DeadlineClass::DebateTurn,
        WorkKind::Judge => DeadlineClass::FinalJudge,
        WorkKind::JudgeRound => DeadlineClass::RoundJudge,
    });
    request.model = grant.binding.model.to_string();
    request.reasoning_effort = if grant.work.kind == WorkKind::JudgeRound {
        ReasoningEffort::Low
    } else {
        grant.binding.effort
    };
    let mut account = AttemptAccounting::new(
        grant,
        chrono::Utc::now(),
        chrono::Local::now().format("%Y-%m-%d").to_string(),
    );
    account.submit(context.submit.as_ref()).await?;
    let authorized_deadline = grant.deadline.map(|deadline| {
        tokio::time::Instant::now() + (deadline - chrono::Utc::now()).to_std().unwrap_or_default()
    });
    let mut stream = client.stream_until(request, cancel.clone(), authorized_deadline);
    let mut content = String::new();
    let mut finish = None;
    let mut chunk_index = 0;
    let mut retry_after = None;
    let mut error = TransportError::Disconnected;
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::AttemptStarted { attempt } if attempt != 1 => {
                error = TransportError::InvalidConfiguration;
                break;
            }
            StreamEvent::Metadata {
                generation_id,
                response_model,
                ..
            } => {
                account.metadata(generation_id, response_model);
                account.submit(context.submit.as_ref()).await?;
            }
            StreamEvent::Usage { usage, .. } => account.usage(&usage),
            StreamEvent::RetryAfter { delay } => retry_after = Some(delay),
            StreamEvent::Delta { text, .. } => {
                if forward_deltas {
                    if chunk_index == 0
                        && let ExecutorKey::Member(nick) = &context.executor
                    {
                        acknowledged(
                            context
                                .submit
                                .submit(ExecutorRequest::Presence(Presence {
                                    nick: nick.clone(),
                                    status: PresenceStatus::Streaming,
                                    work_id: Some(grant.work.work_id),
                                    queue_depth: context
                                        .work
                                        .unfinished()
                                        .await?
                                        .len()
                                        .saturating_sub(1)
                                        .min(constants::AGENT_QUEUE_DEPTH)
                                        as u32,
                                    error_code: None,
                                    observed_at: chrono::Utc::now(),
                                }))
                                .await,
                        )?;
                    }
                    if let Err(error) = acknowledged(
                        context
                            .submit
                            .submit(ExecutorRequest::Delta {
                                fence: grant.fence,
                                epoch: grant.epoch,
                                delta: Delta {
                                    generation: identity(grant),
                                    chunk_index,
                                    text: text.clone(),
                                    debate_id: grant.work.debate_id,
                                    round: grant.work.round,
                                    case_id: grant.work.case_id,
                                    contest_id: grant.work.contest_id,
                                    step_id: grant.work.step_id,
                                },
                            })
                            .await,
                    ) {
                        cancel.cancel();
                        account.finish(UsageOutcome::Cancelled, chrono::Utc::now());
                        account.submit(context.submit.as_ref()).await?;
                        return Err(error.into());
                    }
                    chunk_index += 1;
                }
                content.push_str(&text);
            }
            StreamEvent::Finish { reason, .. } => {
                finish = match reason.as_str() {
                    "stop" => Some(FinishReason::Stop),
                    "length" => Some(FinishReason::Length),
                    "content_filter" => Some(FinishReason::ContentFilter),
                    _ => None,
                }
            }
            StreamEvent::Complete { .. } if finish.is_some() => {
                account.finish(UsageOutcome::Completed, chrono::Utc::now());
                account.submit(context.submit.as_ref()).await?;
                if !cancel.is_cancelled() {
                    // Failure to reconcile leaves a durable unknown-cost record.
                    tokio::select! { biased; _ = cancel.cancelled() => {}, _ = account.reconcile(client) => {} }
                    account.submit(context.submit.as_ref()).await?;
                }
                return Ok(StreamOutput {
                    content,
                    finish_reason: finish.expect("checked finish"),
                    usage: account.record.usage,
                });
            }
            StreamEvent::AttemptFailed { error: failure, .. } => {
                error = failure;
                break;
            }
            _ => {}
        }
    }
    account.finish(
        if matches!(error, TransportError::Cancelled) {
            UsageOutcome::Cancelled
        } else {
            UsageOutcome::Failed
        },
        chrono::Utc::now(),
    );
    account.submit(context.submit.as_ref()).await?;
    if !cancel.is_cancelled() {
        tokio::select! { biased; _ = cancel.cancelled() => {}, _ = account.reconcile(client) => {} }
        account.submit(context.submit.as_ref()).await?;
    }
    Err(if let Some(delay) = retry_after {
        ExecutionError::RetryAfter(error, delay)
    } else {
        ExecutionError::Transport(error)
    })
}
pub async fn failed(
    context: &AgentContext,
    grant: &ExecutionGrant,
    code: ErrorCode,
) -> Result<(), ReaderError> {
    acknowledged(
        context
            .submit
            .submit(ExecutorRequest::Failed {
                work_id: grant.work.work_id,
                attempt: grant.work.attempt,
                fence: grant.fence,
                epoch: grant.epoch,
                repair: None,
                error: ErrorPayload {
                    code,
                    text: Text::new(format!("{code:?}")).expect("error code"),
                    retryable: grant.work.kind == WorkKind::Reply,
                    related_event_id: Some(grant.work.trigger_event_id),
                    work_id: Some(grant.work.work_id),
                    debate_id: grant.work.debate_id,
                    case_id: grant.work.case_id,
                    earliest_cursor: None,
                    retry_action: (grant.work.kind == WorkKind::Reply).then_some(RetryAction {
                        kind: RetryKind::Retry,
                        work_id: grant.work.work_id,
                    }),
                    contest_id: grant.work.contest_id,
                },
            })
            .await,
    )
}
