//! Execute only the last frozen step projection through its leased work reader.
use squad_protocol::*;
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &squad_agents::openrouter::OpenRouterClient,
    _pack: &crate::prompts::PromptPack,
    config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), squad_agents::agents::ExecutionError> {
    let history = context.history.for_work(grant.work.work_id).await?;
    let bundle: serde_json::Value =
        serde_json::from_str(&history.last().ok_or(ReaderError::NotRunnable)?.content)
            .map_err(|_| ReaderError::NotRunnable)?;
    let model: squad_agents::resolver::Model =
        serde_json::from_value(bundle["model"].clone()).map_err(|_| ReaderError::NotRunnable)?;
    let registry =
        squad_agents::registry::Registry::new(config).map_err(|_| ReaderError::NotRunnable)?;
    let member = config
        .agents
        .iter()
        .find(|a| grant.work.target == Target::Member(a.nick.clone()));
    let request = registry
        .work_request(member, &model, grant)
        .map_err(|_| ReaderError::NotRunnable)?;
    let executor = squad_agents::agents::rank::RankExecutor {
        client: client.clone(),
        request,
        context_length: u64::from(model.context_length),
        validate: crate::orchestrator::rank::validate_output,
    };
    squad_agents::worker::WorkExecutor::execute(&executor, context, grant.clone(), cancel)
        .await
        .map_err(Into::into)
}
