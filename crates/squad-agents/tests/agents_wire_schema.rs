//! Regression: live OpenRouter provider 400s on the response schema (2026-11-25).
//! The fake server applies each vendor's published schema rules and answers with
//! the exact error bodies captured from the live routes.
use futures::StreamExt;
use serde_json::{Value, json};
use squad_agents::openrouter::*;
use squad_protocol::*;
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

fn anthropic_400(message: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({"error":{"message":"Provider returned error","code":400,
        "metadata":{"raw":json!({"type":"error","error":{"type":"invalid_request_error","message":format!("output_config.format.schema: {message}")}}).to_string(),
        "provider_name":"Anthropic"}}}))
}

fn walk(schema: &Value, f: &mut dyn FnMut(&Value, &str)) {
    match schema {
        Value::Object(o) => {
            for (k, v) in o {
                f(v, k);
                if matches!(k.as_str(), "properties" | "$defs") {
                    for (_, s) in v.as_object().into_iter().flatten() {
                        walk(s, f);
                    }
                } else {
                    walk(v, f);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|v| walk(v, f)),
        _ => {}
    }
}

/// Anthropic rejects these (live: `Schema keyword 'not' is not supported`,
/// `property 'patternProperties' is not supported`, `'x-required-nullable' is not
/// supported`, `Schema type is missing for schema: {'properties': {'kind': {'enum': ...`).
/// Gemini rejects bound-heavy schemas (`constraint that has too many states for serving`).
/// OpenAI strict rejects `allOf`.
fn vendor_verdict(request: &Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    let format = &body["response_format"]["json_schema"];
    let schema = &format["schema"];
    let mut bad = None;
    walk(schema, &mut |value, key| {
        let reason = match key {
            "not" | "patternProperties" | "allOf" | "if" | "then" | "pattern" | "format"
            | "minimum" | "maximum" | "minItems" | "maxItems" | "minLength" | "maxLength"
            | "$schema" | "title" => Some(key.to_string()),
            k if k.starts_with("x-") => Some(k.to_string()),
            "enum" | "const" => None,
            _ => None,
        };
        if bad.is_none() && reason.is_some() {
            bad = reason;
        }
        if let Value::Object(o) = value
            && (o.contains_key("enum") || o.contains_key("const"))
            && !o.contains_key("type")
            && bad.is_none()
        {
            bad = Some("typeless enum".to_string());
        }
    });
    if let Some(bad) = bad {
        return anthropic_400(&format!("Unsupported schema construct: {bad}"));
    }
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(
            "data: {\"id\":\"gen-1\",\"model\":\"anthropic/claude-haiku-5.5\",\"choices\":[{\"delta\":{\"content\":\"{}\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"gen-1\",\"model\":\"anthropic/claude-haiku-5.5\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2,\"cost\":0.0001}}\n\ndata: [DONE]\n\n",
        )
}

fn request(model: &str, schema: Value) -> ChatRequest {
    ChatRequest {
        model: model.into(),
        messages: vec![ChatMessage {
            role: "user".into(),
            name: Some("squad".into()),
            content: "go".into(),
        }],
        visible_ceiling: 2048,
        published_output_limit: 16384,
        supports_reasoning: false,
        reasoning_effort: ReasoningEffort::Low,
        reasoning_explicit: false,
        response_format: Some(
            json!({"type":"json_schema","json_schema":{"name":"x","strict":true,"schema":schema}}),
        ),
        policy: ExecutionPolicy::SingleAttempt(DeadlineClass::Ordinary),
    }
}

#[tokio::test]
async fn every_mode_schema_is_accepted_by_vendor_schema_rules() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(vendor_verdict)
        .mount(&server)
        .await;
    let client = OpenRouterClient::new(&server.uri(), "sentinel-key-never-log").unwrap();
    let schemas: Vec<(&str, Value)> = vec![
        ("census hunt", validation::schema::<CensusHuntOutput>()),
        ("census merge", validation::schema::<CensusMergeOutput>()),
        ("census rate", validation::schema::<CensusRatingOutput>()),
        ("census final", validation::schema::<CensusFinalOutput>()),
        ("survey extract", validation::schema::<SurveyExtraction>()),
        ("survey ballot", validation::schema::<SurveyBallot>()),
        ("survey report", validation::schema::<SurveyReportText>()),
        ("probe report", validation::schema::<ProbeReport>()),
        ("probe check", validation::schema::<ProbeCheck>()),
        (
            "crosscheck review",
            validation::schema::<CrosscheckReviewResponse>(),
        ),
        (
            "crosscheck audit",
            validation::schema::<CrosscheckAuditResponse>(),
        ),
        (
            "crosscheck ruling",
            validation::schema::<CrosscheckRuling>(),
        ),
        ("judge verdict", validation::schema::<JudgeVerdict>()),
        ("round scores", validation::schema::<RoundJudgeResponse>()),
    ];
    for (name, schema) in schemas {
        let events = client
            .stream(
                request("anthropic/claude-haiku-5.5", schema),
                CancellationToken::new(),
            )
            .collect::<Vec<_>>()
            .await;
        assert!(
            events.contains(&StreamEvent::Complete { attempt: 1 }),
            "{name}: provider rejected the schema: {events:?}"
        );
    }
}

#[tokio::test]
async fn the_unsanitized_schema_would_have_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(vendor_verdict)
        .mount(&server)
        .await;
    let raw = validation::schema::<JudgeVerdict>();
    let response = reqwest::Client::new()
        .post(format!("{}/chat/completions", server.uri()))
        .json(&json!({"response_format":{"type":"json_schema","json_schema":{"schema":raw}}}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}

#[test]
fn map_schemas_leave_openai_strict_mode() {
    let body = request("openai/gpt-5-nano", validation::schema::<ProbeReport>())
        .body()
        .unwrap();
    assert_eq!(body["response_format"]["json_schema"]["strict"], false);
    let body = request(
        "openai/gpt-5-nano",
        validation::schema::<CensusHuntOutput>(),
    )
    .body()
    .unwrap();
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
}

#[test]
fn openai_and_xai_reasoning_uses_effort_not_a_token_budget() {
    for (model, effort) in [
        ("openai/gpt-5-nano", "medium"),
        ("x-ai/grok-4.20", "medium"),
    ] {
        let mut req = request(model, json!({"type": "object"}));
        req.supports_reasoning = true;
        req.reasoning_effort = ReasoningEffort::Medium;
        let body = req.body().unwrap();
        assert_eq!(body["reasoning"], json!({"effort": effort}), "{model}");
        assert_eq!(body["max_tokens"], 2048 + 4096);
    }
    let mut req = request("anthropic/claude-haiku-5.5", json!({"type": "object"}));
    req.supports_reasoning = true;
    req.reasoning_effort = ReasoningEffort::Medium;
    assert_eq!(
        req.body().unwrap()["reasoning"],
        json!({"max_tokens": 4096})
    );
}
