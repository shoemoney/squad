//! One provider attempt; repair and retry scheduling remain router-owned.
//! The shared failure classifier currently recognizes JUDGE_INVALID_MODE_OUTPUT for
//! every structured repair, including member work; use it until the integrator
//! adds MODE_INVALID_OUTPUT to that classifier.
use super::*;
use serde_json::Value;
pub fn enabled() -> bool {
    squad_protocol::forge::ENABLED
}
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    request: ChatRequest,
    data: &Value,
    cancel: CancellationToken,
) -> Result<(), ExecutionError> {
    let output = match stream_attempt(context, grant, client, request, cancel, false).await {
        Ok(o) => o,
        Err(ExecutionError::Reader(e)) => return Err(e.into()),
        Err(ExecutionError::Transport(e) | ExecutionError::RetryAfter(e, _)) => {
            return failed_with(context, grant, ErrorCode::PROVIDER_ERROR, Some(&e))
                .await
                .map_err(Into::into);
        }
    };
    let validated = serde_json::from_str::<Value>(squad_protocol::validation::strip_code_fence(
        &output.content,
    ))
    .map_err(|_| "Return one complete JSON object".to_owned())
    .and_then(|v| {
        squad_protocol::forge::validate_output(&grant.work.kind, data, &v)
            .map(|()| v)
            .map_err(|e| e.message)
    });
    match validated {
        Ok(body) => {
            let actor = match &grant.work.target {
                Target::Judge => Actor::Judge,
                Target::Member(n) => Actor::Member(n.clone()),
            };
            let name = match grant.work.kind {
                WorkKind::ForgeDraft => "forge.draft",
                WorkKind::ForgeRank => "forge.rank",
                WorkKind::ForgeFuse => "forge.fuse",
                WorkKind::ForgeJudge => "forge.judge",
                _ => return Err(ReaderError::Forbidden.into()),
            };
            acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Result {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        output: EnvelopePayload::ContestArtifact(ContestArtifact {
                            kind: Some(ContestKind::Forge),
                            round: Some(0),
                            actor: Some(actor),
                            audience: Some(ArtifactAudience::Human),
                            body: Some(body.clone()),
                            contest_id: grant.work.contest_id.ok_or(ReaderError::NotRunnable)?,
                            step_id: grant.work.step_id.ok_or(ReaderError::NotRunnable)?,
                            artifact_kind: name.parse().map_err(|_| ReaderError::NotRunnable)?,
                            work_id: Some(grant.work.work_id),
                            source_refs: vec![],
                            data: body,
                        }),
                    })
                    .await,
            )
            .map_err(Into::into)
        }
        Err(reason) => {
            let judge = grant.work.kind == WorkKind::ForgeJudge;
            let prior = context
                .work
                .repair_for_attempt(grant.work.work_id, grant.work.attempt)
                .await?;
            let repair =
                (grant.work.attempt < if judge { 3 } else { 2 } && prior.is_none()).then(|| {
                    JudgeRepairData {
                        attempt: grant.work.attempt + 1,
                        rejected_output: output.content,
                        validation_errors: vec![reason],
                    }
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
                            code: ErrorCode::JUDGE_INVALID_MODE_OUTPUT,
                            text: "Invalid forge response".parse().expect("literal"),
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
            .map_err(Into::into)
        }
    }
}
