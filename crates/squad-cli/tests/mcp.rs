use std::{path::Path, process::Stdio, time::Duration};

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::{sleep, timeout},
};
use wiremock::{
    Mock, Request, ResponseTemplate,
    matchers::{method, path},
};

#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::{FakeOpenRouter, MODEL};

const PROTOCOL_VERSION: &str = "2025-11-25";
const JUDGE: &str = "fixture/judge";
const REPLY: &str = "Use Supabase with a reviewed migration and a rollback drill.";
const TOOLS: [&str; 7] = [
    "squad_contest_cancel",
    "squad_contest_confirm",
    "squad_contest_result",
    "squad_contest_status",
    "squad_contests_list",
    "squad_roster",
    "squad_tournament_start",
];

fn squad(root: &Path, provider: &FakeOpenRouter) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
    command
        .env_clear()
        .env("HOME", root)
        .env("OPENROUTER_BASE_URL", provider.base_url())
        .current_dir(root)
        .args(["--data-dir", "room", "mcp"])
        .kill_on_drop(true);
    command
}

fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": {"name": "Squad MCP integration test", "version": "1"}
    })
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn initialized() -> Value {
    json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
}

fn rpc_result(message: Value, id: u64) -> Value {
    assert_eq!(message["jsonrpc"], "2.0", "{message}");
    assert_eq!(message["id"], id, "{message}");
    message
        .get("result")
        .unwrap_or_else(|| panic!("RPC failed: {message}"))
        .clone()
}

fn check_initialize(result: &Value) -> &str {
    assert_eq!(result["serverInfo"]["name"], "squad", "{result}");
    assert!(result["capabilities"]["tools"].is_object(), "{result}");
    result["protocolVersion"].as_str().unwrap()
}

fn check_tools(list: &Value) {
    let tools = list["tools"].as_array().unwrap();
    let mut names: Vec<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, TOOLS);
    let start = tools
        .iter()
        .find(|tool| tool["name"] == "squad_tournament_start")
        .unwrap();
    assert_eq!(start["inputSchema"]["required"], json!(["prompt"]));
    for name in ["prompt", "participants", "request_id", "confirm"] {
        assert!(
            start["inputSchema"]["properties"][name].is_object(),
            "{start}"
        );
    }
}

struct StdioMcp {
    child: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl StdioMcp {
    async fn start(command: &mut Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut session = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()).lines(),
            child,
            next_id: 0,
        };
        let result = session.call("initialize", initialize_params()).await;
        check_initialize(&result);
        session.write(initialized()).await;
        session
    }

    async fn write(&mut self, message: Value) {
        let line = format!("{message}\n");
        timeout(
            Duration::from_secs(5),
            self.input.write_all(line.as_bytes()),
        )
        .await
        .expect("timed out writing MCP request")
        .unwrap();
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.write(request(id, method, params)).await;
        timeout(Duration::from_secs(10), async {
            loop {
                let line = self
                    .output
                    .next_line()
                    .await
                    .unwrap()
                    .unwrap_or_else(|| panic!("Squad closed stdout during {method}"));
                let message: Value = serde_json::from_str(&line)
                    .unwrap_or_else(|_| panic!("Expected JSON-RPC on stdout, got {line:?}"));
                if message.get("id").is_some() {
                    return rpc_result(message, id);
                }
                assert_eq!(message["jsonrpc"], "2.0", "{message}");
                assert!(
                    message["method"]
                        .as_str()
                        .unwrap()
                        .starts_with("notifications/"),
                    "{message}"
                );
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {method}"))
    }

    async fn tool_response(&mut self, name: &str, arguments: Value) -> Value {
        self.call("tools/call", json!({"name": name, "arguments": arguments}))
            .await
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.tool_response(name, arguments).await;
        assert_ne!(result["isError"], true, "{name}: {result}");
        result["structuredContent"]
            .as_object()
            .expect("tool result must contain structuredContent");
        result["structuredContent"].clone()
    }

    async fn assert_tool_error(&mut self, name: &str, arguments: Value, code: &str) {
        let result = self.tool_response(name, arguments).await;
        assert_eq!(result["isError"], true, "{name}: {result}");
        assert_eq!(
            result["structuredContent"]["code"], code,
            "{name}: {result}"
        );
    }

    async fn finish(self) {
        let Self {
            mut child,
            input,
            mut output,
            ..
        } = self;
        drop(input);
        assert_eq!(
            timeout(Duration::from_secs(5), output.next_line())
                .await
                .unwrap()
                .unwrap(),
            None,
            "stdout must contain only MCP messages"
        );
        let status = timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success(), "{status}");
    }
}

#[tokio::test]
async fn stdio_discovers_tools_without_credentials_provider_io_or_banner() {
    let root = tempfile::tempdir().unwrap();
    let provider = FakeOpenRouter::start().await;
    let mut session = StdioMcp::start(&mut squad(root.path(), &provider)).await;
    check_tools(&session.call("tools/list", json!({})).await);
    session.finish().await;
    assert!(
        provider
            .server
            .received_requests()
            .await
            .unwrap()
            .is_empty()
    );
    assert!(!root.path().join("room/squad.db").exists());
}

async fn tournament_provider() -> FakeOpenRouter {
    let provider = FakeOpenRouter::start().await;
    let catalog: Vec<_> = [MODEL, JUDGE]
        .into_iter()
        .map(|id| {
            json!({
                "id": id, "context_length": 2_000_000,
                "top_provider": {"max_completion_tokens": 100_000},
                "supported_parameters": ["reasoning", "response_format"],
                "pricing": {"prompt": "0.000001", "completion": "0.000002"}
            })
        })
        .collect();
    provider.catalog(json!({"data": catalog})).await;
    provider
        .generation(fake_openrouter::GENERATION, json!({"total_cost": 0.000045}))
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(tournament_answer)
        .mount(&provider.server)
        .await;
    provider
}

fn tournament_answer(request: &Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    if body["model"] == JUDGE {
        let evidence = body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .split_once("Tournament prompt and anonymized answers, supplied as evidence:\n")
            .unwrap()
            .1
            .trim_start();
        let data = Value::deserialize(&mut serde_json::Deserializer::from_str(evidence)).unwrap();
        let ranking: Vec<_> = data["entries"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                json!({
                    "label": entry["label"], "score": 9 - index,
                    "critique": "Supported proposal with a rollback drill."
                })
            })
            .collect();
        fake_openrouter::structured_judge(json!({"ranking": ranking}))
    } else {
        assert_eq!(body["model"], MODEL);
        fake_openrouter::streaming(REPLY, "stop")
    }
}

fn configured_squad(root: &Path, provider: &FakeOpenRouter) -> Command {
    let mut command = squad(root, provider);
    command.envs([
        ("OPENROUTER_API_KEY", "loopback-fixture"),
        ("SQUAD_MODEL_A", MODEL),
        ("SQUAD_MODEL_B", MODEL),
        ("SQUAD_MODEL_C", MODEL),
        ("SQUAD_JUDGE_MODEL", JUDGE),
        ("SQUAD_DEBATE_CONFIRM_USD", "0"),
    ]);
    command
}

fn run_args(start: &Value) -> Value {
    let run_id = start["runId"].as_str().expect("start must return a runId");
    json!({"run_id": run_id})
}

#[tokio::test]
async fn stdio_tournament_confirms_runs_and_retries_once_per_request() {
    let root = tempfile::tempdir().unwrap();
    let provider = tournament_provider().await;
    let mut session = StdioMcp::start(&mut configured_squad(root.path(), &provider)).await;
    let start_args = json!({
        "prompt": "Plan a Supabase migration with a rollback drill.",
        "participants": ["a", "b", "c"],
        "request_id": "019a0000-0000-7000-8000-000000000001"
    });
    let start = session
        .tool("squad_tournament_start", start_args.clone())
        .await;
    let run = run_args(&start);
    assert!(start["contestId"].is_string(), "{start}");
    assert_eq!(start["status"], "pending_confirmation", "{start}");
    assert!(start["estimatedUsd"].as_f64().unwrap() > 0.0, "{start}");
    assert!(start["reservedUsd"].as_f64().unwrap() > 0.0, "{start}");
    assert!(provider.captured_bodies().await.is_empty());

    let duplicate = session
        .tool("squad_tournament_start", start_args.clone())
        .await;
    assert_eq!(duplicate["runId"], start["runId"]);
    assert_eq!(duplicate["contestId"], start["contestId"]);
    assert!(provider.captured_bodies().await.is_empty());

    let mut conflict = start_args.clone();
    conflict["prompt"] = json!("This is a different brief.");
    session
        .assert_tool_error("squad_tournament_start", conflict, "IDEMPOTENCY_CONFLICT")
        .await;
    let mut preconfirmed = start_args.clone();
    preconfirmed["confirm"] = json!(true);
    session
        .assert_tool_error(
            "squad_tournament_start",
            preconfirmed.clone(),
            "IDEMPOTENCY_CONFLICT",
        )
        .await;
    let approval = json!({"run_id": run["run_id"], "confirm": true});
    session
        .tool("squad_contest_confirm", approval.clone())
        .await;
    let status = timeout(Duration::from_secs(15), async {
        loop {
            let status = session.tool("squad_contest_status", run.clone()).await;
            if status["status"] == "complete" {
                break status;
            }
            assert_eq!(status["status"], "running", "{status}");
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("tournament never completed");
    assert_eq!(status["contestId"], start["contestId"]);
    assert_eq!(status["usage"]["calls"], 7, "{status}");
    assert_eq!(status["usage"]["unknownCalls"], 0, "{status}");
    let retry = session
        .tool("squad_contest_confirm", approval.clone())
        .await;
    assert_eq!(retry, status);
    let cancelled = session.tool("squad_contest_cancel", run.clone()).await;
    assert_eq!(cancelled, status);
    let result = session.tool("squad_contest_result", run.clone()).await;
    assert_eq!(result["winnerPlan"], REPLY, "{result}");
    assert!(
        matches!(result["champion"].as_str(), Some("a" | "b" | "c")),
        "{result}"
    );
    assert_eq!(
        result["placements"].as_array().unwrap().len(),
        3,
        "{result}"
    );
    assert!(
        !result.to_string().contains("PRIVATE REASONING"),
        "{result}"
    );

    let duplicate = session
        .tool("squad_tournament_start", start_args.clone())
        .await;
    assert_eq!(duplicate["runId"], start["runId"]);
    assert_eq!(duplicate["contestId"], start["contestId"]);
    let listed = session.tool("squad_contests_list", json!({})).await;
    let runs = listed["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1, "{listed}");
    assert_eq!(runs[0]["runId"], start["runId"]);
    assert_eq!(runs[0]["status"], "complete");

    session.finish().await;
    let mut reopened = StdioMcp::start(&mut configured_squad(root.path(), &provider)).await;
    let duplicate = reopened.tool("squad_tournament_start", start_args).await;
    assert_eq!(duplicate, status);
    reopened
        .assert_tool_error(
            "squad_tournament_start",
            preconfirmed,
            "IDEMPOTENCY_CONFLICT",
        )
        .await;
    let retry = reopened.tool("squad_contest_confirm", approval).await;
    assert_eq!(retry, status);
    reopened
        .assert_tool_error(
            "squad_contest_confirm",
            json!({"run_id": run["run_id"], "confirm": false}),
            "IDEMPOTENCY_CONFLICT",
        )
        .await;
    reopened.finish().await;

    assert_eq!(provider.captured_bodies().await.len(), 7);
    let db = rusqlite::Connection::open_with_flags(
        root.path().join("room/squad.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let attempts: i64 = db
        .query_row("SELECT count(*) FROM attempts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(attempts, 7);
    let ledger: (i64, i64, i64) = db
        .query_row(
            "SELECT SUM(spent_micros), SUM(reserved_micros), SUM(unknown_calls) FROM spend_daily",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(ledger, (315, 0, 0));
    let champions: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE kind = 'tournament.champion'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(champions, 1);
}

#[tokio::test]
async fn stdio_cancels_pending_tournament_without_provider_calls() {
    let root = tempfile::tempdir().unwrap();
    let provider = tournament_provider().await;
    let mut session = StdioMcp::start(&mut configured_squad(root.path(), &provider)).await;
    let start = session
        .tool(
            "squad_tournament_start",
            json!({
                "prompt": "Cancel this plan before any model is called.",
                "participants": ["a", "b", "c"]
            }),
        )
        .await;
    assert_eq!(start["status"], "pending_confirmation", "{start}");
    let run = run_args(&start);
    session.tool("squad_contest_cancel", run.clone()).await;
    let status = session.tool("squad_contest_status", run.clone()).await;
    assert_eq!(status["status"], "cancelled", "{status}");
    session.finish().await;

    let mut reopened = StdioMcp::start(&mut configured_squad(root.path(), &provider)).await;
    let retry = reopened
        .tool(
            "squad_contest_confirm",
            json!({"run_id": run["run_id"], "confirm": false}),
        )
        .await;
    assert_eq!(retry, status);
    reopened
        .assert_tool_error(
            "squad_contest_confirm",
            json!({"run_id": run["run_id"], "confirm": true}),
            "IDEMPOTENCY_CONFLICT",
        )
        .await;
    reopened.finish().await;
    assert!(provider.captured_bodies().await.is_empty());
}

#[tokio::test]
async fn stdio_expires_preflight_before_admitting_a_new_tournament_in_the_same_room() {
    let root = tempfile::tempdir().unwrap();
    let provider = tournament_provider().await;
    let mut session = StdioMcp::start(&mut configured_squad(root.path(), &provider)).await;
    let start_args = json!({
        "prompt": "This approval window will expire.",
        "request_id": "019a0000-0000-7000-8000-000000000002"
    });
    let start = session
        .tool("squad_tournament_start", start_args.clone())
        .await;
    assert_eq!(start["status"], "pending_confirmation", "{start}");
    let contest_id = start["contestId"].as_str().unwrap();
    let run = run_args(&start);

    // Backdate the estimate in both persisted copies to exercise the real
    // deadline without waiting for the production approval window.
    let mut db = rusqlite::Connection::open(root.path().join("room/squad.db")).unwrap();
    db.busy_timeout(Duration::from_secs(2)).unwrap();
    let tx = db.transaction().unwrap();
    let deadline = "2000-01-01T00:00:00Z";
    assert_eq!(
        tx.execute(
            "UPDATE contests SET expires_at=?1,plan_json=json_set(plan_json,'$.base.expiresAt',?1) WHERE contest_id=?2",
            [deadline, contest_id],
        )
        .unwrap(),
        1
    );
    assert_eq!(
        tx.execute(
            "UPDATE events SET payload=json_set(payload,'$.base.expiresAt',?1) WHERE kind='tournament.preflight' AND json_extract(payload,'$.base.contestId')=?2",
            [deadline, contest_id],
        )
        .unwrap(),
        1
    );
    tx.commit().unwrap();

    let fresh = session
        .tool(
            "squad_tournament_start",
            json!({"prompt": "Estimate a fresh plan."}),
        )
        .await;
    assert_eq!(fresh["status"], "pending_confirmation", "{fresh}");
    assert_ne!(fresh["runId"], start["runId"], "{fresh}");
    let expired = session.tool("squad_tournament_start", start_args).await;
    assert_eq!(expired["status"], "expired", "{expired}");
    assert_eq!(expired["contestId"], contest_id, "{expired}");
    session
        .assert_tool_error(
            "squad_contest_confirm",
            json!({"run_id": run["run_id"], "confirm": true}),
            "CONTEST_PLAN_EXPIRED",
        )
        .await;
    let durable_state: (bool, String) = db
        .query_row(
            "SELECT active,json_extract(state_json,'$.terminalReason') FROM contests WHERE contest_id=?",
            [contest_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(durable_state, (false, "expired".into()));
    let expiry_events: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind='contest.preflight.result' AND json_extract(payload,'$.contestId')=? AND json_extract(payload,'$.status')='expired'",
            [contest_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(expiry_events, 1);
    session.finish().await;
    assert!(provider.captured_bodies().await.is_empty());
}

#[tokio::test]
async fn stdio_preconfirmation_still_obeys_the_daily_cap() {
    let root = tempfile::tempdir().unwrap();
    let provider = tournament_provider().await;
    let mut command = configured_squad(root.path(), &provider);
    command.env("SQUAD_DAILY_MAX_USD", "0.000001");
    let mut session = StdioMcp::start(&mut command).await;
    let args = json!({
        "prompt": "Plan a Supabase migration.",
        "confirm": true,
        "request_id": "019a0000-0000-7000-8000-000000000003"
    });
    session
        .assert_tool_error("squad_tournament_start", args.clone(), "DAILY_MAX_REACHED")
        .await;
    session
        .assert_tool_error("squad_tournament_start", args.clone(), "DAILY_MAX_REACHED")
        .await;
    let mut changed = args;
    changed["confirm"] = json!(false);
    session
        .assert_tool_error("squad_tournament_start", changed, "IDEMPOTENCY_CONFLICT")
        .await;
    session.finish().await;
    assert!(provider.captured_bodies().await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn streamable_http_requires_bearer_for_discovery_and_calls() {
    use std::{io::Write, net::TcpListener, os::unix::fs::OpenOptionsExt};

    let root = tempfile::tempdir().unwrap();
    let provider = tournament_provider().await;
    let token = "squad-mcp-test-token";
    let token_file = root.path().join("mcp-token");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&token_file)
        .unwrap();
    writeln!(file, "{token}").unwrap();
    drop(file);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let endpoint = format!("http://{address}/mcp");
    let mut child = configured_squad(root.path(), &provider)
        .args(["--http", &address.to_string(), "--token-file"])
        .arg(token_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let post = || {
        client
            .post(&endpoint)
            .header("Accept", "application/json, text/event-stream")
    };
    let initialize = request(1, "initialize", initialize_params());
    let denied = timeout(Duration::from_secs(10), async {
        loop {
            match post().json(&initialize).send().await {
                Ok(response) => break response,
                Err(error) if error.is_connect() => sleep(Duration::from_millis(25)).await,
                Err(error) => panic!("MCP HTTP request failed: {error}"),
            }
        }
    })
    .await
    .expect("Squad never started listening");
    assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
    assert_eq!(denied.headers()["WWW-Authenticate"], "Bearer");

    let response = post()
        .bearer_auth(token)
        .json(&initialize)
        .send()
        .await
        .unwrap();
    let session_id = response.headers().get("Mcp-Session-Id").unwrap().clone();
    let result = rpc_result(http_message(response).await, 1);
    let version = check_initialize(&result);
    let session_post = || {
        post()
            .header("Mcp-Session-Id", session_id.clone())
            .header("MCP-Protocol-Version", version)
    };
    let notification = session_post()
        .bearer_auth(token)
        .json(&initialized())
        .send()
        .await
        .unwrap();
    assert_eq!(notification.status(), reqwest::StatusCode::ACCEPTED);
    let list = request(2, "tools/list", json!({}));
    let denied = session_post()
        .bearer_auth("incorrect-token")
        .json(&list)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
    let response = session_post()
        .bearer_auth(token)
        .json(&list)
        .send()
        .await
        .unwrap();
    check_tools(&rpc_result(http_message(response).await, 2));
    assert!(
        provider
            .server
            .received_requests()
            .await
            .unwrap()
            .is_empty()
    );

    let call = request(
        3,
        "tools/call",
        json!({
            "name": "squad_contests_list", "arguments": {}
        }),
    );
    let response = session_post()
        .bearer_auth(token)
        .json(&call)
        .send()
        .await
        .unwrap();
    let result = rpc_result(http_message(response).await, 3);
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["runs"], json!([]), "{result}");
    child.kill().await.unwrap();
    assert!(provider.captured_bodies().await.is_empty());
}

#[cfg(unix)]
async fn http_message(response: reqwest::Response) -> Value {
    let mut response = response.error_for_status().unwrap();
    let content_type = response.headers()["Content-Type"].to_str().unwrap();
    if content_type.starts_with("application/json") {
        return response.json().await.unwrap();
    }
    assert!(
        content_type.starts_with("text/event-stream"),
        "{content_type}"
    );
    let mut pending = Vec::new();
    while let Some(chunk) = response.chunk().await.unwrap() {
        pending.extend_from_slice(&chunk);
        while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = pending.drain(..=newline).collect();
            let line = std::str::from_utf8(&line).unwrap();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                let message: Value = serde_json::from_str(data).unwrap();
                if message.get("id").is_some() {
                    return message;
                }
            }
        }
    }
    panic!("MCP HTTP stream ended without a response");
}
