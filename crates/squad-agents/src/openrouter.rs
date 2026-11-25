//! Credential-contained OpenRouter transport, independent of Work and ingress.
//!
//! Events are provisional and scoped to a provider-call attempt. An ordinary retry
//! replaces previous provisional output. The caller owns durable admission,
//! accounting, final-result validation and acknowledgement. Use `SingleAttempt`
//! for engine-owned work; only ordinary replies may use `OrdinaryReply`.

mod sse;

use futures::{Stream, StreamExt};
use reqwest::{
    Client,
    header::{HeaderValue, RETRY_AFTER},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use squad_protocol::constants as limits;
use std::{
    fmt,
    pin::Pin,
    time::{Duration, SystemTime},
};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

/// A provider message already assembled by the context/prompt layer.
#[derive(Clone, Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: String,
}

pub use squad_protocol::config::ReasoningEffort;
trait ReasoningReservation {
    fn reservation(self) -> (&'static str, u32);
}
impl ReasoningReservation for ReasoningEffort {
    fn reservation(self) -> (&'static str, u32) {
        match self {
            Self::Low => ("low", limits::REASONING_LOW_TOKENS),
            Self::Medium => ("medium", limits::REASONING_MEDIUM_TOKENS),
            Self::High => ("high", limits::REASONING_HIGH_TOKENS),
        }
    }
}

/// Retry authorization and deadline class; these are not environment settings.
#[derive(Clone, Copy, Debug)]
pub enum ExecutionPolicy {
    OrdinaryReply,
    SingleAttempt(DeadlineClass),
}
#[derive(Clone, Copy, Debug)]
pub enum DeadlineClass {
    Ordinary,
    DebateTurn,
    FinalJudge,
    RoundJudge,
    TournamentJudge,
}
impl DeadlineClass {
    fn duration(self) -> Duration {
        Duration::from_millis(match self {
            Self::Ordinary => limits::TOTAL_TIMEOUT_MS,
            Self::DebateTurn => limits::DEBATE_TURN_DEADLINE_MS,
            Self::FinalJudge => limits::FINAL_JUDGE_DEADLINE_MS,
            Self::RoundJudge => limits::ROUND_JUDGE_DEADLINE_MS,
            Self::TournamentJudge => limits::TOURNAMENT_JUDGE_DEADLINE_MS,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// Owning work kind's canonical visible ceiling.
    pub visible_ceiling: u32,
    pub published_output_limit: u32,
    pub supports_reasoning: bool,
    pub reasoning_effort: ReasoningEffort,
    pub reasoning_explicit: bool,
    pub response_format: Option<Value>,
    pub policy: ExecutionPolicy,
}
impl ChatRequest {
    /// Reserve reasoning first, then clamp visible output to the remaining limit.
    pub fn body(&self) -> Result<Value, TransportError> {
        if self.model.is_empty() || self.model.starts_with('~') || !self.model.contains('/') {
            return Err(TransportError::InvalidModel);
        }
        if !self.supports_reasoning && self.reasoning_explicit {
            return Err(TransportError::UnsupportedReasoning);
        }
        let (_, reservation) = self.reasoning_effort.reservation();
        let reservation = if self.supports_reasoning {
            reservation
        } else {
            0
        };
        let remaining = self
            .published_output_limit
            .checked_sub(reservation)
            .filter(|n| *n > 0)
            .ok_or(TransportError::OutputLimit)?;
        let visible = self.visible_ceiling.min(remaining);
        if visible == 0 || self.visible_ceiling > limits::FINAL_JUDGE_VISIBLE_TOKENS {
            return Err(TransportError::OutputLimit);
        }
        let mut body = json!({"model": self.model, "messages": self.messages,
            "stream": true, "stream_options": {"include_usage": true},
            "max_tokens": visible + reservation});
        if self.supports_reasoning {
            // OpenRouter rejects effort together with a token budget. Keep the
            // explicit budget so provider limits match admission reservations.
            body["reasoning"] = json!({"max_tokens": reservation});
        }
        if let Some(format) = &self.response_format {
            body["response_format"] = format.clone();
        }
        Ok(body)
    }
}

/// Provider totals, with unknown values retained as None. Reasoning is already
/// included in completion_tokens and must never be added to it again.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cost: Option<f64>,
    pub completion_tokens_details: Option<CompletionTokenDetails>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompletionTokenDetails {
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    AttemptStarted {
        attempt: u8,
    },
    Metadata {
        attempt: u8,
        generation_id: Option<String>,
        response_model: Option<String>,
    },
    Delta {
        attempt: u8,
        text: String,
    },
    Finish {
        attempt: u8,
        reason: String,
    },
    Usage {
        attempt: u8,
        usage: Usage,
    },
    RetryAfter {
        delay: Duration,
    },
    AttemptFailed {
        attempt: u8,
        error: TransportError,
    },
    Complete {
        attempt: u8,
    },
}

/// Carries sanitized provider messages without attaching request URLs or headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    InvalidConfiguration,
    InvalidModel,
    ModelNotFound,
    UnsupportedReasoning,
    OutputLimit,
    Http(u16),
    HttpMessage { status: u16, message: String },
    Network,
    MalformedEvent,
    Provider(Option<u16>),
    Disconnected,
    FirstTokenTimeout,
    IdleTimeout,
    Deadline,
    Cancelled,
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self, Self::UnsupportedReasoning) {
            return f.write_str("MODEL_PARAMETER_UNSUPPORTED: reasoning");
        }
        if let Self::HttpMessage { status, message } = self {
            return write!(f, "OpenRouter HTTP {status}: {message}");
        }
        write!(f, "OpenRouter transport: {self:?}")
    }
}
impl std::error::Error for TransportError {}
impl TransportError {
    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::Http(status) | Self::HttpMessage { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Network
                | Self::Disconnected
                | Self::FirstTokenTimeout
                | Self::IdleTimeout
                | Self::Http(429 | 500..=599)
                | Self::HttpMessage {
                    status: 429 | 500..=599,
                    ..
                }
                | Self::Provider(Some(429 | 500..=599))
        )
    }
}

/// Called only for a missing model. Implementations must bound repair attempts
/// and account alias probes; they must not replay a frozen work request.
pub trait ModelRecovery: Send + Sync {
    fn recover(
        &self,
        client: OpenRouterClient,
        model: String,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
}

#[derive(Clone)]
pub struct OpenRouterClient {
    http: Client,
    endpoint: reqwest::Url,
    authorization: HeaderValue,
    model_recovery: Option<std::sync::Arc<dyn ModelRecovery>>,
}
impl fmt::Debug for OpenRouterClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenRouterClient")
            .field("authorization", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}
pub type EventStream = Pin<Box<dyn Stream<Item = StreamEvent> + Send>>;

impl OpenRouterClient {
    pub fn with_model_recovery(mut self, recovery: std::sync::Arc<dyn ModelRecovery>) -> Self {
        self.model_recovery = Some(recovery);
        self
    }

    /// Production constructor keeps credentials in their sensitive header wrapper.
    pub fn from_config(config: &squad_protocol::SquadConfig) -> Result<Self, TransportError> {
        let authorization = config
            .openrouter_api_key
            .authorization_header()
            .map_err(|_| TransportError::InvalidConfiguration)?;
        let mut client = Self::new(&config.openrouter_base_url, "unused")?;
        client.authorization = authorization;
        Ok(client)
    }

    /// `base_url` includes `/api/v1/`; tests supply a loopback fake.
    pub fn new(base_url: &str, api_key: &str) -> Result<Self, TransportError> {
        let base =
            reqwest::Url::parse(base_url).map_err(|_| TransportError::InvalidConfiguration)?;
        if !matches!(base.scheme(), "http" | "https")
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(TransportError::InvalidConfiguration);
        }
        let endpoint = base
            .join("chat/completions")
            .map_err(|_| TransportError::InvalidConfiguration)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|_| TransportError::InvalidConfiguration)?;
        authorization.set_sensitive(true);
        let http = Client::builder()
            .connect_timeout(Duration::from_millis(limits::CONNECT_TIMEOUT_MS))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| TransportError::InvalidConfiguration)?;
        Ok(Self {
            http,
            endpoint,
            authorization,
            model_recovery: None,
        })
    }

    async fn http_error(&self, response: reqwest::Response) -> TransportError {
        let status = response.status().as_u16();
        // Bound untrusted error bodies; never include raw bodies or response headers.
        let mut bytes = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(Ok(chunk)) = bytes.next().await {
            if body.len() + chunk.len() > 16_384 {
                return TransportError::Http(status);
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        if status == 400 && model_not_found(&value) {
            return TransportError::ModelNotFound;
        }
        let Some(message) = value.pointer("/error/message").and_then(Value::as_str) else {
            return TransportError::Http(status);
        };
        let credential = self.authorization.to_str().unwrap_or_default();
        let key = credential.strip_prefix("Bearer ").unwrap_or(credential);
        let message = message
            .replace(credential, "[REDACTED]")
            .replace(key, "[REDACTED]");
        let message: String = message
            .chars()
            .filter(|ch| !ch.is_control())
            .take(1024)
            .collect();
        TransportError::HttpMessage { status, message }
    }

    /// Fresh startup catalog fetch. The resolver owns its three-failure policy.
    pub async fn fetch_catalog(&self) -> Result<Value, TransportError> {
        let url = self
            .endpoint
            .join("../models")
            .map_err(|_| TransportError::InvalidConfiguration)?;
        self.json_call(self.http.get(url)).await
    }

    /// Probe an alias without assuming unpublished reasoning capabilities.
    pub async fn probe_alias(&self, alias: &str, max_tokens: u32) -> Result<Value, TransportError> {
        self.probe_alias_with_reasoning(alias, max_tokens, None)
            .await
    }

    /// The resolver supplies only a catalog-supported lowest reasoning effort.
    pub async fn probe_alias_with_reasoning(
        &self,
        alias: &str,
        max_tokens: u32,
        effort: Option<&str>,
    ) -> Result<Value, TransportError> {
        if !alias.starts_with('~') || !alias.contains('/') || !matches!(max_tokens, 16 | 2048) {
            return Err(TransportError::InvalidModel);
        }
        let mut body = json!({"model": alias, "messages": [{"role": "user", "content": "Reply OK."}], "stream": false, "max_tokens": max_tokens});
        if let Some(effort) = effort {
            body["reasoning"] = json!({"effort": effort});
        }
        self.json_call(self.http.post(self.endpoint.clone()).json(&body))
            .await
    }

    /// Reconcile a known provider generation without exposing credentials.
    pub async fn generation(&self, id: &str) -> Result<Value, TransportError> {
        let mut url = self
            .endpoint
            .join("../generation")
            .map_err(|_| TransportError::InvalidConfiguration)?;
        url.query_pairs_mut().append_pair("id", id);
        self.json_call(self.http.get(url)).await
    }

    async fn json_call(&self, request: reqwest::RequestBuilder) -> Result<Value, TransportError> {
        let response = request
            .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
            .timeout(Duration::from_millis(limits::TOTAL_TIMEOUT_MS))
            .send()
            .await
            .map_err(|_| TransportError::Network)?;
        if !response.status().is_success() {
            return Err(self.http_error(response).await);
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| TransportError::MalformedEvent)?;
        if let Some(error) = value.get("error") {
            return Err(TransportError::Provider(
                error
                    .get("code")
                    .and_then(Value::as_u64)
                    .and_then(|n| u16::try_from(n).ok()),
            ));
        }
        Ok(value)
    }

    /// Lazy stream: dropping it aborts HTTP reads and retry waiting. Cancellation
    /// wins over dispatch, including when the token was already cancelled.
    pub fn stream(&self, request: ChatRequest, cancel: CancellationToken) -> EventStream {
        self.stream_until(request, cancel, None)
    }

    /// A router deadline can shorten, but never extend, the constant deadline.
    pub fn stream_until(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
        authorized_deadline: Option<Instant>,
    ) -> EventStream {
        let this = self.clone();
        Box::pin(async_stream::stream! {
            let body = match request.body() {
                Ok(body) => body,
                Err(error) => { yield StreamEvent::AttemptFailed { attempt: 0, error }; return; }
            };
            let (attempts, class) = match request.policy {
                ExecutionPolicy::OrdinaryReply => (limits::REPLY_ATTEMPTS, DeadlineClass::Ordinary),
                ExecutionPolicy::SingleAttempt(class) => (1, class),
            };
            let constant_deadline = Instant::now() + class.duration();
            let deadline = authorized_deadline.map_or(constant_deadline, |limit| limit.min(constant_deadline));
            for attempt in 1..=attempts {
                if cancel.is_cancelled() {
                    yield StreamEvent::AttemptFailed { attempt, error: TransportError::Cancelled }; return;
                }
                yield StreamEvent::AttemptStarted { attempt };
                let first_deadline = (Instant::now() + Duration::from_millis(limits::FIRST_TOKEN_TIMEOUT_MS)).min(deadline);
                let mut retry_after = None;
                let mut call = this.call(&body, attempt, &mut retry_after);
                let mut activity_deadline = first_deadline;
                let mut got_token = false;
                let error = loop {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => break TransportError::Cancelled,
                        _ = sleep_until(deadline) => break TransportError::Deadline,
                        _ = sleep_until(activity_deadline) => break if got_token { TransportError::IdleTimeout } else { TransportError::FirstTokenTimeout },
                        event = call.next() => match event {
                            Some(Ok(event)) => {
                                if matches!(event, StreamEvent::Delta { .. }) {
                                    got_token = true;
                                }
                                if got_token {
                                    activity_deadline = Instant::now() + Duration::from_millis(limits::IDLE_TIMEOUT_MS);
                                }
                                let complete = matches!(event, StreamEvent::Complete { .. });
                                yield event;
                                if complete { return; }
                            },
                            Some(Err(error)) => break error,
                            None => break TransportError::Disconnected,
                        }
                    }
                };
                drop(call);
                if (matches!(error, TransportError::ModelNotFound | TransportError::Provider(Some(404)))
                    || error.http_status() == Some(404))
                    && let Some(recovery) = &this.model_recovery {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => {},
                        _ = sleep_until(deadline) => {},
                        _ = recovery.recover(this.clone(), request.model.clone()) => {},
                    }
                }
                let retry = attempt < attempts && error.retryable();
                if let Some(delay) = retry_after { yield StreamEvent::RetryAfter { delay }; }
                yield StreamEvent::AttemptFailed { attempt, error };
                if !retry { return; }
                // Full jitter over the exponential window; first retry is [0, 1s].
                let window_ms = (1_000_u64 << (attempt - 1)).min(30_000);
                let jitter = Duration::from_millis(rand::random_range(0..=window_ms));
                let delay = retry_after.unwrap_or_default().max(jitter);
                let wake = Instant::now() + delay;
                if wake >= deadline {
                    yield StreamEvent::AttemptFailed { attempt, error: TransportError::Deadline }; return;
                }
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => { yield StreamEvent::AttemptFailed { attempt, error: TransportError::Cancelled }; return; },
                    _ = sleep_until(wake) => {}
                }
            }
        })
    }

    fn call<'a>(
        &'a self,
        body: &'a Value,
        attempt: u8,
        retry_after: &'a mut Option<Duration>,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent, TransportError>> + Send + 'a>> {
        Box::pin(async_stream::try_stream! {
            let response = self.http.post(self.endpoint.clone())
                .header(reqwest::header::AUTHORIZATION, self.authorization.clone())
                .json(body).send().await.map_err(|_| TransportError::Network)?;
            let status = response.status();
            if !status.is_success() {
                *retry_after = response.headers().get(RETRY_AFTER).and_then(|v| v.to_str().ok()).and_then(parse_retry_after);
                Err(self.http_error(response).await)?;
                return;
            }
            let mut bytes = response.bytes_stream();
            let mut parser = sse::SseParser::default();
            let mut finished = false;
            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|_| TransportError::Network)?;
                for event in parser.push(&chunk)? {
                    let payload = event.data;
                    if payload == "[DONE]" {
                        if !finished { Err(TransportError::Disconnected)?; }
                        yield StreamEvent::Complete { attempt };
                        return;
                    }
                    let value: Value = serde_json::from_str(&payload).map_err(|_| TransportError::MalformedEvent)?;
                    if event.is_error {
                        let error = value.get("error").unwrap_or(&value);
                        let code = error.get("code").and_then(Value::as_u64).and_then(|n| u16::try_from(n).ok());
                        if model_not_found(&value) { Err(TransportError::ModelNotFound)?; }
                        Err(TransportError::Provider(code))?;
                    }
                    if let Some(error) = value.get("error") {
                        let code = error.get("code").and_then(Value::as_u64).and_then(|n| u16::try_from(n).ok());
                        if model_not_found(&value) { Err(TransportError::ModelNotFound)?; }
                        Err(TransportError::Provider(code))?;
                    }
                    let id = value.get("id").and_then(Value::as_str).map(str::to_owned);
                    let model = value.get("model").and_then(Value::as_str).map(str::to_owned);
                    if id.is_some() || model.is_some() {
                        yield StreamEvent::Metadata { attempt, generation_id: id, response_model: model };
                    }
                    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
                        for choice in choices {
                            if choice.get("index").and_then(Value::as_u64).unwrap_or(0) != 0 { continue; }
                            // Only content is answer text. reasoning/reasoning_details never leave this scope.
                            if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                                yield StreamEvent::Delta { attempt, text: text.to_owned() };
                            }
                            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                                finished = true;
                                yield StreamEvent::Finish { attempt, reason: reason.to_owned() };
                            }
                        }
                    }
                    if let Some(usage) = value.get("usage").filter(|u| !u.is_null()) {
                        let usage = serde_json::from_value(usage.clone()).map_err(|_| TransportError::MalformedEvent)?;
                        yield StreamEvent::Usage { attempt, usage };
                    }
                }
            }
            // A clean EOF without DONE is still incomplete, even after finish_reason:
            // terminal usage might have been lost.
            Err(TransportError::Disconnected)?;
        })
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()
                .map(|when| when.duration_since(SystemTime::now()).unwrap_or_default())
        })
}

// Inspect only a safe classification; provider messages never enter diagnostics.
fn model_not_found(value: &Value) -> bool {
    let error = value.get("error").unwrap_or(value);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    message.contains("no endpoints found")
        || message.contains("model")
            && [
                "not found",
                "does not exist",
                "no endpoints found",
                "not a valid model",
            ]
            .iter()
            .any(|needle| message.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn http_error_keeps_status_and_message_without_credentials_or_headers() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/models"))
            .respond_with(ResponseTemplate::new(400)
                .insert_header("x-private", "header-secret")
                .set_body_json(json!({"error": {"message": "Only one reasoning option allowed; Bearer fixture-secret\u{1b}", "headers": {"secret": "header-secret"}}})))
            .mount(&server).await;
        let client =
            OpenRouterClient::new(&format!("{}/", server.uri()), "fixture-secret").unwrap();
        let error = client.fetch_catalog().await.unwrap_err();
        let text = error.to_string();
        assert!(text.contains("HTTP 400: Only one reasoning option allowed"));
        assert!(!text.contains("fixture-secret"));
        assert!(!text.contains("header-secret"));
        assert!(!text.contains('\u{1b}'));
        assert!(!error.retryable());
    }

    #[test]
    fn retry_after_supports_http_dates_and_delta_seconds() {
        assert_eq!(parse_retry_after("13"), Some(Duration::from_secs(13)));
        let future = SystemTime::now() + Duration::from_secs(60);
        let delay = parse_retry_after(&httpdate::fmt_http_date(future)).unwrap();
        assert!(delay >= Duration::from_secs(58) && delay <= Duration::from_secs(60));
        assert_eq!(
            parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(Duration::ZERO)
        );
        assert_eq!(parse_retry_after("invalid"), None);
    }
    #[test]
    fn deadlines_are_protocol_constants_and_errors_are_sanitized() {
        for (class, ms) in [
            (DeadlineClass::Ordinary, limits::TOTAL_TIMEOUT_MS),
            (DeadlineClass::DebateTurn, limits::DEBATE_TURN_DEADLINE_MS),
            (DeadlineClass::FinalJudge, limits::FINAL_JUDGE_DEADLINE_MS),
            (DeadlineClass::RoundJudge, limits::ROUND_JUDGE_DEADLINE_MS),
            (
                DeadlineClass::TournamentJudge,
                limits::TOURNAMENT_JUDGE_DEADLINE_MS,
            ),
        ] {
            assert_eq!(class.duration(), Duration::from_millis(ms));
        }
        assert!(OpenRouterClient::new("http://user:password@localhost/", "key").is_err());
        assert!(OpenRouterClient::new("http://localhost/", "bad\nkey").is_err());
    }
}
