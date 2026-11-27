//! forge work preparation belongs here.
use squad_protocol::*;
pub async fn execute(
    _context: &AgentContext,
    _grant: &ExecutionGrant,
    _client: &squad_agents::openrouter::OpenRouterClient,
    _pack: &crate::prompts::PromptPack,
    _config: &SquadConfig,
    _cancel: tokio_util::sync::CancellationToken,
) -> Result<(), squad_agents::agents::ExecutionError> {
    Err(ReaderError::NotRunnable.into())
}
