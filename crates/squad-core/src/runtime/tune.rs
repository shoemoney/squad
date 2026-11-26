//! Frozen tune invocation delivered through one leased, fresh history context.
use squad_agents::{
    agents::ExecutionError,
    openrouter::{ChatMessage, OpenRouterClient},
};
use squad_protocol::tune::*;
use squad_protocol::*;
pub async fn execute(
    context: &AgentContext,
    grant: &ExecutionGrant,
    client: &OpenRouterClient,
    pack: &crate::prompts::PromptPack,
    config: &SquadConfig,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), ExecutionError> {
    let history = context.history.for_work(grant.work.work_id).await?;
    let invocation: serde_json::Value =
        serde_json::from_str(&history.last().ok_or(ReaderError::NotRunnable)?.content)
            .map_err(|_| ReaderError::NotRunnable)?;
    let input: TuneCaseMaterial = serde_json::from_value(invocation["input"].clone())
        .map_err(|_| ReaderError::NotRunnable)?;
    if input.kind() != grant.work.kind {
        return Err(ReaderError::Forbidden.into());
    }
    let model: squad_agents::resolver::Model = serde_json::from_value(invocation["model"].clone())
        .map_err(|_| ReaderError::NotRunnable)?;
    let request = if let TuneCaseMaterial::Execute { prompt, input } = &input {
        crate::router::executor::executor_call(grant, prompt, input, model.output_limit)
            .map_err(ExecutionError::Transport)?
    } else {
        let repair = context
            .work
            .repair_for_attempt(grant.work.work_id, grant.work.attempt)
            .await?;
        let mut values = pack
            .new_mode_inputs(
                &grant.work,
                &serde_json::to_value(&input).map_err(|_| ReaderError::NotRunnable)?,
                repair.as_ref(),
            )
            .map_err(|_| ReaderError::NotRunnable)?;
        // Limits are frozen with the source, and cannot drift on restart.
        if let Some(v) = values.get_mut("room_word_limit") {
            *v = invocation["roomWords"].clone();
        }
        if let Some(v) = values.get_mut("step_word_limit") {
            *v = invocation["stepWords"].clone();
        }
        let frozen = &invocation["frozen"];
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
        let source = format!("{system}\n{user}");
        values.retain(|key, _| source.contains(&format!("{{{{{key}}}}}")));
        let rendered = pack
            .render_frozen_composition(
                work_name(&grant.work.kind),
                frozen[if repair.is_some() {
                    "repairSystem"
                } else {
                    "system"
                }]
                .as_str()
                .ok_or(ReaderError::NotRunnable)?,
                frozen[if repair.is_some() {
                    "repairUser"
                } else {
                    "user"
                }]
                .as_str()
                .ok_or(ReaderError::NotRunnable)?,
                &values,
            )
            .map_err(|_| ReaderError::NotRunnable)?;
        let member = match &grant.work.target {
            Target::Member(n) => config.agents.iter().find(|a| &a.nick == n),
            Target::Judge => None,
        };
        let mut request = squad_agents::registry::Registry::new(config)
            .map_err(|_| ReaderError::NotRunnable)?
            .work_request(member, &model, grant)
            .map_err(ExecutionError::Transport)?;
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
            serde_json::json!({"type":"json_schema","json_schema":{"name":"tune","strict":true,"schema":tune::output_schema(&grant.work.kind)}}),
        );
        request
    };
    let max = request.body().map_err(ExecutionError::Transport)?["max_tokens"]
        .as_u64()
        .ok_or(ReaderError::NotRunnable)?;
    if squad_agents::context::estimate(&request.messages)
        > squad_agents::context::input_budget(u64::from(model.context_length), max)
    {
        return squad_agents::agents::failed(context, grant, ErrorCode::CONTEXT_TOO_LARGE)
            .await
            .map_err(Into::into);
    }
    squad_agents::agents::tune::execute(
        context,
        grant,
        client,
        request,
        &input,
        invocation["candidateId"].as_str(),
        invocation["caseLabel"].as_str(),
        crate::orchestrator::tune::validate_output,
        cancel,
    )
    .await
}
