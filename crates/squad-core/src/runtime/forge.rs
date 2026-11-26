//! Assemble one isolated Forge invocation from the work-bound history capability.
use squad_protocol::*;
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &squad_agents::openrouter::OpenRouterClient,
    pack: &crate::prompts::PromptPack,
    config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), squad_agents::agents::ExecutionError> {
    let h = context
        .history
        .for_work(grant.work.work_id)
        .await
        .inspect_err(|e| tracing::error!(error=?e,"Forge history read failed"))?;
    let input: serde_json::Value =
        serde_json::from_str(&h.last().ok_or(ReaderError::NotRunnable)?.content).map_err(|e| {
            tracing::error!(error=?e,"Forge invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    let data = &input["data"];
    let repair = context
        .work
        .repair_for_attempt(grant.work.work_id, grant.work.attempt)
        .await
        .inspect_err(|e| tracing::error!(error=?e,"Forge repair read failed"))?;
    let name = squad_store::forge::composition(&grant.work.kind).ok_or(ReaderError::NotRunnable)?;
    let source = &input["prompt"];
    let system = source[if repair.is_some() {
        "repairSystem"
    } else {
        "system"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let user = source[if repair.is_some() {
        "repairUser"
    } else {
        "user"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let values =
        crate::prompts::inputs_forge::frozen(&grant.work, data, repair.as_ref(), system, user)
            .map_err(|e| {
                tracing::error!(error=?e,"Forge invocation preparation failed");
                ReaderError::NotRunnable
            })?;
    let rendered = pack
        .render_frozen_composition(name, system, user, &values)
        .map_err(|e| {
            tracing::error!(error=?e,"Forge invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    let model: squad_agents::resolver::Model = serde_json::from_value(input["model"].clone())
        .map_err(|e| {
            tracing::error!(error=?e,"Forge invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    let member = match &grant.work.target {
        Target::Member(n) => config.agents.iter().find(|m| &m.nick == n),
        Target::Judge => None,
    };
    let mut request = squad_agents::registry::Registry::new(config)
        .map_err(|_| ReaderError::NotRunnable)?
        .work_request(member, &model, grant)
        .map_err(|e| {
            tracing::error!(error=?e,"Forge invocation preparation failed");
            ReaderError::NotRunnable
        })?;
    request.messages = vec![
        squad_agents::openrouter::ChatMessage {
            role: "system".into(),
            name: None,
            content: rendered.system,
        },
        squad_agents::openrouter::ChatMessage {
            role: "user".into(),
            name: Some("squad".into()),
            content: rendered.user,
        },
    ];
    request.response_format = Some(
        serde_json::json!({"type":"json_schema","json_schema":{"name":"forge_response","strict":true,"schema":forge::output_schema(&grant.work.kind)}}),
    );
    let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
        .as_u64()
        .ok_or(ReaderError::NotRunnable)?;
    if squad_agents::context::estimate(&request.messages)
        > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
    {
        return Ok(
            squad_agents::agents::failed(context, grant, ErrorCode::CONTEXT_TOO_LARGE).await?,
        );
    }
    squad_agents::agents::forge::execute(context, grant, client, request, data, cancel).await
}
