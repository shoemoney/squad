use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

#[derive(Clone)]
struct Answers {
    calls: Arc<AtomicUsize>,
    hangups: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
}
impl Respond for Answers {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = request.body_json().unwrap();
        let model = body["model"].as_str().unwrap();
        if body["stream"] != true {
            return fake_openrouter::alias_probe(model);
        }
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if model == MODEL {
            if body["max_tokens"].as_u64() == Some(1536) {
                return fake_openrouter::round_score(serde_json::json!({"scores":{
                    "a":{"score":8,"reason":"Strong evidence"},
                    "b":{"score":7,"reason":"Useful alternative"}
                }}));
            }
            let scores = ["a", "b"].map(|nick| {
                let criteria = ["fit", "feasibility", "originality", "survivedAttacks"].map(|id| {
                    serde_json::json!({"criterionId":id,"score":if nick=="a" {8} else {7},"reasoning":"Supported"})
                });
                serde_json::json!({"nick":nick,"criteria":criteria})
            });
            return fake_openrouter::structured_judge(serde_json::json!({
                "winner":"a","scores":scores,"reasoning":"AUDIT_VERDICT supported by the supplied turns.",
                "whatToStealFromLoser":["Keep the alternative"],"tallyOverrideReason":null}));
        }
        let last = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|message| message["role"] == "user")
            .unwrap()["content"]
            .as_str()
            .unwrap();
        let hangup = last.contains("AUDIT_HANGUP");
        if last.contains("AUDIT_STOP") {
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
        let delayed = last.contains("AUDIT_STOP")
            || (hangup && self.hangups.fetch_add(1, Ordering::SeqCst) == 0);
        let text = if hangup {
            "AUDIT_RECOVERED".to_owned()
        } else {
            format!("AUDIT_ANSWER_{call} independent proposal")
        };
        let stream = fake_openrouter::stream_body(&text, "stop").replace(
            &format!("\"model\":\"{MODEL}\""),
            &format!("\"model\":\"{model}\""),
        );
        let response = fake_openrouter::raw_sse(stream);
        if delayed {
            response.set_delay(Duration::from_secs(120))
        } else {
            response
        }
    }
}
fn scalar(root: &Path, sql: &str) -> i64 {
    rusqlite::Connection::open(root.join("data/squad.db"))
        .unwrap()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}
fn submit(pty: &Pty, text: &str) {
    pty.send(b"\x01\x7f");
    pty.send(text.as_bytes());
    pty.wait_for("the complete draft", |screen| screen.contains(text));
    // A trailing space closes recipient/command completion before submission.
    pty.send(b" \r");
    pty.wait_for("draft consumed or confirmation", |screen| {
        !screen
            .lines()
            .any(|line| line.starts_with('>') && line.contains(text))
            || screen.contains("Confirm")
    });
}
fn resize(pty: &Pty, cols: u16, rows: u16) {
    pty.master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    pty.screen.lock().unwrap().screen_mut().set_size(
        std::num::NonZeroU16::new(rows).unwrap(),
        std::num::NonZeroU16::new(cols).unwrap(),
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_flow_commands_quotes_recall_search_regeneration_export_stop_and_recovery() {
    if pty_deadline::run_test(
        "canonical_pty_audit::canonical_flow_commands_quotes_recall_search_regeneration_export_stop_and_recovery",
    ) {
        return;
    }
    let server = FakeOpenRouter::start().await;
    server.catalog(serde_json::json!({"data":[
        {"id":MODEL,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning","response_format"],"pricing":{"prompt":"0.000001","completion":"0.000002"}},
        {"id":"fixture/plain-v1","context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":[],"pricing":{"prompt":"0.000001","completion":"0.000002"}}
    ]})).await;
    let stops = Arc::new(AtomicUsize::new(0));
    let hangups = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(Answers {
            calls: Arc::new(AtomicUsize::new(0)),
            hangups: hangups.clone(),
            stops: stops.clone(),
        })
        .mount(&server.server)
        .await;
    let root = tempfile::tempdir().unwrap();
    let extra = [
        ("SQUAD_MODEL_A", "fixture/plain-v1"),
        ("SQUAD_MODEL_B", "fixture/plain-v1"),
        ("SQUAD_ROUNDS", "1"),
        ("SQUAD_DEBATE_CONFIRM_USD", "0"),
        ("SQUAD_FX", "off"),
    ];
    let mut pty = Pty::spawn(root.path(), Some(&server), &extra);
    pty.wait_for("startup", ready_screen);
    submit(&pty, "@all $debate AUDIT_CANONICAL_QUESTION @judge");
    pty.wait_for("question-first pick", |screen| {
        screen.contains("CHOOSE") || screen.contains("Pick exactly")
    });
    submit(&pty, "@a @b");
    pty.wait_for("cost preview", |screen| {
        screen.contains("Confirm") || screen.contains("confirm")
    });
    pty.send(b"y");
    pty.wait_for("canonical verdict", |_| {
        scalar(
            root.path(),
            "SELECT COUNT(*) FROM events WHERE kind='verdict'",
        ) == 1
    });
    pty.wait_for("released contest", |_| {
        scalar(root.path(), "SELECT COUNT(*) FROM contests WHERE active=1") == 0
    });

    // Commands and recipient recall reach the parser through real terminal input.
    submit(&pty, "/set ROUNDS 2");
    assert_eq!(
        scalar(root.path(), "SELECT value FROM vars WHERE name='ROUNDS'"),
        2
    );
    pty.send(b"\x12");
    pty.wait_for("recipient recall", |screen| {
        screen.contains("> @a") || screen.contains("> @all")
    });
    pty.send(b" AUDIT_RECALLED\r");
    pty.wait_for("recalled-recipient answers", |_| {
        scalar(
            root.path(),
            "SELECT COUNT(*) FROM events WHERE kind='message.final'",
        ) >= 4
    });
    assert_eq!(
        scalar(
            root.path(),
            "SELECT json_array_length(recipients) FROM events WHERE kind='user.message' AND json_extract(payload,'$.body') LIKE '%AUDIT_RECALLED%'"
        ),
        2
    );
    pty.send(b"\x06");
    pty.wait_for("search", |screen| screen.contains("Enter Next"));
    pty.send(b"AUDIT_ANSWER");
    pty.wait_for("search matches", |screen| {
        screen.contains("Find: AUDIT_ANSWER") && !screen.contains("· 0/0")
    });
    pty.send(b"\r\x1b");
    pty.wait_for("search closed", |screen| !screen.contains("Enter Next"));

    pty.send(b"\x1b[4~");
    pty.wait_for("back to live", |screen| !screen.contains("back to live"));

    resize(&pty, 60, 15);
    pty.wait_for("resize notice", |screen| {
        screen.contains("Terminal too small")
    });
    resize(&pty, 100, 40);
    pty.wait_for("restored viewport", ready_screen);
    submit(&pty, "@a AUDIT_ORDINARY");
    pty.wait_for("ordinary answer", |_| {
        scalar(
            root.path(),
            "SELECT COUNT(*) FROM events WHERE kind='message.final'",
        ) >= 5
    });
    let latest: String = rusqlite::Connection::open(root.path().join("data/squad.db")).unwrap()
        .query_row("SELECT json_extract(payload,'$.content') FROM events WHERE kind='message.final' ORDER BY seq DESC LIMIT 1", [], |row| row.get(0)).unwrap();
    pty.wait_for("ordinary reply rendered", |screen| screen.contains(&latest));
    // F6, Up focuses the newest reply; Space selects it; Enter opens then quotes.
    pty.send(b"\x1b[17~\x1b[A \r");
    pty.wait_for("quote action menu", |screen| {
        screen.contains("Actions") && screen.contains("Quote")
    });
    pty.send(b"\r");
    pty.wait_for("attached quote", |screen| {
        screen.contains("Quoted 1") || screen.contains("quotes 1")
    });
    pty.send(b"\x1b[17~");
    submit(&pty, "@b AUDIT_QUOTED");
    pty.wait_for("quoted answer", |_| {
        scalar(
            root.path(),
            "SELECT COUNT(*) FROM events WHERE kind='message.final'",
        ) >= 6
    });
    assert_eq!(
        scalar(
            root.path(),
            "SELECT json_array_length(json_extract(payload,'$.resolvedQuotes')) FROM events WHERE kind='user.message' AND json_extract(payload,'$.body') LIKE '%AUDIT_QUOTED%'"
        ),
        1
    );
    pty.send(b"\x1b[17~\x1b[B\x1b[Br");
    pty.wait_for("regeneration", |_| {
        scalar(root.path(), "SELECT COUNT(*) FROM superseded") == 1
    });
    pty.wait_for("regenerated answer", |_| {
        scalar(
            root.path(),
            "SELECT COUNT(*) FROM events WHERE kind='message.final'",
        ) >= 7
    });
    pty.send(b"\x1b[17~");
    submit(&pty, "@a AUDIT_STOP");
    pty.wait_for("generation before stop", |_| {
        stops.load(Ordering::SeqCst) >= 1
    });
    submit(&pty, "/stop");
    pty.wait_for("stopped work", |_| {
        scalar(
            root.path(),
            "SELECT COUNT(*) FROM work WHERE status='cancelled'",
        ) >= 1
    });

    submit(&pty, "/export room");
    pty.wait_for("export path", |screen| screen.contains("squad-export.md"));
    pty.send(b"\r");
    pty.wait_for("export flushed", |screen| {
        screen.contains("Exported to") && root.path().join("squad-export.md").is_file()
    });
    let export = std::fs::read_to_string(root.path().join("squad-export.md")).unwrap();
    assert!(export.contains("AUDIT_CANONICAL_QUESTION") && export.contains("AUDIT_VERDICT"));
    assert!(!export.contains("PRIVATE REASONING") && !export.contains("fixture-key"));

    submit(&pty, "@b AUDIT_HANGUP");
    pty.wait_for("in-flight work at hangup", |_| {
        hangups.load(Ordering::SeqCst) >= 1
    });
    pty.signal("HUP");
    pty.wait_exit();
    drop(pty);
    assert!(integrity_ok(root.path()));
    let mut recovered = Pty::spawn(root.path(), Some(&server), &extra);
    recovered.wait_for("recovered work", |screen| {
        screen.contains("AUDIT_RECOVERED")
    });
    recovered.send(b"\x03");
    assert_eq!(recovered.wait_exit().exit_code(), 0);
    assert_restored(&recovered);
}
