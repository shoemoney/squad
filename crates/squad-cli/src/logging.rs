//! File-only JSON tracing. Only the identity fields below may be recorded.
//! Never put credentials, headers, bodies, prompts, reasoning, or arbitrary Debug
//! objects in identity fields or tracing metadata (target/span/event names).
//! Free-form `message` and unrecognized fields are dropped before formatting.
//! TODO(M3.P3): wire the bus diagnostic source and its body opt-in policy.
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
    matches!(
        name,
        "request_id"
            | "requestId"
            | "event_id"
            | "eventId"
            | "work_id"
            | "workId"
            | "attempt"
            | "contest_id"
            | "contestId"
            | "debate_id"
            | "debateId"
            | "case_id"
            | "caseId"
            | "round"
            | "cursor"
            | "seq"
            | "chunk_index"
            | "chunkIndex"
    )
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
        let row = json!({
            "timestamp_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "level": metadata.level().as_str(), "target": metadata.target(),
            "name": metadata.name(), "fields": fields.0, "spans": spans,
        });
        // Flush every event. A failed sink never falls back to terminal logging.
        if let Ok(mut file) = self.file.lock() {
            let _ = serde_json::to_writer(&mut *file, &row)
                .map_err(std::io::Error::other)
                .and_then(|()| file.write_all(b"\n"))
                .and_then(|()| file.flush());
        }
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

fn layer(data_dir: &Path) -> Result<JsonFileLayer, String> {
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
    })
}

pub fn init(data_dir: &Path, level: LevelFilter) -> Result<(), String> {
    tracing_subscriber::registry()
        .with(level)
        .with(layer(data_dir)?)
        .try_init()
        .map_err(|_| "Cannot initialize file logging.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logs_json_identities_but_never_secret_fields_or_messages() {
        let temp = tempfile::tempdir().unwrap();
        let subscriber = tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(layer(temp.path()).unwrap());
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
            tracing::info!(cursor = 42_u64, OPENROUTER_API_KEY = "key-secret", authorization = "auth-secret", proxy_authorization = "proxy-secret", headers = ?["header-secret"], secret_key = "other-secret", "message-secret");
            tracing::debug!(cursor = 99_u64);
        });
        let contents = std::fs::read_to_string(temp.path().join("logs/squad.jsonl")).unwrap();
        for secret in [
            "span-secret",
            "span-auth",
            "key-secret",
            "auth-secret",
            "proxy-secret",
            "header-secret",
            "other-secret",
            "message-secret",
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
        assert_eq!(rows[0]["fields"]["cursor"], 42);
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
