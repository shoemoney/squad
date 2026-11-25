//! Prepare a fresh request from the owning work capability, never room history.
use super::*;
use squad_agents::{openrouter::ChatMessage, worker::WorkExecutor};
pub(super) async fn execute(
    context: &AgentContext,
    grant: ExecutionGrant,
    cancel: CancellationToken,
    client: &OpenRouterClient,
    config: &SquadConfig,
    pack: &PromptPack,
    models: &squad_agents::resolver::Catalog,
) -> Result<(), ReaderError> {
    match prepare(context, &grant, client, config, pack, models).await {
        Ok(executor) => executor.execute(context, grant, cancel).await,
        Err(error) => {
            squad_agents::agents::failed(context, &grant, ErrorCode::CONTEXT_TOO_LARGE).await?;
            Err(error)
        }
    }
}
async fn prepare(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    config: &SquadConfig,
    pack: &PromptPack,
    models: &squad_agents::resolver::Catalog,
) -> Result<squad_agents::agents::crosscheck::CrosscheckExecutor, ReaderError> {
    let values = context
        .work
        .prompt_values_for_work(grant.work.work_id)
        .await?;
    let material: CrosscheckCaseMaterial = serde_json::from_str(
        values
            .get("crosscheck_material_json")
            .ok_or(ReaderError::Forbidden)?,
    )
    .map_err(|_| ReaderError::Forbidden)?;
    let repair = if grant.work.kind == WorkKind::CrosscheckJudge {
        context
            .work
            .repair_for_attempt(grant.work.work_id, grant.work.attempt)
            .await?
    } else {
        None
    };
    let inputs = pack
        .crosscheck_inputs(
            config,
            &grant.binding,
            &grant.work,
            &material,
            repair.as_ref(),
        )
        .map_err(|_| ReaderError::NotRunnable)?;
    let rendered = pack
        .render_composition(material.composition(), &inputs, repair.is_some())
        .map_err(|_| ReaderError::NotRunnable)?;
    let model = models
        .get(grant.binding.model.as_str())
        .ok_or(ReaderError::NotRunnable)?;
    let member = match &grant.work.target {
        Target::Member(n) => config.agents.iter().find(|a| a.nick == *n),
        _ => None,
    };
    let registry =
        squad_agents::registry::Registry::new(config).map_err(|_| ReaderError::NotRunnable)?;
    let mut request = registry
        .work_request(member, model, &grant)
        .map_err(|_| ReaderError::NotRunnable)?;
    request.messages = vec![
        ChatMessage {
            role: "system".into(),
            name: None,
            content: rendered.system,
        },
        ChatMessage {
            role: "user".into(),
            name: Some("squad".into()),
            content: rendered.user,
        },
    ];
    request.response_format = Some(
        serde_json::json!({"type":"json_schema","json_schema":{"name":"crosscheck","strict":true,"schema":material.response_schema()}}),
    );
    let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
        .as_u64()
        .ok_or(ReaderError::NotRunnable)?;
    if squad_agents::context::estimate(&request.messages)
        > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
    {
        return Err(ReaderError::NotRunnable);
    }
    Ok(squad_agents::agents::crosscheck::CrosscheckExecutor {
        client: client.clone(),
        request,
        material,
    })
}
