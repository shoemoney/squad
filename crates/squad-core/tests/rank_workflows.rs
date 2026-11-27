#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::*;
use squad_agents::openrouter::OpenRouterClient;
use squad_protocol::*;
use std::collections::BTreeMap;
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
}
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
        let id = EventId::new();
        let result = if text.starts_with("@all $rank ") {
            let (criterion, items) = text
                .trim_start_matches("@all $rank ")
                .split_once("\n--\n")
                .unwrap();
            let parsed = ParsedInput::Rank {
                input: parser::ConversationalInput {
                    raw: text.into(),
                    quote: vec![],
                    variable_updates: vec![],
                },
                participants: ctx.roster.clone(),
                criterion: criterion.into(),
                items: items.lines().map(str::to_owned).collect(),
            };
            let request = Request::from_parsed_with_context(id, parsed, &meta, &ctx, true).unwrap();
            let Request::UserMessage { input, parsed, .. } = request else {
                panic!()
            };
            let config = f.config.clone();
            let bindings = config
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
                        model: config.judge.model.parse().unwrap(),
                        effort: ReasoningEffort::High,
                    },
                )))
                .collect();
            self.store
                .commit(id, "direct-fixture".into(), id.to_string(), move |tx, g| {
                    squad_core::router::rank::prepare(
                        tx,
                        g,
                        id,
                        &input,
                        &parsed,
                        &squad_core::router::NewModeAdmission {
                            bindings: &bindings,
                            config: Some(&config),
                            daily_max: 200_000_000,
                            confirmation_threshold: 100.0,
                            preconfirmed: true,
                        },
                    )
                })
                .unwrap()
                .result
        } else {
            let parsed = parse(text, &meta, &ctx).unwrap();
            let request = Request::from_parsed_with_context(id, parsed, &meta, &ctx, true).unwrap();
            router.process(request, None).unwrap().result
        };
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
    }
    fn state(&self) -> RankState {
        self.store
            .call(|db| {
                let raw: String = db.query_row(
                    "SELECT state_json FROM contests WHERE kind='rank'",
                    [],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&raw)?)
            })
            .unwrap()
    }
    async fn pump(&self, f: &Fixture) {
        self.pump_limit(f, usize::MAX).await;
    }
    async fn pump_limit(&self, f: &Fixture, limit: usize) {
        let mut calls = 0;
        for _ in 0..30 {
            squad_core::orchestrator::rank::drive(&mut self.router.lock().unwrap()).unwrap();
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
                squad_core::runtime::rank::execute(
                    &context,
                    &grant,
                    &OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap(),
                    &squad_core::prompts::PromptPack::embedded().unwrap(),
                    &f.config,
                    Default::default(),
                )
                .await
                .unwrap();
                calls += 1;
                if calls >= limit {
                    return;
                }
            }
        }
        panic!("mode script did not settle");
    }
}

fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}
#[derive(Clone)]
struct Answers;
fn extract(text: &str) -> serde_json::Value {
    let start = text
        .find("Input:\n")
        .or_else(|| text.find("Anonymous evidence and computed result:\n"))
        .unwrap();
    let tail = &text[start..];
    let brace = tail.find('{').unwrap();
    serde_json::Deserializer::from_str(&tail[brace..])
        .into_iter::<serde_json::Value>()
        .next()
        .unwrap()
        .unwrap()["input"]
        .clone()
}
impl Respond for Answers {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        let data = extract(text);
        let output = if let Some(pairs) = data["pairs"].as_array() {
            serde_json::json!({"choices":pairs.iter().map(|p|serde_json::json!({"pairId":p["pairId"],"winner":"A","reason":"Greater practical impact"})).collect::<Vec<_>>()})
        } else {
            serde_json::json!({"top":data["topIds"].as_array().unwrap().iter().map(|id|serde_json::json!({"itemId":id,"why":"Supported by the pairwise comparisons"})).collect::<Vec<_>>(),"cycles":data["cycleIds"].as_array().unwrap().iter().map(|id|serde_json::json!({"cycleId":id,"reading":"Criterion conflict or noise"})).collect::<Vec<_>>(),"caveats":"Close gaps are uncertain"})
        };
        ResponseTemplate::new(200).set_body_raw(
            stream_body(&output.to_string(), "stop"),
            "text/event-stream",
        )
    }
}
const START: &str =
    "@all $rank prioritize by user impact\n--\nRepair data loss\nImprove search\nRefresh icons";
#[tokio::test]
async fn direct_runtime_completes_seven_calls_and_releases_reservation() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, START);
    d.pump(&f).await;
    assert_eq!(d.state().state, RankStatus::Complete);
    let (batches, judgments, explanation) = d
        .store
        .call(|db| {
            let r = squad_store::rank::run(db, dummy_id(db)?)?;
            Ok((
                r.batches.len(),
                r.tally.unwrap().judgments,
                r.explanation.is_some(),
            ))
        })
        .unwrap();
    assert_eq!((batches, judgments, explanation), (6, 9, true));
    d.store
        .call(|db| {
            let calls: i64 = db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))?;
            let reserved: i64 = db.query_row(
                "SELECT COALESCE(SUM(reserved_micros),0) FROM spend_daily",
                [],
                |r| r.get(0),
            )?;
            assert_eq!((calls, reserved), (7, 0));
            Ok(())
        })
        .unwrap();
    let calls = f.fake.captured_bodies().await;
    assert_eq!(calls.len(), 7);
    for request in calls {
        let b = request;
        if b["model"] == "fixture/judge" {
            let content = b["messages"][1]["content"].as_str().unwrap();
            assert!(!content.contains("PERSONA_") && !content.contains("member0"));
        }
    }
}
fn dummy_id(db: &rusqlite::Connection) -> squad_store::Result<ContestId> {
    let raw: String = db.query_row(
        "SELECT contest_id FROM contests WHERE kind='rank'",
        [],
        |r| r.get(0),
    )?;
    Ok(raw.parse().unwrap())
}
#[tokio::test]
async fn stop_preserves_design_and_fences_queued_work() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, START);
    squad_core::orchestrator::rank::drive(&mut d.router.lock().unwrap()).unwrap();
    let id = d.state().contest_id;
    let setup = d
        .store
        .call(move |db| squad_store::rank::setup(db, id))
        .unwrap();
    d.send(&f, "/stop");
    assert_eq!(d.state().state, RankStatus::Stopped);
    assert_eq!(
        setup,
        d.store
            .call(move |db| squad_store::rank::setup(db, id))
            .unwrap()
    );
    d.store
        .call(|db| {
            let pending: i64 = db.query_row(
                "SELECT COUNT(*) FROM work WHERE status IN ('queued','running')",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(pending, 0);
            Ok(())
        })
        .unwrap();
}
#[tokio::test]
async fn restart_reuses_design_and_accepted_batches() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, START);
    squad_core::orchestrator::rank::drive(&mut d.router.lock().unwrap()).unwrap();
    let id = d.state().contest_id;
    let before = d
        .store
        .call(move |db| squad_store::rank::setup(db, id))
        .unwrap();
    d.pump_limit(&f, 1).await;
    let accepted = d
        .store
        .call(move |db| squad_store::rank::run(db, id))
        .unwrap()
        .batches;
    assert_eq!(accepted.len(), 1);
    squad_core::router::recovery::recover_on_open(&d.store).unwrap();
    d.pump(&f).await;
    let run = d
        .store
        .call(move |db| squad_store::rank::run(db, id))
        .unwrap();
    for (nick, batch) in accepted {
        assert_eq!(run.batches[&nick], batch);
    }
    assert_eq!(f.fake.captured_bodies().await.len(), 7);
    assert_eq!(d.state().state, RankStatus::Complete);
    assert_eq!(
        before,
        d.store
            .call(move |db| squad_store::rank::setup(db, id))
            .unwrap()
    );
}
#[tokio::test]
async fn skipped_members_quorum_preserves_accepted_evidence() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, START);
    squad_core::orchestrator::rank::drive(&mut d.router.lock().unwrap()).unwrap();
    d.store.call(|db|{db.execute("UPDATE work SET status='failed',attempt=2,runnable=0 WHERE target<>'member0' AND kind='rank.compare'",[])?;Ok(())}).unwrap();
    d.pump(&f).await;
    assert_eq!(d.state().state, RankStatus::Failed);
    assert_eq!(
        d.state().terminal_reason.unwrap().as_str(),
        "insufficient_comparisons"
    );
    let id = d.state().contest_id;
    d.store
        .call(move |db| {
            let run = squad_store::rank::run(db, id)?;
            assert_eq!(run.batches.len(), 1);
            assert_eq!(run.skipped.len(), 5);
            Ok(())
        })
        .unwrap();
}
#[tokio::test]
async fn judge_transport_exhaustion_keeps_fitted_order() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"model":"fixture/judge"}),
        ))
        .respond_with(ResponseTemplate::new(503))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let d = Direct::new(&f);
    d.send(&f, START);
    d.pump(&f).await;
    assert_eq!(d.state().state, RankStatus::Failed);
    assert_eq!(d.state().terminal_reason.unwrap().as_str(), "judge_failed");
    let id = d.state().contest_id;
    d.store
        .call(move |db| {
            assert!(squad_store::rank::run(db, id)?.tally.is_some());
            let calls: i64 = db.query_row(
                "SELECT COUNT(*) FROM attempts WHERE target='judge'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(calls, 3);
            Ok(())
        })
        .unwrap();
}
#[derive(Clone)]
struct JudgeRepair(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl Respond for JudgeRepair {
    fn respond(&self, request: &HttpRequest) -> ResponseTemplate {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            streaming("{\"top\":[],\"cycles\":[],\"caveats\":\"\"}", "stop")
        } else {
            Answers.respond(request)
        }
    }
}
#[tokio::test]
async fn judge_gets_one_private_schema_repair() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"model":"fixture/judge"}),
        ))
        .respond_with(JudgeRepair(Default::default()))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let d = Direct::new(&f);
    d.send(&f, START);
    d.pump(&f).await;
    assert_eq!(d.state().state, RankStatus::Complete);
    d.store
        .call(|db| {
            let repairs: i64 =
                db.query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))?;
            assert_eq!(repairs, 1);
            Ok(())
        })
        .unwrap();
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 8);
    assert!(
        bodies.last().unwrap()["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("validation")
    );
}

#[tokio::test]
async fn judge_second_schema_failure_does_not_get_another_repair() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"model":"fixture/judge"}),
        ))
        .respond_with(streaming(
            "{\"top\":[],\"cycles\":[],\"caveats\":\"\"}",
            "stop",
        ))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let d = Direct::new(&f);
    d.send(&f, START);
    d.pump(&f).await;
    assert_eq!(d.state().state, RankStatus::Failed);
    d.store.call(|db|{let (repairs,calls):(i64,i64)=db.query_row("SELECT (SELECT COUNT(*) FROM mode_repairs),(SELECT COUNT(*) FROM attempts WHERE target='judge')",[],|r|Ok((r.get(0)?,r.get(1)?)))?;assert_eq!((repairs,calls),(1,2));Ok(())}).unwrap();
}
