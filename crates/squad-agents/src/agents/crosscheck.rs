//! Structured crosscheck calls. Each invocation uses only one authorized step payload.
use super::*;
use crate::accounting::acknowledged;
use crate::worker::{ExecutionFuture, WorkExecutor};
pub struct CrosscheckExecutor {
    pub client: OpenRouterClient,
    pub request: ChatRequest,
    pub material: CrosscheckCaseMaterial,
}
impl WorkExecutor for CrosscheckExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a> {
        Box::pin(async move {
            if !grant.work.kind.is_crosscheck() || grant.work.kind != self.material.work_kind() {
                return Err(ReaderError::Forbidden);
            }
            let output = match stream_attempt(
                context,
                &grant,
                &self.client,
                self.request.clone(),
                cancel,
                false,
            )
            .await
            {
                Ok(o) => o,
                Err(ExecutionError::Reader(e)) => return Err(e),
                Err(_) => {
                    failed(context, &grant, ErrorCode::PROVIDER_ERROR).await?;
                    return Err(ReaderError::NotRunnable);
                }
            };
            let data = serde_json::from_str::<serde_json::Value>(&output.content).ok();
            let valid = data
                .as_ref()
                .is_some_and(|data| validate_response(&self.material, data.clone()).is_ok());
            if !valid {
                let mut error = ErrorPayload {
                    code: ErrorCode::JUDGE_INVALID_MODE_OUTPUT,
                    text: Text::new("Crosscheck response failed validation").expect("literal"),
                    retryable: false,
                    related_event_id: Some(grant.work.trigger_event_id),
                    work_id: Some(grant.work.work_id),
                    debate_id: None,
                    case_id: grant.work.case_id,
                    earliest_cursor: None,
                    retry_action: None,
                    contest_id: grant.work.contest_id,
                };
                let repair = (grant.work.kind.is_crosscheck_judge() && grant.work.attempt == 1)
                    .then(|| JudgeRepairData {
                        attempt: grant.work.attempt + 1,
                        rejected_output: output.content,
                        validation_errors: vec![
                            "Response does not match the assigned crosscheck task".into(),
                        ],
                    });
                if grant.work.kind.is_crosscheck_member() {
                    error.code = ErrorCode::MODE_INVALID_OUTPUT;
                }
                return acknowledged(
                    context
                        .submit
                        .submit(ExecutorRequest::Failed {
                            work_id: grant.work.work_id,
                            attempt: grant.work.attempt,
                            fence: grant.fence,
                            epoch: grant.epoch,
                            error,
                            repair,
                        })
                        .await,
                );
            }
            acknowledged(
                context
                    .submit
                    .submit(ExecutorRequest::Result {
                        work_id: grant.work.work_id,
                        attempt: grant.work.attempt,
                        fence: grant.fence,
                        epoch: grant.epoch,
                        output: EnvelopePayload::ContestArtifact(ContestArtifact {
                            contest_id: grant.work.contest_id.ok_or(ReaderError::Forbidden)?,
                            step_id: grant.work.step_id.ok_or(ReaderError::Forbidden)?,
                            artifact_kind: Text::new(self.material.composition())
                                .expect("composition"),
                            work_id: Some(grant.work.work_id),
                            source_refs: vec![],
                            data: data.expect("validated"),
                        }),
                    })
                    .await,
            )
        })
    }
}
pub fn validate_response(
    material: &CrosscheckCaseMaterial,
    value: serde_json::Value,
) -> validation::ValidationResult {
    match material {
        CrosscheckCaseMaterial::ExtractGoal { .. } => {
            validation::decode::<CrosscheckGoal>(&value)?;
        }
        CrosscheckCaseMaterial::Inventory { .. } => {
            validation::decode::<CrosscheckInventoryResponse>(&value)?.validate()?
        }
        CrosscheckCaseMaterial::NormalizeInventory { inventories, .. } => {
            validation::decode::<CrosscheckNormalization>(&value)?.validate(inventories)?;
        }
        CrosscheckCaseMaterial::Review { plan, cells } => {
            validation::decode::<CrosscheckReviewResponse>(&value)?.validate(
                &cells.iter().map(|c| c.cell_id).collect::<Vec<_>>(),
                plan.as_str(),
            )?
        }
        CrosscheckCaseMaterial::Audit { samples, .. } => {
            validation::decode::<CrosscheckAuditResponse>(&value)?
                .validate(&samples.iter().map(|c| c.cell_id).collect::<Vec<_>>())?
        }
        CrosscheckCaseMaterial::Rule {
            plan,
            grid,
            audits,
            reviews,
        } => {
            let ruling = validation::decode::<CrosscheckRuling>(&value)?;
            ruling.validate(grid, audits, plan.as_str())?;
            validation::ensure(
                ruling.cells.iter().all(|cell| {
                    cell.status == CrosscheckDisposition::Gap
                        || reviews
                            .iter()
                            .flat_map(|r| &r.response.cells)
                            .any(|review| review.cell_id == cell.cell_id)
                }),
                "failed review cells remain gaps",
            )?;
        }
    }
    Ok(())
}
