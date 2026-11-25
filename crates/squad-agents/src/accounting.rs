//! Attempt snapshots submitted to router-owned atomic persistence.
use crate::openrouter::{OpenRouterClient, Usage as ProviderUsage};
use squad_protocol::*;

pub fn known_usage(usage: &ProviderUsage) -> Option<Usage> {
    Some(Usage {
        input_tokens: usage.prompt_tokens?,
        output_tokens: usage.completion_tokens?,
        cost_usd: usage.cost.filter(|cost| cost.is_finite() && *cost >= 0.0),
    })
}
/// One instance per admitted provider call. Repeated submissions upsert the same
/// `(work_id, attempt)`; accounting never owns a daily total or increments money.
pub struct AttemptAccounting {
    pub record: UsageRecord,
    fence: u64,
    epoch: u64,
    observed: ProviderUsage,
}
impl AttemptAccounting {
    pub fn new(grant: &ExecutionGrant, started_at: DateTime, day: String) -> Self {
        Self {
            record: UsageRecord {
                work_id: grant.work.work_id,
                attempt: grant.work.attempt,
                target: grant.work.target.clone(),
                context_id: grant.work.context_id.clone(),
                provider_request_id: None,
                model: grant.binding.model.clone(),
                started_at,
                ended_at: None,
                outcome: UsageOutcome::Unknown,
                usage: None,
                estimated_cost_usd: None,
                debate_id: grant.work.debate_id,
                case_id: grant.work.case_id,
                contest_id: grant.work.contest_id,
                response_model: None,
                day,
            },
            fence: grant.fence,
            epoch: grant.epoch,
            observed: ProviderUsage::default(),
        }
    }
    /// Rehydrate the original accounting identity for delayed reconciliation.
    pub fn from_record(record: UsageRecord, fence: u64, epoch: u64) -> Result<Self, ReaderError> {
        record.validate().map_err(|_| ReaderError::StaleWork)?;
        let observed = ProviderUsage {
            prompt_tokens: record.usage.as_ref().map(|u| u.input_tokens),
            completion_tokens: record.usage.as_ref().map(|u| u.output_tokens),
            cost: record.usage.as_ref().and_then(|u| u.cost_usd),
            ..ProviderUsage::default()
        };
        Ok(Self {
            record,
            fence,
            epoch,
            observed,
        })
    }
    pub fn metadata(&mut self, id: Option<String>, model: Option<String>) {
        if let Some(id) = id.filter(|id| !id.is_empty()) {
            self.record.provider_request_id = Some(id);
        }
        if let Some(model) = model.and_then(|m| Text::new(m).ok()) {
            self.record.response_model = Some(model);
        }
    }
    pub fn usage(&mut self, usage: &ProviderUsage) {
        if let Some(tokens) = usage.prompt_tokens {
            self.observed.prompt_tokens = Some(tokens);
        }
        if let Some(tokens) = usage.completion_tokens {
            self.observed.completion_tokens = Some(tokens);
        }
        if let Some(cost) = usage.cost.filter(|cost| cost.is_finite() && *cost >= 0.0) {
            self.observed.cost = Some(cost);
        }
        if let Some(mut incoming) = known_usage(&self.observed) {
            if incoming.cost_usd.is_none() {
                incoming.cost_usd = self.record.usage.as_ref().and_then(|u| u.cost_usd);
            }
            self.record.usage = Some(incoming);
        }
    }
    pub fn finish(&mut self, outcome: UsageOutcome, ended_at: DateTime) {
        self.record.outcome = outcome;
        self.record.ended_at = Some(ended_at);
    }
    /// Unknown values remain unknown. Generation lookups merge into this call,
    /// retaining the original local billing day and requested concrete identity.
    pub async fn reconcile(
        &mut self,
        client: &OpenRouterClient,
    ) -> Result<(), crate::openrouter::TransportError> {
        if self
            .record
            .usage
            .as_ref()
            .is_some_and(|u| u.cost_usd.is_some())
        {
            return Ok(());
        }
        let Some(id) = self.record.provider_request_id.as_ref() else {
            return Ok(());
        };
        let value = client.generation(id).await?;
        let data = value.get("data").unwrap_or(&value);
        let old = self.record.usage.as_ref();
        let usage = ProviderUsage {
            prompt_tokens: data
                .get("tokens_prompt")
                .or_else(|| data.get("prompt_tokens"))
                .and_then(serde_json::Value::as_u64)
                .or(self.observed.prompt_tokens)
                .or(old.map(|u| u.input_tokens)),
            completion_tokens: data
                .get("tokens_completion")
                .or_else(|| data.get("completion_tokens"))
                .and_then(serde_json::Value::as_u64)
                .or(self.observed.completion_tokens)
                .or(old.map(|u| u.output_tokens)),
            cost: data
                .get("total_cost")
                .or_else(|| data.get("cost"))
                .and_then(serde_json::Value::as_f64),
            ..ProviderUsage::default()
        };
        self.usage(&usage);
        self.metadata(
            None,
            data.get("model")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        );
        Ok(())
    }
    pub async fn submit(&self, ingress: &dyn ExecutorSubmit) -> Result<(), ReaderError> {
        self.record.validate().map_err(|_| ReaderError::StaleWork)?;
        acknowledged(
            ingress
                .submit(ExecutorRequest::Account {
                    fence: self.fence,
                    epoch: self.epoch,
                    record: self.record.clone(),
                })
                .await,
        )
    }
}
/// An acknowledgement is required; a rejected result never counts as success.
pub fn acknowledged(result: RequestResult) -> Result<(), ReaderError> {
    match result {
        RequestResult::Accepted { .. } => Ok(()),
        RequestResult::Rejected { error, .. } => Err(match error.code {
            ErrorCode::WORK_CANCELLED | ErrorCode::STALE_WORK_ATTEMPT | ErrorCode::ROOM_CLEARED => {
                ReaderError::StaleWork
            }
            _ => ReaderError::StoreUnavailable {
                message: "executor submission rejected".into(),
            },
        }),
        _ => Err(ReaderError::StaleWork),
    }
}
