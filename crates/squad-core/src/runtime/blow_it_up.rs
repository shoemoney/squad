//! Assemble one call from the frozen, capability-filtered step history.
use squad_agents::{agents::ExecutionError, openrouter::*};
use squad_protocol::*;
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    pack: &crate::prompts::PromptPack,
    _config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), ExecutionError> {
    let history = context.history.for_work(grant.work.work_id).await?;
    let frozen: serde_json::Value = serde_json::from_str(
        &history.last().ok_or(ReaderError::NotRunnable)?.content,
    )
    .map_err(|e| {
        tracing::warn!(error=?e,"Blow It Up preparation failed");
        ReaderError::NotRunnable
    })?;
    let data = &frozen["data"];
    let repair = context
        .work
        .repair_for_attempt(grant.work.work_id, grant.work.attempt)
        .await?;
    let system = frozen["prompt"][if repair.is_some() {
        "repairSystem"
    } else {
        "system"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let user = frozen["prompt"][if repair.is_some() {
        "repairUser"
    } else {
        "user"
    }]
    .as_str()
    .ok_or(ReaderError::NotRunnable)?;
    let inputs = crate::prompts::inputs_blow_it_up::frozen_inputs(
        &grant.work,
        data,
        repair.as_ref(),
        system,
        user,
    )
    .map_err(|e| {
        tracing::warn!(error=?e,"Blow It Up preparation failed");
        ReaderError::NotRunnable
    })?;
    let name = squad_protocol::blow_it_up::composition(&grant.work.kind)
        .ok_or(ReaderError::NotRunnable)?;
    let rendered = pack
        .render_frozen_composition(name, system, user, &inputs)
        .map_err(|e| {
            tracing::warn!(error=?e,"Blow It Up preparation failed");
            ReaderError::NotRunnable
        })?;
    let request = ChatRequest {
        model: grant.binding.model.to_string(),
        messages: vec![
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
        ],
        visible_ceiling: frozen["visibleTokens"]
            .as_u64()
            .ok_or(ReaderError::NotRunnable)? as u32,
        published_output_limit: frozen["outputLimit"]
            .as_u64()
            .ok_or(ReaderError::NotRunnable)? as u32,
        supports_reasoning: frozen["supportsReasoning"].as_bool().unwrap_or(false),
        reasoning_effort: constants::effective_effort(&grant.work.kind, grant.binding.effort),
        reasoning_explicit: false,
        response_format: Some(
            serde_json::json!({"type":"json_schema","json_schema":{"name":"blow_it_up","strict":true,"schema":squad_protocol::blow_it_up::output_schema(&grant.work.kind)}}),
        ),
        policy: ExecutionPolicy::SingleAttempt(if grant.work.target == Target::Judge {
            DeadlineClass::FinalJudge
        } else {
            DeadlineClass::DebateTurn
        }),
    };
    let reservation = request.body().map_err(ExecutionError::Transport)?["max_tokens"]
        .as_u64()
        .ok_or(ReaderError::NotRunnable)?;
    if squad_agents::context::estimate(&request.messages)
        > squad_agents::context::input_budget(
            frozen["contextLength"]
                .as_u64()
                .ok_or(ReaderError::NotRunnable)?,
            reservation,
        )
    {
        return squad_agents::agents::failed(context, grant, ErrorCode::CONTEXT_TOO_LARGE)
            .await
            .map_err(Into::into);
    }
    squad_agents::agents::blow_it_up::execute(
        context,
        grant,
        client,
        request,
        data,
        crate::router::blow_it_up::validate_output,
        repair.is_some(),
        cancel,
    )
    .await
    .map_err(Into::into)
}
