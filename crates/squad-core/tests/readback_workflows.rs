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
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_test_writer()
            .try_init();
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
        if text.contains("Read this spec independently") && (spec.contains("SKIP_ONE") && body["model"]=="fixture/member5" || spec.contains("QUORUM") && body["model"]!="fixture/member0") {return ResponseTemplate::new(503);}
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
            if spec.contains("FAIL_JUDGE") {return streaming("{}","stop");}
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

// Exercise the mode-owned runtime independently of the shared judge wrapper so
// its script and storage contracts remain testable while scaffold fixes integrate.
struct Direct {
    router: std::sync::Arc<std::sync::Mutex<squad_core::router::Router>>,
    store: squad_store::Store,
}
struct DirectSubmit {
    router: std::sync::Arc<std::sync::Mutex<squad_core::router::Router>>,
    owner: ExecutorKey,
}
impl ExecutorSubmit for DirectSubmit {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            self.router
                .lock()
                .unwrap()
                .process(
                    Request::Internal(InternalRequest::Executor(request)),
                    Some(self.owner.clone()),
                )
                .unwrap()
                .result
        })
    }
}
struct UnusedRenderer;
impl PromptRenderer for UnusedRenderer {
    fn render(&self, _: &PromptRef, _: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        Err(ReaderError::Forbidden)
    }
}
impl Direct {
    fn new(f: &Fixture) -> Self {
        std::fs::create_dir_all(&f.config.data_dir).unwrap();
        let store = squad_store::Store::open(f.config.data_dir.join("direct.db")).unwrap();
        let models = f
            .config
            .agents
            .iter()
            .map(|a| a.model.clone())
            .chain(std::iter::once(f.config.judge.model.clone()))
            .collect::<Vec<_>>();
        store.call(move|db|{for model in models{db.execute("INSERT INTO catalog VALUES (?,1000000,65536,'{\"prompt\":\"0.000001\",\"completion\":\"0.000002\"}','[\"reasoning\"]','2026-11-26T00:00:00Z')",[model])?;}Ok(())}).unwrap();
        let bindings = f
            .config
            .agents
            .iter()
            .map(|a| {
                (
                    a.nick.to_string(),
                    ExecutorBinding {
                        model: a.model.parse().unwrap(),
                        effort: ReasoningEffort::Low,
                    },
                )
            })
            .chain(std::iter::once((
                "judge".into(),
                ExecutorBinding {
                    model: f.config.judge.model.parse().unwrap(),
                    effort: ReasoningEffort::High,
                },
            )))
            .collect();
        let mut router = squad_core::router::Router::new(
            store.clone(),
            f.config.agents.iter().map(|a| a.nick.clone()).collect(),
            bindings,
            200_000_000,
        )
        .unwrap();
        router.engine_config = Some(f.config.clone());
        Self {
            router: std::sync::Arc::new(std::sync::Mutex::new(router)),
            store,
        }
    }
    fn send(&self, f: &Fixture, text: &str) {
        let mut router = self.router.lock().unwrap();
        let ctx = router.parse_context().unwrap();
        let meta = Default::default();
        let request = Request::from_parsed_with_context(
            EventId::new(),
            parse(text, &meta, &ctx).unwrap(),
            &meta,
            &ctx,
            true,
        )
        .unwrap();
        let result = router.process(request, None).unwrap().result;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        let _ = f;
    }
    fn state(&self) -> ReadbackState {
        self.store
            .call(|db| {
                let raw: String = db.query_row(
                    "SELECT state_json FROM contests WHERE kind='readback'",
                    [],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&raw)?)
            })
            .unwrap()
    }
    async fn pump(&self, f: &Fixture) {
        for _ in 0..30 {
            self.router.lock().unwrap().drive_contests().unwrap();
            let records = self
                .store
                .call(|db| squad_store::leases::eligible(db))
                .unwrap();
            let records = records
                .into_iter()
                .filter(|w| w.status == WorkStatus::Queued)
                .collect::<Vec<_>>();
            if records.is_empty() {
                return;
            }
            for record in records {
                let id = record.work_id;
                let work = self
                    .store
                    .call(move |db| Ok(squad_store::work::template(db, id)?.unwrap().0))
                    .unwrap();
                let owner = if let Some(case) = work.case_id {
                    ExecutorKey::Case(case)
                } else {
                    let Target::Member(n) = &work.target else {
                        panic!()
                    };
                    ExecutorKey::Member(n.clone())
                };
                let key = owner.clone();
                let id = work.work_id;
                let grant = self
                    .store
                    .call(move |db| {
                        squad_store::leases::claim(db, id, &key, chrono::Utc::now(), None)
                    })
                    .unwrap()
                    .unwrap();
                let reader = std::sync::Arc::new(squad_store::readers::ScopedReaders::new(
                    self.store.clone(),
                    owner.clone(),
                    grant.epoch,
                ));
                let (_, mailbox) = tokio::sync::mpsc::channel(1);
                let context = AgentContext {
                    executor: owner.clone(),
                    mailbox,
                    history: reader.clone(),
                    work: reader,
                    submit: std::sync::Arc::new(DirectSubmit {
                        router: self.router.clone(),
                        owner,
                    }),
                    render: std::sync::Arc::new(UnusedRenderer),
                    cancel: Default::default(),
                };
                let grant = squad_agents::worker::start_attempt_sized(
                    &context,
                    &grant,
                    grant.work.attempt,
                    None,
                )
                .await
                .unwrap();
                squad_core::runtime::readback::execute(
                    &context,
                    &grant,
                    &OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap(),
                    &squad_core::prompts::PromptPack::embedded().unwrap(),
                    &f.config,
                    Default::default(),
                )
                .await
                .unwrap();
            }
        }
        panic!("mode script did not settle");
    }
}
#[tokio::test]
async fn direct_runtime_partial_rewrite_and_convergence() {
    let f = Fixture::new().await;
    let direct = Direct::new(&f);
    direct.send(&f, &format!("@all $readback {SPEC}"));
    direct.pump(&f).await;
    assert_eq!(direct.state().state, ReadbackStatus::AwaitingHuman);
    direct.send(&f, "1b");
    direct.send(&f, "/answer 1: Retain for thirty days");
    direct.send(&f, "/continue");
    direct.pump(&f).await;
    let s = direct.state();
    assert_eq!(s.state, ReadbackStatus::Complete);
    assert_eq!(s.terminal_reason.unwrap().as_str(), "converged");
    let run = direct
        .store
        .call(move |db| squad_store::readback::run(db, s.contest_id))
        .unwrap();
    assert_eq!(run.rounds.len(), 2);
    assert_eq!(run.rounds[0].choices.as_ref().unwrap().answers.len(), 1);
    assert!(
        run.spec
            .contains("The operator deletes backups. The client retries requests.")
    );
    let exported =
        squad_core::export::export(&direct.store, ExportScope::Readback, Some(s.contest_id))
            .unwrap();
    assert!(exported.contains("OPEN"));
    assert!(exported.contains("Retain for thirty days"));
    assert!(exported.contains("Spec version 2"));
}
#[tokio::test]
async fn direct_runtime_converged_and_no_readings_chosen_release_reservations() {
    for (marker, reason) in [("CONVERGE", "converged"), ("", "no_readings_chosen")] {
        let f = Fixture::new().await;
        let d = Direct::new(&f);
        d.send(&f, &format!("@all $readback {SPEC} {marker}"));
        d.pump(&f).await;
        if d.state().state == ReadbackStatus::AwaitingHuman {
            d.send(&f, "/continue");
            d.pump(&f).await;
        }
        assert_eq!(d.state().terminal_reason.unwrap().as_str(), reason);
        assert_eq!(
            d.store
                .call(|db| Ok(db.query_row(
                    "SELECT COALESCE(SUM(reserved_micros),0) FROM spend_daily",
                    [],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
            0
        );
    }
}
#[tokio::test]
async fn direct_runtime_judge_repair_round_limit_restart_and_stop() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(
        &f,
        &format!("@all $readback $READBACK_ROUNDS=1 {SPEC} REPAIR_DIFF"),
    );
    d.pump(&f).await;
    assert_eq!(d.state().state, ReadbackStatus::AwaitingHuman);
    assert_eq!(
        d.store
            .call(
                |db| Ok(db.query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r
                    .get::<_, i64>(0))?)
            )
            .unwrap(),
        1
    );
    d.send(&f, "1a");
    let prior = d.state();
    squad_core::router::recovery::recover_on_open_mode(
        &d.store,
        OpenMode::Interactive,
        chrono::Utc::now() + chrono::Duration::days(365),
    )
    .unwrap();
    let pending = d
        .router
        .lock()
        .unwrap()
        .parse_context()
        .unwrap()
        .pending_human
        .unwrap();
    assert_eq!(pending.answers[&1], "a");
    assert_eq!(d.state(), prior);
    d.send(&f, "/continue");
    d.pump(&f).await;
    assert_eq!(
        d.state().terminal_reason.unwrap().as_str(),
        "rounds_complete"
    );
    let id = d.state().contest_id;
    let out = squad_core::export::export(&d.store, ExportScope::Readback, Some(id)).unwrap();
    assert!(out.contains("last rewrite was not read back"));
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, &format!("@all $readback {SPEC}"));
    d.pump(&f).await;
    d.send(&f, "1b");
    d.send(&f, "/stop");
    assert_eq!(d.state().state, ReadbackStatus::Stopped);
    let id = d.state().contest_id;
    let out = squad_core::export::export(&d.store, ExportScope::Readback, Some(id)).unwrap();
    assert!(out.contains("Retain forever"));
    assert!(out.contains("Chosen meaning: OPEN"));
}
#[tokio::test]
async fn direct_literal_single_letter_is_not_a_lettered_choice(){
 let f=Fixture::new().await;let d=Direct::new(&f);d.send(&f,&format!("@all $readback {SPEC}"));d.pump(&f).await;d.send(&f,"1a");d.send(&f,"/answer 1: b");d.send(&f,"/stop");
 let id=d.state().contest_id;let run=d.store.call(move|db|squad_store::readback::run(db,id)).unwrap();assert_eq!(run.rounds[0].choices.as_ref().unwrap().answers["retention"],"b");
}
#[tokio::test]
async fn direct_skipped_members_quorum_and_judge_exhaustion(){
 for (marker,reason) in [("SKIP_ONE CONVERGE","converged"),("QUORUM","insufficient_readings"),("FAIL_JUDGE","judge_attempts_exhausted")] {
 let f=Fixture::new().await;let d=Direct::new(&f);d.send(&f,&format!("@all $readback {SPEC} {marker}"));d.pump(&f).await;
 let s=d.state();assert!(s.state.is_terminal(),"{s:?}");assert_eq!(s.terminal_reason.unwrap().as_str(),reason);
 let run=d.store.call(move|db|squad_store::readback::run(db,s.contest_id)).unwrap();if marker.contains("SKIP_ONE"){assert_eq!(run.rounds[0].skipped.len(),1);assert_eq!(run.rounds[0].interpretations.len(),5);}
 }
}
