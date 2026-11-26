//! Built-in MCP adapter. The core room owns execution, persistence and billing.
pub mod backend;
pub mod http;
pub mod types;

use backend::{Backend, BackendError};
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig},
    service::ServerInitializeError,
    tool, tool_handler, tool_router,
};
use serde_json::{Value, json};
use squad_protocol::SquadConfig;
use std::sync::Arc;
use types::{ConfirmArgs, RunArgs, StartArgs};

#[derive(Clone)]
pub struct SquadMcp {
    backend: Arc<Backend>,
    tool_router: ToolRouter<Self>,
}

fn result(value: Result<Value, BackendError>) -> CallToolResult {
    match value {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(json!({
            "code": error.code, "message": error.message,
        })),
    }
}

#[tool_router]
impl SquadMcp {
    pub fn new(backend: Arc<Backend>) -> Self {
        Self {
            backend,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Read Squad's configured roster, resolved models and current daily budget. Uses the same configured room as Squad.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn squad_roster(&self) -> CallToolResult {
        result(self.backend.roster().await)
    }

    #[tool(
        description = "Start a multi-model elimination tournament for a brief, such as selecting the best comprehensive product/build plan. Omit participants for @all. Returns a runId immediately; poll status and retrieve result separately. Set confirm=true only when authorized to approve the displayed reservation; the configured daily cap always applies. Supply a stable request_id UUID when retrying to avoid duplicate runs.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn squad_tournament_start(
        &self,
        Parameters(args): Parameters<StartArgs>,
    ) -> CallToolResult {
        result(self.backend.start_tournament(args).await)
    }

    #[tool(
        description = "Approve or decline the cost preview of a pending tournament. Use its runId and confirm=true to begin paid model calls, or false to decline.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn squad_contest_confirm(
        &self,
        Parameters(args): Parameters<ConfirmArgs>,
    ) -> CallToolResult {
        result(self.backend.confirm(args).await)
    }

    #[tool(
        description = "Read compact tournament progress, phase, work counts, cost and errors by runId. Long-running tournaments continue between requests.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn squad_contest_status(&self, Parameters(args): Parameters<RunArgs>) -> CallToolResult {
        result(self.backend.status(args).await)
    }

    #[tool(
        description = "Read a tournament's winning plan, champion, rankings, placements, actual usage and Markdown export. The report preserves failures and incomplete outcomes; it does not claim that proposed plans were executed.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn squad_contest_result(&self, Parameters(args): Parameters<RunArgs>) -> CallToolResult {
        result(self.backend.result(args).await)
    }

    #[tool(
        description = "Cancel the specified pending or active tournament and release its unstarted reservations. Completed provider calls remain billable. Unrelated ordinary work and history are preserved.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true
        )
    )]
    async fn squad_contest_cancel(&self, Parameters(args): Parameters<RunArgs>) -> CallToolResult {
        result(self.backend.cancel(args).await)
    }

    #[tool(
        description = "List durable tournament run IDs and compact progress from the configured room, including runs recovered after a server restart.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn squad_contests_list(&self) -> CallToolResult {
        result(self.backend.list_runs().await)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SquadMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("squad", env!("CARGO_PKG_VERSION")))
            .with_instructions("Squad runs model tournaments through its normal room engine. Start returns a runId; status/result are separate calls. Pending cost previews require explicit confirmation. Reuse request_id when retrying starts. Results are proposals, not evidence that a plan was built or tested. Never send credentials in a brief.")
    }
}

pub async fn run(
    config: Result<SquadConfig, String>,
    args: crate::args::Mcp,
) -> Result<(), String> {
    let backend = Arc::new(Backend::new(config));
    let server = SquadMcp::new(backend.clone());
    let serving = if let Some(listen) = args.http {
        let public_url = match args.public_url {
            Some(url) => url,
            None => reqwest::Url::parse(&format!("http://{listen}/mcp"))
                .map_err(|_| "Invalid MCP listener URL".to_owned())?,
        };
        let auth = if let Some(path) = args.token_file {
            http::AuthOptions::BearerFile(path)
        } else {
            http::AuthOptions::OAuth(http::OAuthOptions {
                issuer: args
                    .oauth_issuer
                    .ok_or("HTTP requires bearer or OAuth authentication")?,
                audience: args.oauth_audience.ok_or("OAuth requires an audience")?,
                owner_subject: args
                    .oauth_subject
                    .ok_or("OAuth requires the allowed owner subject")?,
            })
        };
        http::serve(
            server,
            http::HttpOptions {
                listen,
                public_url,
                auth,
            },
        )
        .await
    } else {
        serve_stdio(server).await
    };
    let stopped = backend.shutdown().await.map_err(|error| error.message);
    serving.and(stopped)
}

async fn serve_stdio(server: SquadMcp) -> Result<(), String> {
    let cancellation = tokio_util::sync::CancellationToken::new();
    let stopped = cancellation.clone();
    let serving = async move {
        let service = match server
            .serve_with_ct(rmcp::transport::stdio_pipes(), stopped)
            .await
        {
            Ok(service) => service,
            Err(ServerInitializeError::Cancelled) => return Ok(()),
            Err(_) => return Err("MCP protocol startup failed".to_owned()),
        };
        service
            .waiting()
            .await
            .map_err(|_| "MCP service task failed".to_owned())?;
        Ok(())
    };
    tokio::pin!(serving);
    tokio::select! {
        result = &mut serving => result,
        () = shutdown_signal() => {
            cancellation.cancel();
            tokio::time::timeout(std::time::Duration::from_secs(5), serving).await
                .map_err(|_| "MCP shutdown timed out".to_owned())?
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
