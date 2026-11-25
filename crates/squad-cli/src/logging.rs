//! File-only JSON tracing with sensitive field filtering and credential redaction.
use serde_json::{Map, Value, json};
use std::{
    fmt,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{
    Layer,
    filter::LevelFilter,
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
};

#[derive(Default)]
struct Fields(Map<String, Value>);

fn allowed(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let credential = ["key", "token"]
        .iter()
        .any(|part| name == *part || name.ends_with(&format!("_{part}")));
    !credential
        && ![
            "authorization",
            "secret",
            "header",
            "body",
            "prompt",
            "reasoning",
        ]
        .iter()
        .any(|part| name.contains(part))
}
impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if allowed(field.name()) {
            self.0
                .insert(field.name().into(), json!(format!("{value:?}")));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if allowed(field.name()) {
            self.0.insert(field.name().into(), json!(value));
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        if allowed(field.name()) {
            self.0.insert(field.name().into(), json!(value));
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        if allowed(field.name()) {
            self.0.insert(field.name().into(), json!(value));
        }
    }
}

struct JsonFileLayer {
    file: Mutex<File>,
    secrets: Vec<String>,
}
impl<S> Layer<S> for JsonFileLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        attrs.record(&mut fields);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(fields);
        }
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id)
            && let Some(fields) = span.extensions_mut().get_mut::<Fields>()
        {
            values.record(fields);
        }
    }
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        let mut spans = Vec::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let values = extensions
                    .get::<Fields>()
                    .map(|f| f.0.clone())
                    .unwrap_or_default();
                spans.push(json!({"name": span.name(), "fields": values}));
            }
        }
        event.record(&mut fields);
        let metadata = event.metadata();
        let mut row = json!({
            "timestamp_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "level": metadata.level().as_str(), "target": metadata.target(),
            "name": metadata.name(), "fields": fields.0, "spans": spans,
        });
        redact(&mut row, &self.secrets);
        // Flush every event. A failed sink never falls back to terminal logging.
        if let Ok(mut file) = self.file.lock() {
            let _ = serde_json::to_writer(&mut *file, &row)
                .map_err(std::io::Error::other)
                .and_then(|()| file.write_all(b"\n"))
                .and_then(|()| file.flush());
        }
    }
}

fn redact(value: &mut Value, secrets: &[String]) {
    match value {
        Value::String(text) => {
            for secret in secrets.iter().filter(|s| !s.is_empty()) {
                *text = text.replace(secret, "[REDACTED]");
            }
        }
        Value::Array(values) => {
            for value in values {
                redact(value, secrets);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                redact(value, secrets);
            }
        }
        _ => {}
    }
}

pub fn level(value: squad_protocol::config::LogLevel) -> LevelFilter {
    use squad_protocol::config::LogLevel;
    match value {
        LogLevel::Trace => LevelFilter::TRACE,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Off => LevelFilter::OFF,
    }
}

fn layer(data_dir: &Path, secrets: Vec<String>) -> Result<JsonFileLayer, String> {
    let logs = data_dir.join("logs");
    crate::paths::create_private_dir(&logs)
        .map_err(|_| "Cannot create log directory.".to_owned())?;
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(logs.join("squad.jsonl"))
        .map_err(|_| "Cannot open log file.".to_owned())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "Cannot secure log file.".to_owned())?;
    }
    Ok(JsonFileLayer {
        file: Mutex::new(file),
        secrets,
    })
}

pub fn init(
    data_dir: &Path,
    level: LevelFilter,
    key: &squad_protocol::config::OpenRouterApiKey,
) -> Result<(), String> {
    tracing_subscriber::registry()
        .with(level)
        .with(layer(
            data_dir,
            vec![
                key.authorization_header()
                    .map_err(|_| "Cannot initialize credential redaction.".to_owned())?
                    .to_str()
                    .map_err(|_| "Cannot initialize credential redaction.".to_owned())?
                    .strip_prefix("Bearer ")
                    .unwrap_or_default()
                    .to_owned(),
            ],
        )?)
        .try_init()
        .map_err(|_| "Cannot initialize file logging.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logs_messages_and_identities_without_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let subscriber = tracing_subscriber::registry().with(LevelFilter::INFO).with(
            layer(
                temp.path(),
                vec!["fixture-openrouter-credential".to_owned()],
            )
            .unwrap(),
        );
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "request",
                request_id = "r1",
                event_id = "e1",
                work_id = "w1",
                attempt = 2_u64,
                contest_id = "c1",
                case_id = "k1",
                api_key = "span-secret",
                authorization = "span-auth"
            );
            let _guard = span.enter();
            span.record("attempt", 3_u64);
            tracing::info!(model = "provider/model", cursor = 42_u64, input_tokens = 11_u64, output_tokens = 7_u64, max_output_tokens = 64_u64, token = "token-secret", access_token = "access-secret", prompt = "private-prompt", body = "private-body", reasoning = "private-reasoning", OPENROUTER_API_KEY = "key-secret", authorization = "auth-secret", proxy_authorization = "proxy-secret", headers = ?["header-secret"], secret_key = "other-secret", "Doctor initialized: {}", "fixture-openrouter-credential");
            tracing::debug!(cursor = 99_u64);
        });
        let contents = std::fs::read_to_string(temp.path().join("logs/squad.jsonl")).unwrap();
        for secret in [
            "token-secret",
            "access-secret",
            "private-prompt",
            "private-body",
            "private-reasoning",
            "span-secret",
            "span-auth",
            "key-secret",
            "auth-secret",
            "proxy-secret",
            "header-secret",
            "other-secret",
            "fixture-openrouter-credential",
            "authorization",
            "API_KEY",
        ] {
            assert!(!contents.contains(secret), "leaked {secret}");
        }
        let rows: Vec<Value> = contents
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0]["fields"]["message"],
            "Doctor initialized: [REDACTED]"
        );
        assert_eq!(rows[0]["fields"]["cursor"], 42);
        assert_eq!(rows[0]["fields"]["input_tokens"], 11);
        assert_eq!(rows[0]["fields"]["output_tokens"], 7);
        assert_eq!(rows[0]["fields"]["max_output_tokens"], 64);
        assert_eq!(rows[0]["fields"]["model"], "provider/model");
        let fields = &rows[0]["spans"][0]["fields"];
        for (key, value) in [
            ("request_id", "r1"),
            ("event_id", "e1"),
            ("work_id", "w1"),
            ("contest_id", "c1"),
            ("case_id", "k1"),
        ] {
            assert_eq!(fields[key], value);
        }
        assert_eq!(fields["attempt"], 3);
    }
}
