# Squad MCP server

`squad mcp` gives an MCP client access to the tournament workflow in one persistent Squad room. The server uses the room's existing roster, judge, OpenRouter credentials, and daily budget. Clients can initialize and discover tools before the room opens or makes provider requests.

## Local clients over stdio

Use absolute paths for the binary, env file, and room directory so the client can launch Squad from any working directory. The env file holds `OPENROUTER_API_KEY` and any roster or budget overrides. Keep the file private. See [Squad's environment variables](../crates/squad-cli/src/env-help.md).

**Claude Code**

```sh
claude mcp add --transport stdio squad -- \
  /path/to/squad --env-file /path/to/squad.env --data-dir /path/to/squad-room mcp
```

**Codex**

```sh
codex mcp add squad -- \
  /path/to/squad --env-file /path/to/squad.env --data-dir /path/to/squad-room mcp
```

Each registration starts a Squad subprocess. Keep a stdio server running while its tournament is in progress. To use the same live room from several clients, connect them to one HTTP server with the same data directory. Only one Squad process can hold the room lock at a time. Squad writes protocol messages to stdout and diagnostics to stderr.

## Streamable HTTP and the OpenAI Responses API

On Unix, start a server on loopback with a regular file containing one bearer token. Set the file's permissions to `0600`.

```sh
squad --env-file /path/to/squad.env --data-dir /path/to/squad-room mcp \
  --http 127.0.0.1:8790 --token-file /path/to/squad-mcp.token
```

The MCP endpoint is `http://127.0.0.1:8790/mcp`. For a remote client, put a TLS reverse proxy in front of the loopback listener and set `--public-url` to its external HTTPS MCP URL, including `/mcp`. Preserve the public host and forward the authorization, MCP protocol, and session headers. Allow streamed responses to pass through without buffering. Every authorized session shares the configured room and budget.

This example uses an OpenAI Responses API model and lists existing runs. Set `OPENAI_API_KEY`, `OPENAI_MODEL`, `SQUAD_MCP_URL` (the reachable HTTPS endpoint), and `SQUAD_MCP_TOKEN` (the value stored in the server's token file) in the client environment.

```js
import OpenAI from "openai";

const client = new OpenAI();
const response = await client.responses.create({
  model: process.env.OPENAI_MODEL,
  input: "Use Squad to list my tournament runs and report their statuses.",
  tools: [{
    type: "mcp",
    server_label: "squad",
    server_url: process.env.SQUAD_MCP_URL,
    authorization: process.env.SQUAD_MCP_TOKEN,
    allowed_tools: ["squad_contests_list"],
    require_approval: "never",
  }],
});

console.log(response.output);
```

Only the read-only list tool is allowed in this example. Include the MCP tool configuration and its authorization value again on follow-up API requests. Add mutation tools when the client is authorized to start or change runs. See the [Responses API MCP guide](https://developers.openai.com/api/docs/guides/tools-connectors-mcp).

## Remote ChatGPT connection over OAuth

For the public endpoint connection in ChatGPT, deploy a Streamable HTTP endpoint through an HTTPS reverse proxy and configure an external OAuth authorization server. Start Squad with the protected resource URL, the provider's issuer, the expected access-token audience, and the subject for the single account allowed to use this room.

```sh
squad --env-file /path/to/squad.env --data-dir /path/to/squad-room mcp \
  --http 127.0.0.1:8790 \
  --public-url "$SQUAD_MCP_URL" \
  --oauth-issuer "$SQUAD_OAUTH_ISSUER" \
  --oauth-audience "$SQUAD_OAUTH_AUDIENCE" \
  --oauth-subject "$SQUAD_OAUTH_SUBJECT"
```

Configure the provider for the authorization-code flow with S256 PKCE, OAuth discovery metadata, and a ChatGPT-supported client registration method. Register the ChatGPT redirect URI if using a predefined client. Configure the provider to accept the MCP resource identifier and issue signed JWT access tokens with Squad's expected audience and owner subject. Route `/.well-known/oauth-protected-resource/mcp` and `/mcp` through the proxy and let the `WWW-Authenticate` challenge reach ChatGPT. Create the custom MCP connection using the HTTPS endpoint and OAuth authentication. Choose OAuth mode for this connection. See [ChatGPT MCP authentication](https://developers.openai.com/plugins/build/auth) and [ChatGPT connection setup](https://developers.openai.com/plugins/deploy/connect-chatgpt).

## Tournament tools

| Tool | Arguments | Result |
| --- | --- | --- |
| `squad_roster` | `{}` | Configured roster to use when selecting entrants. |
| `squad_tournament_start` | `prompt`, optional `participants`, `request_id`, and `confirm` | Starts or prepares a tournament. Returns `runId`, `contestId`, status and cost estimates. |
| `squad_contest_confirm` | `run_id`, `confirm` | Approves (`true`) or declines (`false`) a pending estimate. |
| `squad_contest_status` | `run_id` | Current status, phase, work, and usage. |
| `squad_contest_result` | `run_id` | Champion, winning plan, rankings, placements, and Markdown for a completed tournament. |
| `squad_contest_cancel` | `run_id` | Cancels the specified run. Provider requests already started can still incur charges. |
| `squad_contests_list` | `{}` | Run summaries in this room. |

1. Start with a brief and optionally a list of roster nicknames. Omit `participants` to use the full roster. Supply a client-generated UUID as `request_id` and reuse it with the same arguments, including `confirm`, if the start response is lost. Conflicting retries return `IDEMPOTENCY_CONFLICT`. Repeating the same confirmation decision is safe after reconnecting or restarting the server.
2. If the response status is `pending_confirmation`, show `estimatedUsd` and `reservedUsd` to the operator. Send `squad_contest_confirm` with the returned `runId` as `run_id` and the operator's decision as `confirm`. Start with `confirm: true` only when the operator has already authorized the spend. Squad still applies the daily cap.
3. When status is `running`, poll `squad_contest_status` every few seconds. The room advances the tournament while the server stays running. Stop polling at `complete`, `cancelled`, `expired`, or `failed`. For `complete`, call `squad_contest_result` with the same `run_id`.

Status reports work and provider attempts from durable rows, including in-flight calls with unresolved charges. `usage.costUsd` is the known ledger charge; `usage.unknownCalls` counts attempts whose charge is still unresolved. `reservedUsd` retains the original worst-case preflight estimate and is not the remaining reservation. An expired confirmation is released durably, so another tournament can start in the same server process.

## Example brief for a Supabase tournament

Use this as the `prompt` for `squad_tournament_start`. Leave `confirm` unset so the configured threshold governs approval.

> We entered a contest to build something unique and interesting that could spread widely through organic sharing, using Supabase. Propose the strongest product idea and explain its distinctive user experience, target audience, sharing loop and practical use of Supabase. Make a comprehensive build plan with an overview brief, milestones, phases and concrete line items. Include dependencies, owner roles, effort estimates, rollout and distribution experiments, and measurable success criteria. Define what done looks like for every phase and for the finished product: working demo, database migrations, authentication and row-level-security tests, relevant Supabase features, deployment, monitoring and acceptance evidence. Separate the MVP from later scope and explain how adoption and sharing will be measured.

Review the completed response's `winnerPlan` and rankings. The brief asks the entrants to draft a plan for review.
