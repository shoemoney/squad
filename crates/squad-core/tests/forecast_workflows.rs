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
    store: Arc<std::sync::Mutex<Option<squad_store::Store>>>,
}
impl Fixture {
    async fn new() -> Self {
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
            store: Arc::new(std::sync::Mutex::new(None)),
        }
    }
    async fn open(&self, mode: OpenMode) -> RoomHandle {
        RoomHandle::open_with_launcher(
            self.config.clone(),
            "main",
            mode,
            OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
            &ForecastLauncher(self.store.clone()),
        )
        .await
        .unwrap()
    }
    async fn start(&self, text: &str) -> RequestResult {
        let store = self.store.lock().unwrap().clone().unwrap();
        let config = self.config.clone();
        let raw = text.to_owned();
        let id = EventId::new();
        store
            .commit(
                id,
                "forecast-fixture".into(),
                id.to_string(),
                move |tx, g| {
                    let participants = config.agents.iter().map(|a| a.nick.clone()).collect();
                    let parsed = ParsedInput::Forecast {
                        input: parser::ConversationalInput {
                            raw: raw.clone(),
                            quote: vec![],
                            variable_updates: vec![],
                        },
                        participants,
                        question: raw.split("$forecast ").nth(1).unwrap().into(),
                        background: None,
                    };
                    let bindings = config
                        .agents
                        .iter()
                        .map(|a| {
                            (
                                a.nick.to_string(),
                                ExecutorBinding {
                                    model: Text::new(a.model.clone()).unwrap(),
                                    effort: ReasoningEffort::Low,
                                },
                            )
                        })
                        .chain(std::iter::once((
                            "judge".into(),
                            ExecutorBinding {
                                model: Text::new(config.judge.model.clone()).unwrap(),
                                effort: constants::NEW_MODE_JUDGE_REASONING_EFFORT,
                            },
                        )))
                        .collect();
                    squad_core::router::forecast::prepare(
                        tx,
                        g,
                        id,
                        &UserMessageInput {
                            message_id: MessageId::new(),
                            text: Text::new(raw).unwrap(),
                            quote: None,
                            reply_to: None,
                        },
                        &parsed,
                        &squad_core::router::NewModeAdmission {
                            bindings: &bindings,
                            config: Some(&config),
                            daily_max: 200_000_000,
                            confirmation_threshold: 100.0,
                            preconfirmed: true,
                        },
                    )
                },
            )
            .unwrap()
            .result
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
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["response_format"]["json_schema"]["name"] != "forecast_response" {
            return streaming("ordinary response", "stop");
        }
        let text = body["messages"][1]["content"].as_str().unwrap();
        let data = extract_data(text);
        let model = body["model"].as_str().unwrap();
        if (text.contains("MISS_REVISION")
            && text.contains("Revise your own")
            && model.ends_with('0'))
            || (text.contains("NO_QUORUM")
                && text.contains("Independently forecast")
                && !model.ends_with('0')
                && !model.ends_with('1'))
        {
            return ResponseTemplate::new(500);
        }
        if text.contains("INVALID_FIRST")
            && text.contains("Independently forecast")
            && model.ends_with('0')
            && !text.contains("Your previous output for")
        {
            return streaming(&serde_json::json!({"probabilityPct":0,"baseRate":{"referenceClass":"Comparable launches","ratePct":null},"factors":[{"direction":"up","text":"a"},{"direction":"down","text":"b"},{"direction":"up","text":"c"}],"rationale":"invalid endpoint"}).to_string(),"stop");
        }
        if text.contains("NO_CRUX") && text.contains("Explain disagreement") {
            return streaming("{\"probabilityPct\":50}", "stop");
        }

        let output = if text.contains("Freeze a forecast") {
            serde_json::json!({"question":data["question"],"resolutionCriteria":"Public production release","resolutionDate":"2027-12-31","resolutionSource":"Official release notes","voidRule":"Void if project ceases","forecastable":!text.contains("NOT_FORECASTABLE"),"reason":"Dated observable event"})
        } else if text.contains("Independently forecast") {
            let model = body["model"].as_str().unwrap();
            let i = model.chars().last().unwrap().to_digit(10).unwrap();
            serde_json::json!({"probabilityPct":20+i*10,"baseRate":{"referenceClass":"Comparable launches: 25 percent, 0.25, 1 in 4, 1 to 3, 1:3; member0 wrote this","ratePct":25},"factors":[{"direction":"up","text":"Team staffed"},{"direction":"down","text":"No external validation"},{"direction":"up","text":"Working prototype"}],"rationale":"Evidence is mixed"})
        } else if text.contains("Revise your own") {
            serde_json::json!({"probabilityPct":55,"changeReason":"Peer reasons persuaded me","citedLabels":data["labels"].as_array().unwrap().iter().take(1).cloned().collect::<Vec<_>>()})
        } else {
            serde_json::json!({"cruxes":[{"text":"Readiness","higherLabels":[data["labels"][0].clone()],"lowerLabels":[data["labels"][1].clone()]}],"signposts":[{"observable":"Public beta","direction":"up","byWhen":"2027-06-30"}],"sharedAssumptions":["The project continues"],"weakEvidence":["No verified reference class"]})
        };
        let response = streaming(&output.to_string(), "stop");
        if (text.contains("SLOW_FRAME") && text.contains("Freeze a forecast"))
            || (text.contains("SLOW_REVISION") && text.contains("Revise your own"))
        {
            response.set_delay(Duration::from_millis(800))
        } else {
            response
        }
    }
}
fn extract_data(text: &str) -> serde_json::Value {
    let data = text.split("Authorized input: ").nth(1).unwrap();
    let mut stream = serde_json::Deserializer::from_str(data).into_iter::<serde_json::Value>();
    stream.next().unwrap().unwrap()
}
fn state(f: &Fixture) -> ForecastState {
    let raw: String = f
        .db()
        .query_row(
            "SELECT state_json FROM contests WHERE kind='forecast' ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
#[tokio::test]
async fn six_members_fourteen_calls_reserves_thirty_and_exports_blind_bundles() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = f
        .start("@all $forecast Will the launch happen by 2027-12-31?")
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| state(&f).state.is_terminal()).await;
    assert_eq!(state(&f).state, ForecastStatus::Complete);
    let raw: String = f
        .db()
        .query_row(
            "SELECT plan_json FROM contests WHERE kind='forecast'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let plan: ContestPreflight = serde_json::from_str(&raw).unwrap();
    assert_eq!(plan.estimate.model_calls, 14);
    assert_eq!(plan.settings["reservedAttempts"], 30);
    let bodies = f.fake.captured_bodies().await;
    let calls = bodies
        .iter()
        .filter(|b| b["response_format"]["json_schema"]["name"] == "forecast_response")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 14);
    for b in calls.iter().filter(|b| {
        b["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Revise your own")
    }) {
        let text = b["messages"][1]["content"].as_str().unwrap();
        let d = extract_data(text);
        assert_eq!(d["others"].as_array().unwrap().len(), 5);
        let others = d["others"].to_string();
        for token in [
            "25 percent",
            "0.25",
            "1 in 4",
            "1 to 3",
            "1:3",
            "member0",
            "ratePct",
            "probabilityPct",
        ] {
            assert!(!others.contains(token), "leaked {token}: {others}");
        }
        let my = b["model"]
            .as_str()
            .unwrap()
            .chars()
            .last()
            .unwrap()
            .to_digit(10)
            .unwrap();
        let whole = b["messages"].to_string();
        for i in 0..6 {
            if i != my {
                assert!(
                    !whole.contains(&format!("member{i}")),
                    "nick leaked in {whole}"
                );
            }
        }
    }
    let rows: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM forecast_records WHERE record_key LIKE 'bundle:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 6);
    let export = room
        .export(ExportScope::Forecast, Some(state(&f).contest_id))
        .await
        .unwrap();
    assert!(export.contains("Pooled probability: **55.0%**"));
    assert!(export.contains("forecast.bundle"));
    assert!(export.contains("trimmedLogOdds"));
    f.assert_ledger();
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(state(&f).state, ForecastStatus::Complete);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn unforecastable_stops_before_members_and_releases_reservations() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    f.start("@all $forecast NOT_FORECASTABLE What is beautiful?")
        .await;
    f.wait(|| state(&f).state.is_terminal()).await;
    assert_eq!(
        state(&f).terminal_reason.unwrap().as_str(),
        "not_forecastable"
    );
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE kind='forecast.estimate'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
// The production legacy judge gate omits new_mode_judge(). Exercise the owned
// executor through the public runtime seam until the integrator fixes that gate.
use squad_core::{
    bus::InProcessBus,
    prompts::PromptPack,
    room::{RoomError, RoomFuture, RoomRuntime, RuntimeLauncher},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
struct ForecastLauncher(Arc<std::sync::Mutex<Option<squad_store::Store>>>);
struct OwnedExecutor {
    client: OpenRouterClient,
    config: SquadConfig,
    pack: PromptPack,
}
impl squad_agents::worker::WorkExecutor for OwnedExecutor {
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> squad_agents::worker::ExecutionFuture<'a> {
        Box::pin(async move {
            squad_core::runtime::forecast::execute(
                context,
                &grant,
                &self.client,
                &self.pack,
                &self.config,
                cancel,
            )
            .await
            .map_err(|_| ReaderError::NotRunnable)
        })
    }
}
struct OwnedRuntime {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl RoomRuntime for OwnedRuntime {
    fn ready(&mut self) -> RoomFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn wake(&self, _: &[WorkId]) {}
    fn shutdown(self: Box<Self>) -> RoomFuture<'static, ()> {
        Box::pin(async move {
            self.cancel.cancel();
            self.task.await.unwrap();
            Ok(())
        })
    }
}
impl RuntimeLauncher for ForecastLauncher {
    fn recover_on_open_mode(
        &self,
        store: &squad_store::Store,
        mode: OpenMode,
        now: DateTime,
    ) -> Result<(), RoomError> {
        squad_core::router::recovery::recover_on_open_mode(store, mode, now)
            .map(|_| ())
            .map_err(Into::into)
    }
    fn start_with_mode(
        &self,
        config: &SquadConfig,
        store: squad_store::Store,
        bus: InProcessBus,
        client: OpenRouterClient,
        _: squad_agents::resolver::ResolutionSnapshot,
        _: OpenMode,
    ) -> Result<Box<dyn RoomRuntime>, RoomError> {
        *self.0.lock().unwrap() = Some(store.clone());
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let config = config.clone();
        let task = tokio::spawn(async move {
            let bindings = config
                .agents
                .iter()
                .map(|a| {
                    (
                        a.nick.to_string(),
                        ExecutorBinding {
                            model: Text::new(a.model.clone()).unwrap(),
                            effort: ReasoningEffort::Low,
                        },
                    )
                })
                .chain(std::iter::once((
                    "judge".into(),
                    ExecutorBinding {
                        model: Text::new(config.judge.model.clone()).unwrap(),
                        effort: constants::NEW_MODE_JUDGE_REASONING_EFFORT,
                    },
                )))
                .collect();
            let mut driver = squad_core::router::Router::new(
                store.clone(),
                config.agents.iter().map(|a| a.nick.clone()).collect(),
                bindings,
                200_000_000,
            )
            .unwrap();
            let pack = PromptPack::embedded().unwrap();
            let executor = Arc::new(OwnedExecutor {
                client,
                config: config.clone(),
                pack: pack.clone(),
            });
            let mut tasks = std::collections::BTreeMap::<
                String,
                tokio::task::JoinHandle<Result<(), ReaderError>>,
            >::new();
            loop {
                squad_core::orchestrator::forecast::drive(&mut driver).unwrap();
                for a in &config.agents {
                    bus.wake_mailbox(&ExecutorKey::Member(a.nick.clone()));
                }
                let finished = tasks
                    .iter()
                    .filter(|(_, t)| t.is_finished())
                    .map(|(k, _)| k.clone())
                    .collect::<Vec<_>>();
                for key in finished {
                    tasks.remove(&key).unwrap().await.unwrap().unwrap();
                }
                let keys=store.call(|db|{let mut q=db.prepare("SELECT DISTINCT case_id FROM work WHERE case_id IS NOT NULL AND status IN ('queued','running')")?;Ok(q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)}).unwrap();
                let mut keys = keys
                    .into_iter()
                    .map(|k| ExecutorKey::Case(k.parse().unwrap()))
                    .collect::<Vec<_>>();
                keys.extend(
                    config
                        .agents
                        .iter()
                        .map(|a| ExecutorKey::Member(a.nick.clone())),
                );
                for key in keys {
                    let owner = serde_json::to_string(&key).unwrap();
                    if let std::collections::btree_map::Entry::Vacant(entry) = tasks.entry(owner) {
                        let context = squad_core::runtime::contexts::agent_context(
                            store.clone(),
                            &bus,
                            key,
                            1,
                            Arc::new(pack.clone()),
                            child.child_token(),
                        );
                        entry.insert(squad_agents::worker::spawn(context, executor.clone()));
                    }
                }
                tokio::select! {_=child.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(10))=>{}}
            }
            for (_, t) in tasks {
                t.await.unwrap().unwrap();
            }
        });
        Ok(Box::new(OwnedRuntime { cancel, task }))
    }
}
#[tokio::test]
async fn member_endpoint_repairs_once_and_missing_revision_is_not_unchanged() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    f.start("@all $forecast INVALID_FIRST MISS_REVISION Will it ship by 2027-12-31?")
        .await;
    f.wait(|| state(&f).state.is_terminal()).await;
    assert_eq!(state(&f).state, ForecastStatus::Complete);
    let db = f.db();
    let repairs:i64=db.query_row("SELECT COUNT(*) FROM mode_repairs r JOIN work w USING(work_id) WHERE w.kind='forecast.estimate'",[],|r|r.get(0)).unwrap();
    assert_eq!(repairs, 1);
    let raw: String = db
        .query_row(
            "SELECT body FROM contest_artifacts WHERE artifact_kind='forecast.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let report: squad_protocol::forecast::ForecastReport = serde_json::from_str(&raw).unwrap();
    let m = report
        .members
        .iter()
        .find(|m| m.member.as_str() == "member0")
        .unwrap();
    assert!(m.not_revised);
    assert!(m.revision.is_none());
    assert_eq!(m.final_probability_pct, m.first.probability_pct);
    let export = room
        .export(ExportScope::Forecast, Some(state(&f).contest_id))
        .await
        .unwrap();
    assert!(export.contains("not revised"));
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn insufficient_quorum_and_failed_crux_keep_only_accepted_evidence() {
    for (marker, reason) in [
        ("NO_QUORUM", "insufficient_forecasts"),
        ("NO_CRUX", "judge_failed"),
    ] {
        let f = Fixture::new().await;
        let room = f.open(OpenMode::Interactive).await;
        f.start(&format!(
            "@all $forecast {marker} Will it ship by 2027-12-31?"
        ))
        .await;
        f.wait(|| state(&f).state.is_terminal()).await;
        assert_eq!(state(&f).terminal_reason.unwrap().as_str(), reason);
        let export = room
            .export(ExportScope::Forecast, Some(state(&f).contest_id))
            .await
            .unwrap();
        if marker == "NO_CRUX" {
            assert!(export.contains("Pooled probability"));
            assert!(export.contains("trimmedLogOdds"));
        } else {
            assert!(!export.contains("Pooled probability"));
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn global_stop_fences_inflight_work_and_restart_keeps_the_frame() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    f.start("@all $forecast SLOW_FRAME Will it ship by 2027-12-31?")
        .await;
    f.wait(|| {
        f.db()
            .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))
            .unwrap()
            > 0
    })
    .await;
    let contest = state(&f).contest_id;
    room.submit(request(&f.config, "/stop")).await;
    assert_eq!(state(&f).state, ForecastStatus::Stopped);
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM contest_artifacts", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(state(&f).contest_id, contest);
    assert_eq!(state(&f).state, ForecastStatus::Stopped);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn restart_reuses_frozen_frame_labels_and_delivered_bundles() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    f.start("@all $forecast SLOW_REVISION Will it ship by 2027-12-31?")
        .await;
    f.wait(|| {
        state(&f).state == ForecastStatus::Revising
            && f.db()
                .query_row(
                    "SELECT COUNT(*) FROM work WHERE kind='forecast.revise' AND status='running'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0
    })
    .await;
    let contest = state(&f).contest_id;
    let frozen = || {
        f.db().prepare("SELECT record_key,body FROM forecast_records WHERE record_key IN ('frame','labels') OR record_key LIKE 'bundle:%' ORDER BY record_key").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
    };
    let before = frozen();
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| state(&f).state.is_terminal()).await;
    assert_eq!(state(&f).state, ForecastStatus::Complete);
    assert_eq!(state(&f).contest_id, contest);
    assert_eq!(before, frozen());
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE kind='forecast.estimate'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        6
    );
    room.shutdown().await.unwrap();
}
