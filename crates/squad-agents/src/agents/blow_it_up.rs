//! One durable provider attempt. Repairs remain private and router-scheduled.
use super::*;
use serde_json::Value;
pub fn enabled() -> bool {
    squad_protocol::blow_it_up::ENABLED
}
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    request: ChatRequest,
    data: &Value,
    validate: fn(&WorkKind, &Value, &Value) -> validation::ValidationResult,
    already_repaired: bool,
    cancel: CancellationToken,
) -> Result<(), ReaderError> {
    let output = match stream_attempt(context, grant, client, request, cancel, false).await {
        Ok(o) => o,
        Err(ExecutionError::Reader(e)) => return Err(e),
        Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
            return failed_with(context, grant, ErrorCode::PROVIDER_ERROR, Some(&e)).await;
        }
    };
    let result = serde_json::from_str::<Value>(validation::strip_code_fence(&output.content))
        .map_err(|_| validation::ValidationError {
            message: "Return one complete JSON object; shorten prose if output was truncated"
                .into(),
        })
        .and_then(|v| {
            validate_shape(&grant.work.kind, &v)
                .and_then(|()| validate(&grant.work.kind, data, &v))
                .map(|()| v)
        });
    match result {
        Ok(body) => {
            let actor = match &grant.work.target {
                Target::Member(n) => Actor::Member(n.clone()),
                Target::Judge => Actor::Judge,
            };
            let artifact = ContestArtifact {
                kind: Some(ContestKind::BlowItUp),
                round: Some(0),
                actor: Some(actor),
                audience: Some(ArtifactAudience::Human),
                body: Some(body),
                contest_id: grant.work.contest_id.ok_or(ReaderError::NotRunnable)?,
                step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
                artifact_kind: Text::new(
                    squad_protocol::blow_it_up::composition(&grant.work.kind)
                        .ok_or(ReaderError::NotRunnable)?,
                )
                .unwrap(),
                work_id: Some(grant.work.work_id),
                source_refs: vec![],
                data: serde_json::json!({}),
            };
            acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Result {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        output: EnvelopePayload::ContestArtifact(artifact),
                    })
                    .await,
            )
        }
        Err(reason) => {
            let judge = grant.work.target == Target::Judge;
            let repair = (!already_repaired && grant.work.attempt < if judge { 3 } else { 2 })
                .then(|| JudgeRepairData {
                    attempt: grant.work.attempt + 1,
                    rejected_output: output.content,
                    validation_errors: vec![reason.message],
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
                            code: if grant.work.kind.new_mode_judge() {
                                ErrorCode::JUDGE_INVALID_MODE_OUTPUT
                            } else {
                                ErrorCode::MODE_INVALID_OUTPUT
                            },
                            text: Text::new("Invalid Blow It Up output").unwrap(),
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
        }
    }
}

/// The adapter rejects non-mode schemas before contextual validation by core.
pub fn validate_shape(kind: &WorkKind, output: &Value) -> validation::ValidationResult {
    let name = squad_protocol::blow_it_up::composition(kind).ok_or_else(|| {
        validation::ValidationError {
            message: "Not Blow It Up work".into(),
        }
    })?;
    squad_protocol::blow_it_up::validate_artifact(name, output)
}
