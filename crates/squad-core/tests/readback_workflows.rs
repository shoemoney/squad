#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::*;
use squad_agents::openrouter::OpenRouterClient;
use squad_core::room::RoomHandle;
use squad_protocol::*;
use std::{collections::BTreeMap, time::Duration};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};

struct Fixture {
    _dir: tempfile::TempDir,
    config: SquadConfig,
    fake: FakeOpenRouter,
}
impl Fixture {
    async fn new() -> Self {
        let _=tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).with_test_writer().try_init();
        let dir = tempfile::tempdir().unwrap();
        let options = config::LoadOptions {
            env_file: None,
            data_dir: Some(dir.path().join("data")),
            working_dir: dir.path().into(),
            config_dir: None,
            default_data_dir: dir.path().join("data"),
        };
        let mut config = SquadConfig::load_with_env(
            &options,
            &BTreeMap::from([("OPENROUTER_API_KEY".into(), "fake-key".into())]),
        )
        .unwrap();
        config.agents = (0..6)
            .map(|i| config::AgentConfig {
                nick: nick(&format!("member{i}")),
                model: format!("fixture/member{i}"),
                persona: format!("PERSONA_{i}"),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = "fixture/judge".into();
        config.budget.daily_max_usd = 200.;
        config.budget.debate_confirm_usd = 100.;
        let fake = FakeOpenRouter::start().await;
        let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
        fake.catalog(serde_json::json!({"data":models})).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Answers)
            .mount(&fake.server)
            .await;
        Self {
            _dir: dir,
            config,
            fake,
        }
    }
    async fn open(&self, mode: OpenMode) -> RoomHandle {
        RoomHandle::open_with_client(
            self.config.clone(),
            "main",
            mode,
            OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
        )
        .await
        .unwrap()
    }
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    fn assert_ledger(&self) {
        let mut db = self.db();
        let tx = db.transaction().unwrap();
        let (spent,reserved,unknown):(i64,i64,i64)=tx.query_row("SELECT COALESCE(SUM(spent_micros),0),COALESCE(SUM(reserved_micros),0),COALESCE(SUM(unknown_calls),0) FROM spend_daily",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        let actual: i64 = tx
            .query_row(
                "SELECT COALESCE(SUM(reported_cost_micros),0) FROM attempts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let held: i64 = tx
            .query_row(
                "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let pending: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM attempts WHERE unresolved=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((spent, reserved, unknown), (actual, held, pending));
        assert!(spent >= 0 && reserved >= 0);
    }
    async fn wait(&self, check: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !check() {
                self.assert_ledger();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            let db = self.db();
            let mut q = db
                .prepare("SELECT kind,status,attempt,error_json FROM work")
                .unwrap();
            let rows = q
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            panic!("timeout: {rows:?}")
        });
    }
}
fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}
fn request(config: &SquadConfig, text: &str) -> squad_protocol::Request {
    let ctx = parser::ParseContext {
        roster: config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    squad_protocol::Request::from_parsed(
        EventId::new(),
        parse(text, &meta, &ctx).unwrap(),
        &meta,
        false,
    )
    .unwrap()
}
#[derive(Clone)]
struct Answers;
const SPEC: &str =
    "The service retains records. The operator deletes backups. The client retries requests.";
fn ledger(spec: &str) -> serde_json::Value {
    serde_json::json!({"entries":if spec.contains("CONVERGE") || spec.contains("for thirty days"){vec![]}else{vec![serde_json::json!({"entryId":"retention","passage":"The service retains records.","question":"For how long?","readings":[{"readingId":"a","text":"Retain for thirty days","sourceLabels":["Reader 1"]},{"readingId":"b","text":"Retain forever","sourceLabels":["Reader 2"]}],"behaviourChange":"Records expire or persist"}),serde_json::json!({"entryId":"backups","passage":"The operator deletes backups.","question":"When?","readings":[{"readingId":"a","text":"Delete daily","sourceLabels":["Reader 1"]},{"readingId":"b","text":"Delete on request","sourceLabels":["Reader 2"]}],"behaviourChange":"Backup lifetime differs"})]}})
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["response_format"]["json_schema"]["name"] != "readback_response" {
            return streaming("ordinary response", "stop");
        }
        let text = body["messages"][1]["content"].as_str().unwrap();
        let data = extract(text);
        let spec = data["spec"].as_str().unwrap();
        let output = if text.contains("Read this spec independently") {
            if spec.contains("FAIL_MEMBERS") {
                serde_json::json!({"wrong":"shape"})
            } else if spec.contains("REPAIR_MEMBER") && !text.contains("failed validation") {
                serde_json::json!({"wrong":"shape"})
            } else {
                serde_json::json!({"build":"Implement retention","behaviours":[{"localId":"one","situation":"A record is saved","behaviour":"Keep it","specQuote":if spec.contains("for thirty days"){"The service retains records for thirty days."}else{"The service retains records."}}],"edgeCases":[],"assumptions":[]})
            }
        } else if text.contains("Compare the labelled") {
            let mut value = ledger(spec);
            if spec.contains("REPAIR_DIFF") && !text.contains("failed validation") {
                value["entries"][0]["passage"] =
                    serde_json::json!("Invented passage with no source");
            }
            value
        } else {
            let changes=data["choices"].as_object().unwrap().keys().map(|key|{
 let e=data["ledger"]["entries"].as_array().unwrap().iter().find(|e|e["entryId"].as_str()==Some(key)).unwrap();
 serde_json::json!({"entryId":key,"before":e["passage"],"after":"The service retains records for thirty days."})}).collect::<Vec<_>>();
            let mut rewritten = spec.to_owned();
            for c in &changes {
                rewritten = rewritten.replacen(
                    c["before"].as_str().unwrap(),
                    c["after"].as_str().unwrap(),
                    1,
                );
            }
            serde_json::json!({"spec":rewritten,"changes":changes})
        };
        streaming(&output.to_string(), "stop")
    }
}
fn extract(text: &str) -> serde_json::Value {
    for (i, c) in text.char_indices() {
        if c == '{' {
            let mut stream =
                serde_json::Deserializer::from_str(&text[i..]).into_iter::<serde_json::Value>();
            if let Some(Ok(v)) = stream.next()
                && v.get("input").is_some()
            {
                return v["input"].clone();
            }
        }
    }
    panic!("No input: {text}")
}
impl Fixture {
    fn state(&self) -> ReadbackState {
        let s: String = self
            .db()
            .query_row(
                "SELECT state_json FROM contests WHERE kind='readback' ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&s).unwrap()
    }
    async fn answer(&self, room: &RoomHandle, text: Option<&str>) {
        let state = self.state();
        let r = room
            .submit(Request::HumanAnswer {
                request_id: EventId::new(),
                contest_id: state.contest_id,
                expected_state_version: state.state_version,
                text: text.map(str::to_owned),
            })
            .await;
        assert!(matches!(r, RequestResult::Accepted { .. }), "{r:?}");
    }
    async fn start(&self, room: &RoomHandle, marker: &str) {
        let r = room
            .submit(request(
                &self.config,
                &format!("@all $readback {SPEC} {marker}"),
            ))
            .await;
        assert!(matches!(r, RequestResult::Accepted { .. }), "{r:?}");
    }
}
#[tokio::test]
async fn partial_choices_restart_rewrite_fresh_round_and_export() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    f.start(&room, "").await;
    f.wait(|| f.state().state == ReadbackStatus::AwaitingHuman)
        .await;
    assert_eq!(f.fake.captured_bodies().await.len(), 7);
    let holds: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM work WHERE held_by_contest_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(holds, 0);
    f.answer(&room, Some("1b")).await;
    f.answer(&room, Some("1a")).await;
    let state = f.state();
    assert_eq!(state.state, ReadbackStatus::AwaitingHuman);
    let raw: String = f
        .db()
        .query_row(
            "SELECT pending_json FROM contest_human_steps WHERE closed=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw.contains("a"));
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(f.state(), state);
    let result = room
        .submit(request(&f.config, "@all an ordinary message"))
        .await;
    assert!(matches!(result, RequestResult::Accepted { .. }));
    assert_eq!(f.state().state, ReadbackStatus::AwaitingHuman);
    f.answer(&room, None).await;
    f.wait(|| f.state().state.is_terminal()).await;
    assert_eq!(f.state().terminal_reason.unwrap().as_str(), "converged");
    let run = squad_store::readback::run(&f.db(), state.contest_id).unwrap();
    assert_eq!(run.rounds.len(), 2);
    assert!(
        run.spec
            .contains("The operator deletes backups. The client retries requests.")
    );
    assert_eq!(run.rounds[0].choices.as_ref().unwrap().answers.len(), 1);
    let bodies = f.fake.captured_bodies().await;
    let interpretations = bodies
        .iter()
        .filter(|b| {
            b["response_format"]["json_schema"]["name"] == "readback_response"
                && b["messages"][1]["content"]
                    .as_str()
                    .unwrap()
                    .contains("Read this spec independently")
        })
        .collect::<Vec<_>>();
    assert_eq!(interpretations.len(), 12);
    for body in interpretations.iter().filter(|b| {
        b["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("for thirty days")
    }) {
        let text = body["messages"][1]["content"].as_str().unwrap();
        let data = extract(text);
        assert_eq!(data.as_object().unwrap().len(), 1);
        assert!(data["spec"].as_str().unwrap().contains("for thirty days"));
    }
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE kind LIKE 'readback.%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        15
    );
    let report:String=f.db().query_row("SELECT payload FROM events WHERE json_extract(payload,'$.artifactKind')='readback.report'",[],|r|r.get(0)).unwrap();
    let a: ContestArtifact = serde_json::from_str(&report).unwrap();
    let report: squad_protocol::readback::ReadbackReport =
        serde_json::from_value(a.body.unwrap()).unwrap();
    assert_eq!(report.convergence[0].open, 1);
    assert_eq!(report.raw_answers, vec!["1b", "1a"]);
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COALESCE(SUM(reserved_micros),0) FROM spend_daily",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn converged_no_choices_and_insufficient_readings() {
    for (marker, reason) in [
        ("CONVERGE", "converged"),
        ("", "no_readings_chosen"),
        ("FAIL_MEMBERS", "insufficient_readings"),
    ] {
        let f = Fixture::new().await;
        let room = f.open(OpenMode::Interactive).await;
        f.start(&room, marker).await;
        f.wait(|| {
            matches!(
                f.state().state,
                ReadbackStatus::AwaitingHuman | ReadbackStatus::Complete | ReadbackStatus::Failed
            )
        })
        .await;
        if f.state().state == ReadbackStatus::AwaitingHuman {
            f.answer(&room, None).await;
            f.wait(|| f.state().state.is_terminal()).await;
        }
        assert_eq!(f.state().terminal_reason.unwrap().as_str(), reason);
        assert_eq!(
            f.db()
                .query_row(
                    "SELECT COUNT(*) FROM work WHERE kind='readback.rewrite'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn member_and_judge_repairs_and_stop_fence() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    f.start(&room, "REPAIR_MEMBER REPAIR_DIFF").await;
    f.wait(|| f.state().state == ReadbackStatus::AwaitingHuman)
        .await;
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        7
    );
    let before = f.fake.captured_bodies().await.len();
    let r = room
        .submit(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    assert!(matches!(r, RequestResult::Accepted { .. }), "{r:?}");
    assert_eq!(f.state().state, ReadbackStatus::Stopped);
    let s = f.state();
    let r = room
        .submit(Request::HumanAnswer {
            request_id: EventId::new(),
            contest_id: s.contest_id,
            expected_state_version: s.state_version,
            text: Some("1a".into()),
        })
        .await;
    assert!(matches!(r, RequestResult::Rejected { .. }));
    assert_eq!(f.fake.captured_bodies().await.len(), before);
    room.shutdown().await.unwrap();
}
