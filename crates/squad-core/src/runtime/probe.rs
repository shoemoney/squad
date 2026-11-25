//! Runtime assembly for fresh probe contexts; the executor owns no storage access.
use super::*;
use squad_agents::{agents::failed, openrouter::ChatMessage};
pub(super) async fn execute(
    context: &AgentContext,
    grant: ExecutionGrant,
    cancel: CancellationToken,
    config: &SquadConfig,
    pack: &PromptPack,
    models: &squad_agents::resolver::Catalog,
    client: &OpenRouterClient,
) -> Result<(), ReaderError> {
    let result = run(context, &grant, cancel, config, pack, models, client).await;
    if matches!(result, Err(ReaderError::NotRunnable)) {
        return failed(context, &grant, ErrorCode::CONFIG_INVALID).await;
    }
    result
}
async fn run(
    context: &AgentContext,
    grant: &ExecutionGrant,
    cancel: CancellationToken,
    config: &SquadConfig,
    pack: &PromptPack,
    models: &squad_agents::resolver::Catalog,
    client: &OpenRouterClient,
) -> Result<(), ReaderError> {
    let Some(model) = models.get(grant.binding.model.as_str()) else {
        return failed(context, &grant, ErrorCode::MODEL_UNAVAILABLE).await;
    };
    let values = context
        .work
        .prompt_values_for_work(grant.work.work_id)
        .await?;
    let input: ProbeInput = serde_json::from_str(
        values
            .get("mode_data_json")
            .ok_or(ReaderError::NotRunnable)?,
    )
    .map_err(|_| ReaderError::NotRunnable)?;
    let repair = if grant.work.kind == WorkKind::ProbeReport {
        context
            .work
            .repair_for_attempt(grant.work.work_id, grant.work.attempt)
            .await?
    } else {
        None
    };
    let mut frozen_config = config.clone();
    frozen_config.prompts.limits.room_words = values
        .get("room_words")
        .and_then(|v| v.parse().ok())
        .ok_or(ReaderError::NotRunnable)?;
    frozen_config.prompts.limits.plan_words = values
        .get("plan_words")
        .and_then(|v| v.parse().ok())
        .ok_or(ReaderError::NotRunnable)?;
    let (id, inputs) = pack
        .probe_inputs(
            &frozen_config,
            &grant.work,
            &grant.binding,
            &input,
            repair.as_ref(),
        )
        .map_err(|e| {
            tracing::error!(error=?e,"Probe input preparation failed");
            ReaderError::NotRunnable
        })?;
    let schema = inputs
        .get("mode_schema_json")
        .cloned()
        .ok_or(ReaderError::NotRunnable)?;
    let frozen: serde_json::Value = serde_json::from_str(
        values
            .get("probe_prompt_json")
            .ok_or(ReaderError::NotRunnable)?,
    )
    .map_err(|_| ReaderError::NotRunnable)?;
    let system = frozen[if repair.is_some() {
        "repairSystem"
    } else {
        "system"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let user = frozen[if repair.is_some() {
        "repairUser"
    } else {
        "user"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let rendered = pack
        .render_frozen_composition(id, system, user, &inputs)
        .map_err(|e| {
            tracing::error!(error=?e,"Probe composition failed");
            ReaderError::NotRunnable
        })?;
    let member = match &grant.work.target {
        Target::Member(n) => config.agents.iter().find(|m| &m.nick == n),
        Target::Judge => None,
    };
    let mut request = squad_agents::registry::Registry::new(config)
        .map_err(|_| ReaderError::NotRunnable)?
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
        serde_json::json!({"type":"json_schema","json_schema":{"name":"probe","strict":true,"schema":schema}}),
    );
    let reservation = request.body().map_err(|_| ReaderError::NotRunnable)?["max_tokens"]
        .as_u64()
        .ok_or(ReaderError::NotRunnable)?;
    if squad_agents::context::estimate(&request.messages)
        > squad_agents::context::input_budget(u64::from(model.context_length), reservation)
    {
        return failed(context, &grant, ErrorCode::CONTEXT_TOO_LARGE).await;
    }
    squad_agents::agents::probe::execute(context, &grant, client, request, &input, cancel).await
}
