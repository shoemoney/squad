//! One accounted provider attempt; invalid objects use the D75 repair channel.
use super::*;
use crate::accounting::acknowledged;
use squad_protocol::tune::*;
pub fn enabled() -> bool {
    squad_protocol::tune::ENABLED
}
#[allow(clippy::too_many_arguments)]
pub async fn execute<F>(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    request: ChatRequest,
    input: &TuneCaseMaterial,
    candidate: Option<&str>,
    case_label: Option<&str>,
    validate: F,
    cancel: CancellationToken,
) -> Result<(), ExecutionError>
where
    F: Fn(&TuneCaseMaterial, &serde_json::Value) -> validation::ValidationResult,
{
    let output = match stream_attempt(context, grant, client, request, cancel, false).await {
        Ok(o) => o,
        Err(ExecutionError::Reader(e)) => return Err(e.into()),
        Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
            return failed_with(
                context,
                grant,
                if matches!(e, TransportError::Cancelled) {
                    ErrorCode::WORK_CANCELLED
                } else {
                    ErrorCode::PROVIDER_ERROR
                },
                Some(&e),
            )
            .await
            .map_err(Into::into);
        }
    };
    let body = if matches!(input, TuneCaseMaterial::Execute { .. }) {
        if output.content.trim().is_empty() {
            return failed(context, grant, ErrorCode::MODE_INVALID_OUTPUT)
                .await
                .map_err(Into::into);
        }
        serde_json::to_value(TuneExecution {
            candidate_id: candidate.ok_or(ReaderError::NotRunnable)?.into(),
            case_label: case_label.ok_or(ReaderError::NotRunnable)?.into(),
            output: Some(output.content),
            truncated: output.finish_reason == FinishReason::Length,
        })
        .map_err(|_| ReaderError::NotRunnable)?
    } else {
        let parsed = serde_json::from_str::<serde_json::Value>(validation::strip_code_fence(
            &output.content,
        ));
        let valid = parsed
            .as_ref()
            .map_err(|_| "Return one complete JSON object; shorten fields if cut off.".to_owned())
            .and_then(|v| validate(input, v).map_err(|e| e.message));
        if let Err(reason) = valid {
            let repaired = context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?
                .is_some();
            let limit = if grant.work.kind.new_mode_judge() {
                3
            } else {
                2
            };
            let mut error = ErrorPayload {
                code: ErrorCode::JUDGE_INVALID_MODE_OUTPUT,
                text: Text::new("Tune output failed validation").expect("literal"),
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
            };
            // The error class remains invalid on exhaustion, so failure_policy
            // cannot turn a second invalid repair into another plain attempt.
            error.retryable = false;
            return acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Failed {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        error,
                        repair: (!repaired && grant.work.attempt < limit).then_some(
                            JudgeRepairData {
                                attempt: grant.work.attempt + 1,
                                rejected_output: output.content,
                                validation_errors: vec![reason],
                            },
                        ),
                    })
                    .await,
            )
            .map_err(Into::into);
        }
        parsed.map_err(|_| ReaderError::NotRunnable)?
    };
    let actor = match &grant.work.target {
        Target::Member(n) => Actor::Member(n.clone()),
        Target::Judge => Actor::Judge,
    };
    let artifact = ContestArtifact {
        kind: Some(ContestKind::Tune),
        round: Some(grant.work.round.unwrap_or(0)),
        actor: Some(actor),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        contest_id: grant.work.contest_id.ok_or(ReaderError::NotRunnable)?,
        step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
        artifact_kind: Text::new(work_name(&grant.work.kind)).expect("kind"),
        work_id: Some(grant.work.work_id),
        source_refs: vec![],
        data: body,
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
    .map_err(Into::into)
}
