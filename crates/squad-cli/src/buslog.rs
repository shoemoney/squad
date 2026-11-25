//! `--bus-log` sink: request, commit and mailbox wake-up diagnostics as JSON lines.
//! Bodies are redacted unless `--bus-log-bodies` is given; credentials never appear.
use serde_json::{Map, Value, json};
use squad_protocol::{EnvelopePayload, FeedItem};
use std::{
    fmt,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{
    Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

const TARGET: &str = "squad_core::bus::diagnostics";

#[derive(Clone)]
pub struct BusLog {
    file: Arc<Mutex<File>>,
    secrets: Arc<Vec<String>>,
    bodies: bool,
}

static ACTIVE: OnceLock<BusLog> = OnceLock::new();

impl BusLog {
    /// Creates (truncating) the file with owner-only permissions. A symlink or a
    /// non-regular file at `path` is refused, never followed, truncated or chmodded.
    pub fn open(path: &Path, bodies: bool, secrets: Vec<String>) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // O_NONBLOCK keeps a planted FIFO from hanging the open.
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| format!("Cannot open bus log: {}.", path.display()))?;
        if !file.metadata().is_ok_and(|meta| meta.is_file()) {
            return Err(format!(
                "Bus log must be a regular file: {}.",
                path.display()
            ));
        }
        file.set_len(0)
            .map_err(|_| format!("Cannot open bus log: {}.", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| "Cannot secure bus log.".to_owned())?;
        }
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            secrets: Arc::new(secrets),
            bodies,
        })
    }

    pub fn install(&self) {
        let _ = ACTIVE.set(self.clone());
    }

    pub fn layer<S: Subscriber>(&self) -> impl Layer<S> {
        BusLayer { log: self.clone() }
    }

    fn write(&self, mut row: Value) {
        redact(&mut row, &self.secrets);
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
        Value::Array(values) => values.iter_mut().for_each(|v| redact(v, secrets)),
        Value::Object(values) => values.values_mut().for_each(|v| redact(v, secrets)),
        _ => {}
    }
}

#[derive(Default)]
struct Fields(Map<String, Value>);
impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().into(), json!(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().into(), json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), json!(value));
    }
}

struct BusLayer {
    log: BusLog,
}
impl<S: Subscriber> Layer<S> for BusLayer {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _: Context<'_, S>) -> bool {
        metadata.target() == TARGET
    }
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != TARGET {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        // The tracing "message" is the diagnostic kind: bus.request | bus.commit | bus.mailbox.
        let kind = fields
            .0
            .remove("message")
            .and_then(|v| v.as_str().map(|s| s.trim_start_matches("bus.").to_owned()))
            .unwrap_or_else(|| event.metadata().name().to_owned());
        self.log.write(json!({
            "timestamp_ms": now_ms(),
            "kind": kind,
            "fields": fields.0,
        }));
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// Captures human-visible transcript text from the feed when bodies are enabled.
pub struct BodyTap {
    log: BusLog,
    feed: tokio::sync::broadcast::Receiver<FeedItem>,
}
impl BodyTap {
    pub fn start(feed: tokio::sync::broadcast::Receiver<FeedItem>) -> Option<Self> {
        let log = ACTIVE.get().filter(|log| log.bodies)?.clone();
        Some(Self { log, feed })
    }

    /// Writes every buffered committed message body; call before the room closes.
    pub fn drain(&mut self) {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match self.feed.try_recv() {
                Ok(FeedItem::Committed(event)) => {
                    let text = match &event.content {
                        EnvelopePayload::UserMessage(m) => Some(m.body.clone()),
                        EnvelopePayload::AgentReply(c) | EnvelopePayload::MessageFinal(c) => {
                            Some(c.content.clone())
                        }
                        _ => None,
                    };
                    if let Some(text) = text {
                        self.log.write(json!({
                            "timestamp_ms": now_ms(),
                            "kind": "body",
                            "fields": {
                                "event_id": event.base.id.to_string(),
                                "seq": event.commit.seq.get(),
                                "text": text,
                            },
                        }));
                    }
                }
                Ok(_) | Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Empty | TryRecvError::Closed) => return,
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::BusLog;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn a_planted_symlink_is_refused_and_its_target_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("victim");
        std::fs::write(&target, "precious").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let link = dir.path().join("bus.log");
        symlink(&target, &link).unwrap();
        assert!(BusLog::open(&link, false, Vec::new()).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "precious");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn non_regular_files_are_refused_and_regular_ones_truncated_to_0600() {
        let dir = tempfile::tempdir().unwrap();
        assert!(BusLog::open(dir.path(), false, Vec::new()).is_err());
        let file = dir.path().join("bus.log");
        std::fs::write(&file, "old contents").unwrap();
        BusLog::open(&file, false, Vec::new()).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 0);
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
