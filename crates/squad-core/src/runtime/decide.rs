//! One provider attempt using the last frozen, work-bound history projection.
use squad_agents::{
    agents::{ExecutionError, decide::DecideExecutor},
    openrouter::{ChatRequest, DeadlineClass, ExecutionPolicy},
    worker::WorkExecutor,
};
use squad_protocol::*;
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &squad_agents::openrouter::OpenRouterClient,
    _pack: &crate::prompts::PromptPack,
    _config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), ExecutionError> {
    let history = context.history.for_work(grant.work.work_id).await?;
    let frozen: serde_json::Value =
        serde_json::from_str(&history.last().ok_or(ReaderError::NotRunnable)?.content)
            .map_err(|_| ReaderError::NotRunnable)?;
    let request = ChatRequest {
        model: grant.binding.model.to_string(),
        messages: vec![],
        visible_ceiling: grant
            .work
            .max_output_tokens
            .or_else(|| grant.work.kind.new_mode_visible_tokens())
            .ok_or(ReaderError::NotRunnable)?,
        published_output_limit: frozen["maxOutput"]
            .as_u64()
            .ok_or(ReaderError::NotRunnable)? as u32,
        supports_reasoning: frozen["reasoning"].as_bool().unwrap_or(false),
        reasoning_effort: grant.binding.effort,
        reasoning_explicit: false,
        response_format: None,
        policy: ExecutionPolicy::SingleAttempt(if grant.work.kind.new_mode_judge() {
            DeadlineClass::FinalJudge
        } else {
            DeadlineClass::DebateTurn
        }),
    };
    let executor = DecideExecutor {
        client: client.clone(),
        request,
        context_length: frozen["contextLength"]
            .as_u64()
            .ok_or(ReaderError::NotRunnable)?,
    };
    executor
        .execute(context, grant.clone(), cancel)
        .await
        .map_err(|e| {
            tracing::warn!(work_id=%grant.work.work_id,error=?e,"decide executor failed");
            e.into()
        })
}
