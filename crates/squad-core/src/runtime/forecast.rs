//! Recover the immutable invocation through the work-scoped history capability.
use squad_protocol::*;
use squad_agents::worker::WorkExecutor;
pub async fn execute(context:&AgentContext,grant:&ExecutionGrant,client:&squad_agents::openrouter::OpenRouterClient,_pack:&crate::prompts::PromptPack,config:&SquadConfig,cancel:tokio_util::sync::CancellationToken)->Result<(),squad_agents::agents::ExecutionError>{
 let history=context.history.for_work(grant.work.work_id).await?;
 let input:serde_json::Value=serde_json::from_str(&history.last().ok_or(ReaderError::NotRunnable)?.content).map_err(|_|ReaderError::NotRunnable)?;
 let model:squad_agents::resolver::Model=serde_json::from_value(input["model"].clone()).map_err(|_|ReaderError::NotRunnable)?;
 let member=match &grant.work.target{Target::Member(n)=>config.agents.iter().find(|a|&a.nick==n),Target::Judge=>None};
 let registry=squad_agents::registry::Registry::new(config).map_err(|_|ReaderError::NotRunnable)?;
 let request=registry.work_request(member,&model,grant).map_err(|_|ReaderError::NotRunnable)?;
 let executor=squad_agents::agents::forecast::ForecastExecutor{client:client.clone(),request,context_length:u64::from(model.context_length)};
 executor.execute(context,grant.clone(),cancel).await.map_err(Into::into)
}
