mod support;
use futures::StreamExt;
use serde_json::json;
use squad_agents::openrouter::*;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use support::fake_openrouter as fake;
use tokio_util::sync::CancellationToken;
use wiremock::ResponseTemplate;

fn request() -> ChatRequest {
    ChatRequest {
        model: fake::MODEL.into(),
        messages: vec![ChatMessage {
            role: "user".into(),
            name: Some("me".into()),
            content: "hello".into(),
        }],
        visible_ceiling: 2048,
        published_output_limit: 16384,
        supports_reasoning: true,
        reasoning_effort: ReasoningEffort::Medium,
        reasoning_explicit: false,
        response_format: None,
        policy: ExecutionPolicy::SingleAttempt(DeadlineClass::Ordinary),
    }
}
fn client(fake: &fake::FakeOpenRouter) -> OpenRouterClient {
    OpenRouterClient::new(&fake.base_url(), "sentinel-key-never-log").unwrap()
}
fn visible(events: &[StreamEvent], target: u8) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Delta { attempt, text } if *attempt == target => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
fn complete(events: &[StreamEvent], target: u8) -> bool {
    events.contains(&StreamEvent::Complete { attempt: target })
}

#[tokio::test]
async fn finish_reasons_terminal_usage_identity_and_reasoning_exclusion() {
    for reason in [
        "stop",
        "length",
        "content_filter",
        "tool_calls",
        "provider_specific",
    ] {
        let fake = fake::FakeOpenRouter::start().await;
        fake.completions(vec![fake::streaming("héllo", reason)])
            .await;
        let events = client(&fake)
            .stream(request(), CancellationToken::new())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(visible(&events, 1), "héllo");
        assert!(complete(&events, 1));
        assert!(events.contains(&StreamEvent::Finish {
            attempt: 1,
            reason: reason.into()
        }));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Metadata { generation_id: Some(id), response_model: Some(model), .. } if id == fake::GENERATION && model == fake::MODEL)));
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Usage { usage, .. } if usage.completion_tokens == Some(17) && usage.cost == Some(0.000045) && usage.completion_tokens_details.as_ref().unwrap().reasoning_tokens == Some(12))));
        assert!(!format!("{events:?}").contains("PRIVATE"));
        let bodies = fake.captured_bodies().await;
        assert_eq!(bodies[0]["model"], fake::MODEL);
        assert_eq!(bodies[0]["max_tokens"], 6144);
        assert_eq!(bodies[0]["reasoning"], json!({"max_tokens": 4096}));
        assert_eq!(bodies[0]["stream_options"]["include_usage"], true);
    }
}

#[test]
fn request_reserves_reasoning_clamps_visible_and_rejects_invalid_parameters() {
    let mut req = request();
    req.published_output_limit = 4500;
    assert_eq!(req.body().unwrap()["max_tokens"], 4500);
    req.published_output_limit = 4096;
    assert_eq!(req.body().unwrap_err(), TransportError::OutputLimit);
    req.published_output_limit = 16384;
    req.supports_reasoning = false;
    assert_eq!(req.body().unwrap()["max_tokens"], 2048);
    assert!(req.body().unwrap().get("reasoning").is_none());
    req.reasoning_explicit = true;
    assert_eq!(
        req.body().unwrap_err(),
        TransportError::UnsupportedReasoning
    );
    req.reasoning_explicit = false;
    req.model = "~fixture/latest".into();
    assert_eq!(req.body().unwrap_err(), TransportError::InvalidModel);
    req = request();
    for (effort, tokens) in [(ReasoningEffort::Low, 1024), (ReasoningEffort::High, 8192)] {
        req.reasoning_effort = effort;
        assert_eq!(req.body().unwrap()["reasoning"]["max_tokens"], tokens);
    }
}

#[tokio::test]
async fn actual_http_fragmentation_and_broken_chunked_body() {
    let body = fake::stream_body("café", "stop");
    let peer = fake::FragmentedPeer::start(
        body.as_bytes().chunks(3).map(|c| c.to_vec()).collect(),
        Duration::ZERO,
        false,
    )
    .await;
    let events = OpenRouterClient::new(&peer.base_url, "fake")
        .unwrap()
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(visible(&events, 1), "café");
    assert!(complete(&events, 1));
    let body = fake::event(json!({"choices": [{"delta": {"content": "partial"}}]}));
    let peer = fake::FragmentedPeer::start(vec![body.into_bytes()], Duration::ZERO, true).await;
    let events = OpenRouterClient::new(&peer.base_url, "fake")
        .unwrap()
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(visible(&events, 1), "partial");
    assert!(!complete(&events, 1));
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::AttemptFailed {
            error: TransportError::Network,
            ..
        }
    )));
}

#[tokio::test]
async fn ordinary_retry_replaces_partial_output_and_is_bounded() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![
        fake::disconnected(),
        fake::streaming("replacement", "stop"),
    ])
    .await;
    let mut req = request();
    req.policy = ExecutionPolicy::OrdinaryReply;
    let events = client(&fake)
        .stream(req.clone(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(visible(&events, 1), "partial");
    assert_eq!(visible(&events, 2), "replacement");
    assert!(complete(&events, 2));
    assert_eq!(fake.captured_bodies().await.len(), 2);
    assert_eq!(
        fake.captured_bodies().await[0],
        fake.captured_bodies().await[1]
    );
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![
        ResponseTemplate::new(503),
        ResponseTemplate::new(503),
        fake::streaming("third", "stop"),
    ])
    .await;
    let events = client(&fake)
        .stream(req, CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(!complete(&events, 2));
    assert_eq!(fake.captured_bodies().await.len(), 2);
}

#[tokio::test]
async fn malformed_midstream_errors_and_terminal_4xx_do_not_complete() {
    for response in [
        fake::malformed(),
        fake::provider_error(400),
        ResponseTemplate::new(401),
        raw_midstream_error(),
    ] {
        let fake = fake::FakeOpenRouter::start().await;
        fake.completions(vec![response]).await;
        let events = client(&fake)
            .stream(request(), CancellationToken::new())
            .collect::<Vec<_>>()
            .await;
        assert!(!complete(&events, 1));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::AttemptFailed { .. }))
        );
        assert_eq!(fake.captured_bodies().await.len(), 1);
    }
}
fn raw_midstream_error() -> ResponseTemplate {
    fake::raw_sse(
        fake::event(json!({"choices": [{"delta": {"content": "partial"}}]}))
            + &fake::event(json!({"error": {"code": 503, "message": "sentinel-key-never-log"}})),
    )
}

#[tokio::test]
async fn single_attempt_engine_policy_never_retries() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![
        ResponseTemplate::new(503),
        fake::streaming("forbidden", "stop"),
    ])
    .await;
    let events = client(&fake)
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(!complete(&events, 1));
    assert_eq!(fake.captured_bodies().await.len(), 1);
}

#[tokio::test]
async fn retry_after_is_honored_and_wait_exceeding_total_is_rejected() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![fake::rate_limited("1"), fake::streaming("ok", "stop")])
        .await;
    let mut req = request();
    req.policy = ExecutionPolicy::OrdinaryReply;
    let started = tokio::time::Instant::now();
    let events = client(&fake)
        .stream(req.clone(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(complete(&events, 2));
    assert!(started.elapsed() >= Duration::from_secs(1));
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![fake::rate_limited("301")]).await;
    let events = client(&fake)
        .stream(req, CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(events.contains(&StreamEvent::AttemptFailed {
        attempt: 1,
        error: TransportError::Deadline
    }));
    assert_eq!(fake.captured_bodies().await.len(), 1);
}

#[tokio::test]
async fn cancellation_before_dispatch_during_headers_stream_and_retry_wait() {
    let fake = fake::FakeOpenRouter::start().await;
    let token = CancellationToken::new();
    token.cancel();
    let events = client(&fake)
        .stream(request(), token)
        .collect::<Vec<_>>()
        .await;
    assert!(events.contains(&StreamEvent::AttemptFailed {
        attempt: 1,
        error: TransportError::Cancelled
    }));
    assert!(fake.captured_bodies().await.is_empty());
    fake.completions(vec![fake::delayed(Duration::from_secs(60))])
        .await;
    assert_cancelled(client(&fake), request()).await;
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![fake::rate_limited("30")]).await;
    let mut req = request();
    req.policy = ExecutionPolicy::OrdinaryReply;
    assert_cancelled(client(&fake), req).await;
    assert_eq!(fake.captured_bodies().await.len(), 1);
    let body = fake::event(json!({"choices": [{"delta": {"content": "partial"}}]}));
    let peer =
        fake::FragmentedPeer::start(vec![body.into_bytes()], Duration::from_secs(60), false).await;
    assert_cancelled(
        OpenRouterClient::new(&peer.base_url, "fake").unwrap(),
        request(),
    )
    .await;
}
async fn assert_cancelled(client: OpenRouterClient, req: ChatRequest) {
    let token = CancellationToken::new();
    let cancel = token.clone();
    let task = tokio::spawn(async move { client.stream(req, token).collect::<Vec<_>>().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    let events = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::AttemptFailed {
            error: TransportError::Cancelled,
            ..
        }
    )));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::Complete { .. }))
    );
}

#[tokio::test]
async fn catalog_alias_generation_and_structured_freeform_fixtures() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.catalog(fake::catalog()).await;
    fake.generation(
        fake::GENERATION,
        json!({"total_cost": 0.2, "tokens_prompt": 10}),
    )
    .await;
    fake.completions(vec![
        ResponseTemplate::new(400),
        fake::alias_probe(fake::MODEL),
        fake::structured_judge(json!({"winner": "a"})),
        fake::free_form_case("a free form ruling"),
    ])
    .await;
    let http = reqwest::Client::new();
    let catalog: serde_json::Value = http
        .get(format!("{}models", fake.base_url()))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(catalog["data"][0]["id"], fake::MODEL);
    for cap in [16, 2048] {
        let response = http
            .post(format!("{}chat/completions", fake.base_url()))
            .json(&json!({"model": "~fixture/latest", "max_tokens": cap}))
            .send()
            .await
            .unwrap();
        if cap == 16 {
            assert_eq!(response.status(), 400);
        } else {
            assert_eq!(
                response.json::<serde_json::Value>().await.unwrap()["model"],
                fake::MODEL
            );
        }
    }
    let usage: serde_json::Value = http
        .get(format!(
            "{}generation?id={}",
            fake.base_url(),
            fake::GENERATION
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(usage["data"]["total_cost"], 0.2);
    let mut req = request();
    req.response_format = Some(
        json!({"type": "json_schema", "json_schema": {"name": "verdict", "strict": true, "schema": {"type": "object"}}}),
    );
    let events = client(&fake)
        .stream(req.clone(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(visible(&events, 1), "{\"winner\":\"a\"}");
    assert_eq!(
        fake.captured_bodies().await[2]["response_format"],
        req.response_format.unwrap()
    );
    let events = client(&fake)
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(visible(&events, 1), "a free form ruling");
}

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[tokio::test(flavor = "current_thread")]
async fn api_key_never_appears_in_debug_errors_or_logs() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![raw_midstream_error()]).await;
    let client = client(&fake);
    assert!(!format!("{client:?}").contains("sentinel-key-never-log"));
    let capture = Capture(Arc::new(Mutex::new(Vec::new())));
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let events = client
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(!format!("{events:?}").contains("sentinel-key-never-log"));
    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        !logs.contains("sentinel-key-never-log"),
        "credential appeared in captured trace logs"
    );
}

#[tokio::test]
async fn constant_first_token_and_idle_deadlines() {
    // Establish the delayed HTTP request before pausing the clock. No real wait
    // for a 90-second timer is needed, and no server readiness depends on time.
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![fake::delayed(Duration::from_secs(120))])
        .await;
    let client = client(&fake);
    let task = tokio::spawn(async move {
        client
            .stream(request(), CancellationToken::new())
            .collect::<Vec<_>>()
            .await
    });
    while fake.captured_bodies().await.is_empty() {
        tokio::task::yield_now().await;
    }
    tokio::time::pause();
    tokio::time::advance(Duration::from_millis(
        squad_protocol::constants::FIRST_TOKEN_TIMEOUT_MS + 1,
    ))
    .await;
    let events = task.await.unwrap();
    assert!(events.contains(&StreamEvent::AttemptFailed {
        attempt: 1,
        error: TransportError::FirstTokenTimeout
    }));
    tokio::time::resume();

    let body = fake::event(json!({"choices": [{"delta": {"content": "first"}}]}));
    let peer =
        fake::FragmentedPeer::start(vec![body.into_bytes()], Duration::from_secs(60), false).await;
    let client = OpenRouterClient::new(&peer.base_url, "fake").unwrap();
    let mut stream = client.stream(request(), CancellationToken::new());
    while !matches!(stream.next().await.unwrap(), StreamEvent::Delta { .. }) {}
    tokio::time::pause();
    let task = tokio::spawn(async move { stream.collect::<Vec<_>>().await });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(
        squad_protocol::constants::IDLE_TIMEOUT_MS + 1,
    ))
    .await;
    let events = task.await.unwrap();
    assert!(events.contains(&StreamEvent::AttemptFailed {
        attempt: 1,
        error: TransportError::IdleTimeout
    }));
    tokio::time::resume();
}

#[tokio::test]
async fn missing_usage_is_unknown_and_response_model_is_not_rewritten() {
    let fake = fake::FakeOpenRouter::start().await;
    let body = fake::event(json!({"id": "gen-no-usage", "model": "fixture/different",
        "choices": [{"delta": {"content": "ok"}, "finish_reason": "stop"}]}))
        + "data: [DONE]\n\n";
    fake.completions(vec![fake::raw_sse(body)]).await;
    let events = client(&fake)
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(complete(&events, 1));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage { .. }))
    );
    assert!(events.iter().any(|e| matches!(e, StreamEvent::Metadata { response_model: Some(model), .. } if model == "fixture/different")));
    assert_eq!(fake.captured_bodies().await[0]["model"], fake::MODEL);
}

#[tokio::test]
async fn named_error_events_and_ordinary_terminal_errors_are_not_retried() {
    for response in [
        ResponseTemplate::new(403),
        fake::provider_error(400),
        fake::raw_sse(
            "event: error\ndata: {\"code\":401,\"message\":\"sentinel-key-never-log\"}\n\n",
        ),
    ] {
        let fake = fake::FakeOpenRouter::start().await;
        fake.completions(vec![response, fake::streaming("must not dispatch", "stop")])
            .await;
        let mut req = request();
        req.policy = ExecutionPolicy::OrdinaryReply;
        let events = client(&fake)
            .stream(req, CancellationToken::new())
            .collect::<Vec<_>>()
            .await;
        assert_eq!(fake.captured_bodies().await.len(), 1);
        assert!(!complete(&events, 1));
        assert!(!format!("{events:?}").contains("sentinel-key-never-log"));
    }
}

#[tokio::test]
async fn reasoning_budget_obeys_openrouter_mutually_exclusive_fields() {
    use wiremock::{
        Mock, Request,
        matchers::{method, path},
    };

    let fake = fake::FakeOpenRouter::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(|request: &Request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let reasoning = &body["reasoning"];
            if reasoning.get("effort").is_some() && reasoning.get("max_tokens").is_some() {
                ResponseTemplate::new(400).set_body_json(json!({"error": {
                    "code": 400,
                    "message": "Only one of reasoning.effort and reasoning.max_tokens can be specified"
                }}))
            } else {
                fake::streaming("OK", "stop")
            }
        })
        .expect(1)
        .mount(&fake.server)
        .await;
    let events = client(&fake)
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(complete(&events, 1), "{events:?}");
    assert_eq!(visible(&events, 1), "OK");
    let bodies = fake.captured_bodies().await;
    assert_eq!(bodies[0]["reasoning"], json!({"max_tokens": 4096}));
    assert_eq!(bodies[0]["max_tokens"], 6144);
}

struct CountRecovery(Arc<Mutex<u32>>);
impl ModelRecovery for CountRecovery {
    fn recover(
        &self,
        _client: OpenRouterClient,
        _model: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        *self.0.lock().unwrap() += 1;
        Box::pin(async {})
    }
}
#[tokio::test]
async fn openrouters_404_with_a_json_body_still_triggers_alias_repair() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![ResponseTemplate::new(404).set_body_json(
        json!({"error": {"message": "Upstream hiccup", "code": 404}}),
    )])
    .await;
    let calls = Arc::new(Mutex::new(0));
    let events = client(&fake)
        .with_model_recovery(Arc::new(CountRecovery(calls.clone())))
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::AttemptFailed { error, .. } if error.http_status() == Some(404)
    )));
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn empty_answers_fail_after_usage_with_safe_classification() {
    for (delta, finish, kind) in [
        (json!({"content": ""}), "stop", EmptyResponseKind::NoContent),
        (
            json!({"content": " \n"}),
            "stop",
            EmptyResponseKind::NoContent,
        ),
        (
            json!({"reasoning": "PRIVATE", "reasoning_details": [{"text": "PRIVATE"}]}),
            "stop",
            EmptyResponseKind::ReasoningOnly,
        ),
        (
            json!({"content": null, "reasoning": "PRIVATE"}),
            "length",
            EmptyResponseKind::ReasoningTruncated,
        ),
        (
            json!({"content": null}),
            "length",
            EmptyResponseKind::Truncated,
        ),
        (
            json!({"content": [{"type": "thinking", "thinking": "PRIVATE"}]}),
            "stop",
            EmptyResponseKind::ReasoningOnly,
        ),
        (
            json!({"refusal": "PRIVATE"}),
            "content_filter",
            EmptyResponseKind::Refusal,
        ),
        (
            json!({"tool_calls": [{"function": {"arguments": "PRIVATE"}}]}),
            "tool_calls",
            EmptyResponseKind::ToolCall,
        ),
        (json!({}), "error", EmptyResponseKind::ProviderError),
    ] {
        let fake = fake::FakeOpenRouter::start().await;
        let body = fake::event(
            json!({"id": "gen-empty", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}),
        ) + &fake::event(
            json!({"choices": [], "usage": {"completion_tokens": 100, "cost": 0.001}}),
        ) + "data: [DONE]\n\n";
        fake.completions(vec![fake::raw_sse(body)]).await;
        let events = client(&fake)
            .stream(request(), CancellationToken::new())
            .collect::<Vec<_>>()
            .await;
        assert!(!complete(&events, 1));
        assert_eq!(
            events.last(),
            Some(&StreamEvent::AttemptFailed {
                attempt: 1,
                error: TransportError::EmptyResponse(kind)
            })
        );
        assert!(
            events.iter().any(
                |e| matches!(e, StreamEvent::Usage { usage, .. } if usage.cost == Some(0.001))
            )
        );
        assert!(!format!("{events:?}").contains("PRIVATE"));
        assert_eq!(fake.captured_bodies().await.len(), 1);
        assert_eq!(
            TransportError::EmptyResponse(kind).retryable(),
            matches!(
                kind,
                EmptyResponseKind::NoContent
                    | EmptyResponseKind::ReasoningOnly
                    | EmptyResponseKind::Truncated
                    | EmptyResponseKind::ReasoningTruncated
            )
        );
    }
}

#[tokio::test]
async fn typed_content_parts_extract_answer_in_order_without_private_thinking() {
    let fake = fake::FakeOpenRouter::start().await;
    let body =
        fake::event(json!({"choices": [{"index": 1, "delta": {"content": "wrong choice"}}]}))
            + &fake::event(json!({"choices": [{"index": 0, "delta": {"content": [
            {"type": "thinking", "thinking": "PRIVATE"},
            {"type": "text", "text": "Hello "},
            {"type": "reasoning", "text": "PRIVATE"},
            {"type": "output_text", "text": "world"},
            {"type": "unknown", "text": "PRIVATE"}
        ]}, "finish_reason": "stop"}]}))
            + "data: [DONE]\n\n";
    fake.completions(vec![fake::raw_sse(body)]).await;
    let events = client(&fake)
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert_eq!(visible(&events, 1), "Hello world");
    assert!(complete(&events, 1));
    assert!(!format!("{events:?}").contains("PRIVATE"));
}

#[tokio::test]
async fn ordinary_empty_response_retries_with_both_attempts_usage_retained() {
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![
        fake::streaming("", "length"),
        fake::streaming("recovered", "stop"),
    ])
    .await;
    let mut req = request();
    req.policy = ExecutionPolicy::OrdinaryReply;
    let events = client(&fake)
        .stream(req, CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(!complete(&events, 1));
    assert!(complete(&events, 2));
    assert_eq!(visible(&events, 2), "recovered");
    for target in [1, 2] {
        assert!(events.iter().any(|e| matches!(e, StreamEvent::Usage { attempt, usage } if *attempt == target && usage.cost == Some(0.000045))));
    }
    assert_eq!(fake.captured_bodies().await.len(), 2);
}

#[tokio::test]
async fn captured_mistral_reasoning_only_stream_is_a_billed_truncation() {
    let response: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/mistral-empty.json")).unwrap();
    assert!(response["choices"][0]["message"]["content"].is_null());
    assert_eq!(response["choices"][0]["finish_reason"], "length");
    assert_eq!(
        response["usage"]["completion_tokens_details"]["reasoning_tokens"],
        511
    );
    let fake = fake::FakeOpenRouter::start().await;
    fake.completions(vec![fake::raw_sse(include_str!(
        "fixtures/mistral-empty.sse"
    ))])
    .await;
    let events = client(&fake)
        .stream(request(), CancellationToken::new())
        .collect::<Vec<_>>()
        .await;
    assert!(visible(&events, 1).is_empty());
    assert!(!complete(&events, 1));
    assert_eq!(
        events.last(),
        Some(&StreamEvent::AttemptFailed {
            attempt: 1,
            error: TransportError::EmptyResponse(EmptyResponseKind::ReasoningTruncated)
        })
    );
    assert!(events.iter().any(|e| matches!(e, StreamEvent::Usage { usage, .. } if usage.cost == Some(0.00142534) && usage.completion_tokens == Some(512))));
    assert!(!format!("{events:?}").contains("SCRUBBED THINKING"));
}
