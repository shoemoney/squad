//! Extensible credential-free provider fixtures. wiremock handles normal routes;
//! a tiny loopback HTTP peer exercises real chunk fragmentation and broken bodies
//! (wiremock deliberately buffers response bodies).
// Each integration suite uses only the fixtures for its own execution paths.
#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{method, path, query_param},
};

pub const MODEL: &str = "fixture/reasoner-v1";
pub const GENERATION: &str = "gen-fixture-1";

pub struct FakeOpenRouter {
    pub server: MockServer,
}
impl FakeOpenRouter {
    pub async fn start() -> Self {
        Self {
            server: MockServer::start().await,
        }
    }
    pub fn base_url(&self) -> String {
        format!("{}/api/v1/", self.server.uri())
    }
    pub async fn catalog(&self, catalog: Value) {
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(catalog))
            .mount(&self.server)
            .await;
    }
    pub async fn generation(&self, id: &str, usage: Value) {
        Mock::given(method("GET"))
            .and(path("/api/v1/generation"))
            .and(query_param("id", id))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": usage})))
            .mount(&self.server)
            .await;
    }
    /// Ordered scripted calls; when exhausted, fail loudly rather than fabricate success.
    pub async fn completions(&self, responses: Vec<ResponseTemplate>) {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Script(Arc::new(Mutex::new(responses.into()))))
            .mount(&self.server)
            .await;
    }
    pub async fn captured_bodies(&self) -> Vec<Value> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method == "POST")
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect()
    }
}
#[derive(Clone)]
struct Script(Arc<Mutex<VecDeque<ResponseTemplate>>>);
impl Respond for Script {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| ResponseTemplate::new(418))
    }
}

pub fn catalog() -> Value {
    json!({"data": [{"id": MODEL, "name": "Fixture reasoner", "context_length": 32768,
        "top_provider": {"max_completion_tokens": 16384},
        "supported_parameters": ["reasoning", "response_format"],
        "pricing": {"prompt": "0.000001", "completion": "0.000002"}},
        {"id": "fixture/plain-v1", "context_length": 8192,
         "top_provider": {"max_completion_tokens": 4096}, "supported_parameters": [],
         "pricing": {"prompt": "0", "completion": "0"}}]})
}
pub fn usage() -> Value {
    json!({"prompt_tokens": 11, "completion_tokens": 17, "total_tokens": 28,
        "completion_tokens_details": {"reasoning_tokens": 12}, "cost": 0.000045})
}
pub fn event(value: Value) -> String {
    format!("data: {value}\r\n\r\n")
}
pub fn stream_body(text: &str, finish: &str) -> String {
    let mut body = String::from(": fixture keepalive\r\n\r\n");
    body.push_str(&event(json!({"id": GENERATION, "model": MODEL,
        "choices": [{"index": 0, "delta": {"reasoning": "PRIVATE REASONING", "reasoning_details": [{"text": "PRIVATE DETAILS"}]}, "finish_reason": null}]})));
    for part in text.chars() {
        body.push_str(&event(json!({"choices": [{"index": 0, "delta": {"content": part.to_string()}, "finish_reason": null}]})));
    }
    body.push_str(&event(
        json!({"choices": [{"index": 0, "delta": {}, "finish_reason": finish}]}),
    ));
    body.push_str(&event(json!({"choices": [], "usage": usage()})));
    body.push_str("data: [DONE]\r\n\r\n");
    body
}
pub fn streaming(text: &str, finish: &str) -> ResponseTemplate {
    raw_sse(stream_body(text, finish))
}
pub fn raw_sse(body: impl Into<String>) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}
pub fn malformed() -> ResponseTemplate {
    raw_sse("data: {invalid-json\n\n")
}
pub fn delayed(delay: Duration) -> ResponseTemplate {
    streaming("late", "stop").set_delay(delay)
}
pub fn disconnected() -> ResponseTemplate {
    raw_sse(event(json!({"id": GENERATION, "model": MODEL,
        "choices": [{"delta": {"content": "partial"}, "finish_reason": null}]})))
}
pub fn provider_error(code: u16) -> ResponseTemplate {
    raw_sse(event(
        json!({"error": {"code": code, "message": "fixture failure"}}),
    ))
}
pub fn rate_limited(retry_after: &str) -> ResponseTemplate {
    ResponseTemplate::new(429)
        .insert_header("Retry-After", retry_after)
        .set_body_json(json!({"error": {"code": 429}}))
}
pub fn alias_probe(model: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"id": GENERATION, "model": model,
        "choices": [{"message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}], "usage": usage()}))
}
pub fn structured_judge(verdict: Value) -> ResponseTemplate {
    streaming(&verdict.to_string(), "stop")
}
pub fn free_form_case(text: &str) -> ResponseTemplate {
    streaming(text, "stop")
}

/// Typed schemas belong to the contest engine: fixtures accept its serialized
/// output rather than maintaining a second schema here.
pub fn round_score(scores: Value) -> ResponseTemplate {
    structured_judge(scores)
}
pub fn tied_round_score(first: &str, second: &str) -> ResponseTemplate {
    let mut scores = serde_json::Map::new();
    for nick in [first, second] {
        scores.insert(
            nick.to_owned(),
            json!({"score": 7, "reason": "Equally strong supporting evidence."}),
        );
    }
    structured_judge(json!({"scores": scores}))
}
pub fn malformed_round_score() -> ResponseTemplate {
    streaming("{\"scores\": [99]}", "stop")
}
pub fn delayed_round_score(scores: Value, delay: Duration) -> ResponseTemplate {
    round_score(scores).set_delay(delay)
}
pub fn failed_round_score() -> ResponseTemplate {
    provider_error(503)
}
pub fn tournament_ranking(ranking: Value) -> ResponseTemplate {
    structured_judge(ranking)
}
pub fn repair_output(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn grill_questions(questions: &str) -> ResponseTemplate {
    streaming(questions, "stop")
}
pub fn grill_pass() -> ResponseTemplate {
    streaming("PASS", "stop")
}
pub fn grill_answer(answer: &str) -> ResponseTemplate {
    streaming(answer, "stop")
}
pub fn grill_rebuild(plan: &str) -> ResponseTemplate {
    streaming(plan, "stop")
}
pub fn missing_cost() -> ResponseTemplate {
    let mut body = stream_body("answer", "stop");
    let mut without_cost = usage();
    without_cost.as_object_mut().unwrap().remove("cost");
    body = body.replace(
        &event(json!({"choices": [], "usage": usage()})),
        &event(json!({"choices": [], "usage": without_cost})),
    );
    raw_sse(body)
}
/// Usage arrives before the failure; the accounting adapter must retain it.
pub fn billed_failure() -> ResponseTemplate {
    raw_sse(format!(
        "{}{}{}",
        event(json!({"id": GENERATION, "model": MODEL})),
        event(json!({"usage": usage()})),
        event(json!({"error": {"code": 503}}))
    ))
}
/// A generation exists and may be billed even if cancellation drops its body.
pub fn billed_cancellation(delay: Duration) -> ResponseTemplate {
    streaming("cancelled after dispatch", "stop").set_delay(delay)
}
pub fn delayed_cost(delay: Duration) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({"data": {"id": GENERATION, "total_cost": 0.000045}}))
        .set_delay(delay)
}
pub fn response_model_mismatch(model: &str) -> ResponseTemplate {
    raw_sse(stream_body("answer", "stop").replace(
        &format!("\"model\":\"{MODEL}\""),
        &format!("\"model\":\"{model}\""),
    ))
}
pub fn census_member(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn census_judge(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn crosscheck_member(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn crosscheck_judge(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn survey_member(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn survey_judge(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn probe_member(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn probe_judge(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn goal_extraction(output: Value) -> ResponseTemplate {
    structured_judge(output)
}
pub fn malformed_schema() -> ResponseTemplate {
    structured_judge(json!({"unexpected": true}))
}
pub fn delayed_mode(output: Value, delay: Duration) -> ResponseTemplate {
    structured_judge(output).set_delay(delay)
}
pub fn failed_member() -> ResponseTemplate {
    provider_error(503)
}
pub fn repair_exhaustion(attempts: usize) -> Vec<ResponseTemplate> {
    (0..attempts).map(|_| malformed_schema()).collect()
}

/// One-call raw peer, with real HTTP chunk boundaries, optional pause after each
/// fragment, and an intentionally truncated HTTP body when disconnect is true.
/// Drop aborts its task, so paused or cancelled tests cannot leak background work.
pub struct FragmentedPeer {
    pub base_url: String,
    task: JoinHandle<()>,
}
impl Drop for FragmentedPeer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl FragmentedPeer {
    pub async fn start(fragments: Vec<Vec<u8>>, pause: Duration, disconnect: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/api/v1/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Consume headers and request body before responding (avoid TCP reset).
            let mut received = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    return;
                }
                received.extend_from_slice(&buf[..n]);
                if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length:")
                                .and_then(|n| n.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if received.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.is_err() { return; }
            for fragment in fragments {
                if fragment.is_empty() {
                    continue;
                }
                if socket
                    .write_all(format!("{:x}\r\n", fragment.len()).as_bytes())
                    .await
                    .is_err()
                {
                    return;
                }
                if socket.write_all(&fragment).await.is_err() {
                    return;
                }
                if socket.write_all(b"\r\n").await.is_err() {
                    return;
                }
                tokio::time::sleep(pause).await;
            }
            if !disconnect {
                let _ = socket.write_all(b"0\r\n\r\n").await;
            }
        });
        Self { base_url, task }
    }
}
