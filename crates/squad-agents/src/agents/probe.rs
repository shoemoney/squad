//! Exactly one provider call per issued probe attempt, without provisional leaks.
use super::*;
use crate::accounting::acknowledged;

pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    request: ChatRequest,
    input: &ProbeInput,
    cancel: CancellationToken,
) -> Result<(), ReaderError> {
    let output = match stream_attempt(context, grant, client, request, cancel, false).await {
        Ok(v) => v,
        Err(ExecutionError::Reader(e)) => return Err(e),
        Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
            return failed(
                context,
                grant,
                if matches!(e, TransportError::Cancelled) {
                    ErrorCode::WORK_CANCELLED
                } else {
                    ErrorCode::PROVIDER_ERROR
                },
            )
            .await;
        }
    };
    let parsed = serde_json::from_str::<serde_json::Value>(&output.content);
    let valid = parsed
        .as_ref()
        .is_ok_and(|v| input.validate_output(&grant.work.kind, v).is_ok());
    if !valid {
        let repair_used = if grant.work.kind == WorkKind::ProbeReport {
            context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?
                .is_some()
        } else {
            false
        };
        if grant.work.kind == WorkKind::ProbeReport && grant.work.attempt < 3 && !repair_used {
            let mut error = ErrorPayload {
                code: ErrorCode::JUDGE_INVALID_VERDICT,
                text: Text::new("Probe report failed schema or semantic validation")
                    .expect("literal"),
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
            error.retryable = false;
            return acknowledged(context.submit.submit(ExecutorRequest::Failed{work_id:grant.work.work_id,attempt:grant.work.attempt,fence:grant.fence,epoch:grant.epoch,error,repair:Some(JudgeRepairData{attempt:grant.work.attempt+1,rejected_output:output.content,validation_errors:vec!["Invalid schema, references, locked predictions or unexpected-result branches".into()]})}).await);
        }
        return failed(
            context,
            grant,
            if grant.work.kind == WorkKind::ProbeReport {
                ErrorCode::JUDGE_INVALID_VERDICT
            } else {
                ErrorCode::MODE_INVALID_OUTPUT
            },
        )
        .await;
    }
    let completed = CompletedGeneration {
        generation: identity(grant),
        model: grant.binding.model.clone(),
        finish_reason: output.finish_reason,
        usage: output.usage,
    };
    let contest_id = grant.work.contest_id.ok_or(ReaderError::NotRunnable)?;
    let step_id = grant.work.step_id.ok_or(ReaderError::NotRunnable)?;
    let value = parsed.map_err(|_| ReaderError::NotRunnable)?;
    let payload = match grant.work.kind {
        WorkKind::ProbeExplain => EnvelopePayload::ProbeExplanation(ProbeOutput {
            contest_id,
            step_id,
            completed,
            data: validation::decode(&value).map_err(|_| ReaderError::NotRunnable)?,
        }),
        WorkKind::ProbeCheck => EnvelopePayload::ProbeCheck(ProbeOutput {
            contest_id,
            step_id,
            completed,
            data: validation::decode(&value).map_err(|_| ReaderError::NotRunnable)?,
        }),
        WorkKind::ProbePredict => EnvelopePayload::ProbePrediction(ProbeOutput {
            contest_id,
            step_id,
            completed,
            data: validation::decode(&value).map_err(|_| ReaderError::NotRunnable)?,
        }),
        WorkKind::ProbeReport => EnvelopePayload::ProbeReport(ProbeOutput {
            contest_id,
            step_id,
            completed,
            data: validation::decode(&value).map_err(|_| ReaderError::NotRunnable)?,
        }),
        _ => return Err(ReaderError::NotRunnable),
    };
    acknowledged(
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
    )
}
