//! Per-kind executors share attempt-scoped streaming and ingress fencing.
pub mod census;
pub mod crosscheck;
pub mod grill;
pub mod member;
pub mod probe;
pub mod survey;
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
        WorkKind::TuneCases => DeadlineClass::DebateTurn,
        WorkKind::TuneFreeze => DeadlineClass::FinalJudge,
        WorkKind::TuneExecute => DeadlineClass::Executor,
        WorkKind::TuneScore => DeadlineClass::FinalJudge,
        WorkKind::TuneMutate => DeadlineClass::DebateTurn,
        WorkKind::BassAckwardsPrdDone => DeadlineClass::DebateTurn,
        WorkKind::BassAckwardsPrdFreeze => DeadlineClass::FinalJudge,
        WorkKind::BassAckwardsPrdTree => DeadlineClass::DebateTurn,
        WorkKind::BassAckwardsPrdMerge => DeadlineClass::FinalJudge,
        WorkKind::BassAckwardsPrdReview => DeadlineClass::DebateTurn,
        WorkKind::BassAckwardsPrdRevise => DeadlineClass::FinalJudge,
        WorkKind::BassAckwardsPrdMustHave => DeadlineClass::DebateTurn,
        WorkKind::BassAckwardsPrdFinalize => DeadlineClass::FinalJudge,
        WorkKind::BassAckwardsPrdApply => DeadlineClass::FinalJudge,

        WorkKind::ForgeDraft => DeadlineClass::DebateTurn,
        WorkKind::ForgeRank => DeadlineClass::DebateTurn,
        WorkKind::ForgeFuse => DeadlineClass::DebateTurn,
        WorkKind::ForgeJudge => DeadlineClass::FinalJudge,
        WorkKind::ForecastFrame => DeadlineClass::FinalJudge,
        WorkKind::ForecastEstimate => DeadlineClass::DebateTurn,
        WorkKind::ForecastRevise => DeadlineClass::DebateTurn,
        WorkKind::ForecastCruxes => DeadlineClass::FinalJudge,
        WorkKind::DecideOptions => DeadlineClass::DebateTurn,
        WorkKind::DecideShortlist => DeadlineClass::FinalJudge,
        WorkKind::DecideCriteria => DeadlineClass::DebateTurn,
        WorkKind::DecideFreeze => DeadlineClass::FinalJudge,
        WorkKind::DecideScore => DeadlineClass::DebateTurn,
        WorkKind::DecideExplain => DeadlineClass::FinalJudge,
        WorkKind::NegotiateMap => DeadlineClass::DebateTurn,
        WorkKind::NegotiateBrief => DeadlineClass::FinalJudge,
        WorkKind::NegotiatePackage => DeadlineClass::DebateTurn,
        WorkKind::NegotiateMove => DeadlineClass::DebateTurn,
        WorkKind::NegotiateCounter => DeadlineClass::DebateTurn,
        WorkKind::NegotiatePlaybook => DeadlineClass::FinalJudge,
        WorkKind::BlowItUpAttack => DeadlineClass::DebateTurn,
        WorkKind::BlowItUpChallenge => DeadlineClass::DebateTurn,
        WorkKind::BlowItUpRule => DeadlineClass::FinalJudge,
        WorkKind::ReadbackInterpret => DeadlineClass::DebateTurn,
        WorkKind::ReadbackDiff => DeadlineClass::FinalJudge,
        WorkKind::ReadbackRewrite => DeadlineClass::FinalJudge,
        WorkKind::DebuggingHypothesize => DeadlineClass::DebateTurn,
        WorkKind::DebuggingMerge => DeadlineClass::FinalJudge,
        WorkKind::DebuggingTest => DeadlineClass::DebateTurn,
        WorkKind::DebuggingSelect => DeadlineClass::FinalJudge,
        WorkKind::DebuggingReport => DeadlineClass::FinalJudge,
        WorkKind::TraceExtract => DeadlineClass::DebateTurn,
        WorkKind::TraceMerge => DeadlineClass::FinalJudge,
        WorkKind::TraceVerify => DeadlineClass::DebateTurn,
        WorkKind::TraceAssemble => DeadlineClass::FinalJudge,
        WorkKind::RankCompare => DeadlineClass::DebateTurn,
        WorkKind::RankExplain => DeadlineClass::FinalJudge,

        WorkKind::CensusHunt | WorkKind::CensusRate => DeadlineClass::DebateTurn,
        WorkKind::CensusMerge | WorkKind::CensusFinal => DeadlineClass::FinalJudge,
        WorkKind::CrosscheckInventory | WorkKind::CrosscheckCell | WorkKind::CrosscheckAudit => {
            DeadlineClass::DebateTurn
        }
        WorkKind::CrosscheckGoal | WorkKind::CrosscheckNormalize | WorkKind::CrosscheckRule => {
            DeadlineClass::FinalJudge
        }
        WorkKind::SurveyExtract | WorkKind::SurveyReport => DeadlineClass::FinalJudge,
        WorkKind::SurveyVote | WorkKind::SurveyRevote => DeadlineClass::Ordinary,
        WorkKind::GrillQuestion | WorkKind::GrillAnswer | WorkKind::GrillRebuild => {
            DeadlineClass::DebateTurn
        }
        WorkKind::Reply => DeadlineClass::Ordinary,
        WorkKind::ProbeExplain | WorkKind::ProbeCheck | WorkKind::ProbePredict => {
            DeadlineClass::DebateTurn
        }
        WorkKind::ProbeReport => DeadlineClass::FinalJudge,
        WorkKind::DebateTurn => DeadlineClass::DebateTurn,
        WorkKind::Judge => DeadlineClass::FinalJudge,
        WorkKind::JudgeTournament => DeadlineClass::TournamentJudge,
        WorkKind::JudgeRound => DeadlineClass::RoundJudge,
    });
    request.model = grant.binding.model.to_string();
    request.reasoning_effort = if matches!(
        grant.work.kind,
        WorkKind::SurveyExtract | WorkKind::SurveyReport
    ) || grant.work.kind.new_mode_judge()
    {
        squad_protocol::constants::NEW_MODE_JUDGE_REASONING_EFFORT
    } else if grant.work.kind == WorkKind::JudgeRound {
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
                if account.metadata(generation_id, response_model) {
                    account.submit(context.submit.as_ref()).await?;
                }
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
/// Settle an attempt whose durable provider start was committed but whose call
/// was never made (stopped or cleared first), so its reservation is released.
pub async fn abandon(context: &AgentContext, grant: &ExecutionGrant) {
    let mut account = AttemptAccounting::new(
        grant,
        chrono::Utc::now(),
        chrono::Local::now().format("%Y-%m-%d").to_string(),
    );
    account.finish(UsageOutcome::Cancelled, chrono::Utc::now());
    let _ = account.submit(context.submit.as_ref()).await;
}
pub async fn failed(
    context: &AgentContext,
    grant: &ExecutionGrant,
    code: ErrorCode,
) -> Result<(), ReaderError> {
    failed_with(context, grant, code, None).await
}

/// Like [`failed`], carrying the transport's sanitized HTTP status and message
/// (never headers or credentials) so the CLI can show why the provider refused.
pub async fn failed_with(
    context: &AgentContext,
    grant: &ExecutionGrant,
    code: ErrorCode,
    transport: Option<&TransportError>,
) -> Result<(), ReaderError> {
    let (provider_status, provider_message) = match transport {
        Some(TransportError::HttpMessage { status, message }) => {
            (Some(*status), Some(message.clone()))
        }
        Some(TransportError::Http(status)) => (Some(*status), None),
        Some(TransportError::Provider(Some(status))) => (Some(*status), None),
        _ => (None, None),
    };
    let text = match (provider_status, &provider_message) {
        (Some(status), Some(message)) => format!("{code:?} (HTTP {status}): {message}"),
        (Some(status), None) => format!("{code:?} (HTTP {status})"),
        _ => format!("{code:?}"),
    };
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
                    text: Text::new(text).expect("error code"),
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
                    provider_status,
                    provider_message,
                },
            })
            .await,
    )
}

pub mod tournament;

pub mod forge;

pub mod forecast;

pub mod decide;

pub mod negotiate;

pub mod blow_it_up;

pub mod readback;

pub mod debugging;

pub mod trace;

pub mod rank;

pub mod tune;

pub mod bass_ackwards_prd;

/// A frozen executor request has no prompt-pack, persona, history or redaction inputs.
pub fn bare_executor_request(
    grant: &ExecutionGrant,
    prompt: &str,
    input: &str,
    published_output_limit: u32,
) -> Result<ChatRequest, TransportError> {
    if !grant.work.kind.is_executor()
        || grant.work.validate().is_err()
        || !grant.owner.owns(&grant.work)
        || prompt.matches("{{input}}").count() > 1
    {
        return Err(TransportError::InvalidConfiguration);
    }
    let message = |role: &str, content: String| ChatMessage {
        role: role.into(),
        name: None,
        content,
    };
    let messages = if prompt.contains("{{input}}") {
        vec![message("user", prompt.replacen("{{input}}", input, 1))]
    } else {
        vec![
            message("system", prompt.into()),
            message("user", input.into()),
        ]
    };
    let request = ChatRequest {
        model: grant.binding.model.to_string(),
        messages,
        visible_ceiling: constants::TUNE_EXEC_VISIBLE_TOKENS,
        published_output_limit,
        supports_reasoning: false,
        reasoning_effort: grant.binding.effort,
        reasoning_explicit: false,
        response_format: None,
        policy: ExecutionPolicy::SingleAttempt(DeadlineClass::Executor),
    };
    request.body()?;
    Ok(request)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorOutput {
    pub text: String,
    pub truncated: bool,
}
pub fn executor_output(output: StreamOutput) -> Result<ExecutorOutput, ExecutionError> {
    if output.content.trim().is_empty() {
        return Err(ExecutionError::Reader(ReaderError::NotRunnable));
    }
    Ok(ExecutorOutput {
        text: output.content,
        truncated: output.finish_reason == FinishReason::Length,
    })
}

/// Execute one already admitted bare attempt; the router owns any plain retry.
#[allow(clippy::too_many_arguments)]
pub async fn execute_bare(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    prompt: &str,
    input: &str,
    published_output_limit: u32,
    cancel: CancellationToken,
) -> Result<ExecutorOutput, ExecutionError> {
    let request = bare_executor_request(grant, prompt, input, published_output_limit)
        .map_err(ExecutionError::Transport)?;
    executor_output(stream_attempt(context, grant, client, request, cancel, false).await?)
}
