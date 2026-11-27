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
fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}
#[derive(Clone)]
struct Answers;
fn extract(text: &str) -> serde_json::Value {
    for (i, c) in text.char_indices() {
        if c == '{'
            && let Some(Ok(v)) = serde_json::Deserializer::from_str(&text[i..])
                .into_iter::<serde_json::Value>()
                .next()
            && v.get("input").is_some()
        {
            return v["input"].clone();
        }
    }
    panic!("no input");
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        let data = extract(text);
        let brief = data["brief"].as_str().unwrap();
        let schema = &body["response_format"]["json_schema"]["schema"]["properties"];
        let repair = text.contains("failed validation");
        if (brief.contains("SKIP_ONE") && body["model"] == "fixture/member5"
            || brief.contains("QUORUM") && body["model"] != "fixture/member0")
            && schema.get("hypotheses").is_some()
            && schema["hypotheses"]["items"]
                .get("$ref")
                .is_some_and(|v| v.as_str().unwrap_or("").ends_with("DebuggingHypothesis"))
        {
            return ResponseTemplate::new(503);
        }
        let output = if data.get("tables").is_some() {
            if brief.contains("FAIL_JUDGE") || brief.contains("REPAIR_MERGE") && !repair {
                serde_json::json!({})
            } else {
                let tables = data["tables"].as_object().unwrap();
                serde_json::json!({"hypotheses":[{"hypothesisId":"H1","cause":"DNS failure","sources":tables.keys().map(|k|format!("{k}:one")).collect::<Vec<_>>(),"supporting":[],"contradicting":[],"priorSpread":["high"]},{"hypothesisId":"H2","cause":"Server unavailable","sources":tables.keys().map(|k|format!("{k}:two")).collect::<Vec<_>>(),"supporting":[],"contradicting":[],"priorSpread":["low"]}]})
            }
        } else if data.get("proposals").is_some() {
            let proposals: std::collections::BTreeMap<
                String,
                squad_protocol::debugging::DebuggingTestProposal,
            > = serde_json::from_value(data["proposals"].clone()).unwrap();
            let tests = squad_core::orchestrator::debugging::tests(&proposals);
            let (key, t) = tests.first_key_value().unwrap();
            let none = brief.contains("NO_SPLIT");
            serde_json::json!({"testRef":if none {None}else{Some(key)},"instruction":if none{""}else{&t.procedure},"outcomes":if none{vec![]}else{t.outcomes.iter().enumerate().map(|(i,o)|serde_json::json!({"label":format!("O{}",i+1),"description":o})).collect::<Vec<_>>()},"predictions":if none{vec![]}else{t.predictions.iter().map(|p|serde_json::json!({"hypothesisId":p.hypothesis_id,"outcome":if p.outcome.as_ref()==t.outcomes.first(){"O1"}else{"O2"}})).collect::<Vec<_>>()},"addedHypotheses":[],"notSelected":tests.keys().filter(|k|none || *k!=key).map(|k|serde_json::json!({"testRef":k,"why":"Similar test"})).collect::<Vec<_>>()})
        } else if data.get("patchAllowed").is_some() {
            serde_json::json!({"summary":"Investigated the failure","nextTest":"Check again","patch":if data["patchAllowed"]==true {Some("Correct the DNS configuration")}else{None},"patchRationale":if data["patchAllowed"]==true{"Addresses DNS"}else{"No patch: the evidence does not yet isolate a cause."},"risks":[]})
        } else if data.get("differential").is_some() {
            let ids = data["differential"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|h| h["status"] == "live")
                .map(|h| h["hypothesis"]["hypothesisId"].as_str().unwrap())
                .collect::<Vec<_>>();
            serde_json::json!({"tests":if brief.contains("NO_TESTS"){vec![]}else{vec![serde_json::json!({"localId":"test","test":"Check DNS","procedure":"Read the resolver response","effort":"minutes","safety":"read_only","outcomes":["DNS fails","DNS works"],"predictions":ids.iter().enumerate().map(|(i,h)|serde_json::json!({"hypothesisId":h,"outcome":if i==0 || brief.contains("NO_SPLIT"){"DNS fails"}else{"DNS works"},"cannotPredictReason":null})).collect::<Vec<_>>()})]},"newHypothesis":null})
        } else if brief.contains("REPAIR_MEMBER") && !repair || brief.contains("FAIL_MEMBERS") {
            serde_json::json!({"wrong":true})
        } else {
            serde_json::json!({"hypotheses":[{"localId":"one","cause":"DNS failure","supporting":[],"contradicting":[],"prior":"high"},{"localId":"two","cause":"Server unavailable","supporting":[],"contradicting":[],"prior":"low"}]})
        };
        streaming(&output.to_string(), "stop")
    }
}
// Drive leased calls deterministically alongside the real RoomHandle workflows.
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
    fn send(&self, _f: &Fixture, text: &str) {
        let mut router = self.router.lock().unwrap();
        let ctx = router.parse_context().unwrap();
        let meta = Default::default();
        let parsed = parse(text, &meta, &ctx).unwrap();
        let request =
            Request::from_parsed_with_context(EventId::new(), parsed, &meta, &ctx, true).unwrap();
        let result = router.process(request, None).unwrap().result;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
    }
    fn state(&self) -> DebuggingState {
        self.store
            .call(|db| {
                let raw: String = db.query_row(
                    "SELECT state_json FROM contests WHERE kind='debug'",
                    [],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&raw)?)
            })
            .unwrap()
    }
    async fn pump(&self, f: &Fixture) {
        self.pump_until(f, None).await;
    }
    async fn pump_until(&self, f: &Fixture, before_call: Option<DebuggingStatus>) {
        for _ in 0..30 {
            squad_core::orchestrator::debugging::drive(&mut self.router.lock().unwrap()).unwrap();
            let state = self.state();
            if before_call == Some(state.state) && state.step_id.is_some() {
                return;
            }
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
                squad_core::runtime::debugging::execute(
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
async fn human_outcome_isolation_accounting_restart_and_export() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, "@all $debug Network requests fail intermittently.");
    let attempts = d.store.call(|db| {
        let raw: String = db.query_row("SELECT plan_json FROM contests", [], |r| r.get(0))?;
        let p: ContestPreflight = serde_json::from_str(&raw)?;
        assert_eq!(p.estimate.model_calls, 29);
        assert_eq!(p.settings["reservedAttempts"], 63);
        Ok(())
    });
    attempts.unwrap();
    d.pump(&f).await;
    assert_eq!(d.state().state, DebuggingStatus::AwaitingHuman);
    let pending = d
        .store
        .call(|db| squad_store::contest_steps::pending_human(db))
        .unwrap()
        .unwrap();
    let id = d.state().contest_id;
    squad_core::router::recovery::recover_on_open(&d.store).unwrap();
    let pending2 = d
        .store
        .call(|db| squad_store::contest_steps::pending_human(db))
        .unwrap()
        .unwrap();
    assert_eq!(pending, pending2);
    let before = f.fake.captured_bodies().await.len();
    d.pump(&f).await;
    assert_eq!(f.fake.captured_bodies().await.len(), before);
    d.send(&f, "/answer O1 resolver returned no address");
    d.pump(&f).await;
    assert_eq!(d.state().state, DebuggingStatus::Complete);
    let run = d
        .store
        .call(move |db| squad_store::debugging::run(db, id))
        .unwrap();
    assert!(squad_core::orchestrator::debugging::patch_allowed(
        &run.differential
    ));
    assert_eq!(run.reason.as_deref(), Some("isolated"));
    assert_eq!(
        run.rounds[0].result.as_ref().unwrap().evidence,
        "resolver returned no address"
    );
    d.store
        .call(|db| {
            let reserve: i64 = db.query_row(
                "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(reserve, 0);
            let spent: i64 =
                db.query_row("SELECT SUM(spent_micros) FROM spend_daily", [], |r| {
                    r.get(0)
                })?;
            let costs: i64 =
                db.query_row("SELECT SUM(reported_cost_micros) FROM attempts", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(spent, costs);
            Ok(())
        })
        .unwrap();
    let exported = squad_core::export::export(&d.store, ExportScope::Debugging, Some(id)).unwrap();
    for field in [
        "resolver returned no address",
        "DIFFERENTIAL",
        "isolated",
        "Correct the DNS configuration",
    ] {
        assert!(exported.contains(field), "{field}");
    }
    let bodies = f.fake.captured_bodies().await;
    for b in bodies {
        let text = b["messages"][1]["content"].as_str().unwrap();
        let data = extract(text);
        if data.get("tables").is_none()
            && data.get("proposals").is_none()
            && data.get("patchAllowed").is_none()
        {
            assert!(data.get("tables").is_none());
            assert!(data.get("proposals").is_none());
        }
        if data.get("tables").is_some() || data.get("proposals").is_some() {
            assert!(!text.contains("PERSONA_"));
            assert!(!b["messages"].to_string().contains("@fable"));
            assert!(!text.contains("fixture/member"));
        }
    }
}
#[tokio::test]
async fn unknown_other_continue_and_round_limit_do_not_allow_a_patch() {
    for answer in [
        "/answer unknown",
        "/answer other unexpected response",
        "/continue",
    ] {
        let f = Fixture::new().await;
        let d = Direct::new(&f);
        d.send(&f, "@all $debug Network requests fail intermittently.");
        for _ in 0..3 {
            d.pump(&f).await;
            assert_eq!(d.state().state, DebuggingStatus::AwaitingHuman);
            d.send(&f, answer);
        }
        d.pump(&f).await;
        assert_eq!(d.state().state, DebuggingStatus::Complete);
        let id = d.state().contest_id;
        let run = d
            .store
            .call(move |db| squad_store::debugging::run(db, id))
            .unwrap();
        assert_eq!(run.reason.as_deref(), Some("rounds_complete"));
        assert!(run.differential.iter().all(|h| h.status
            == squad_protocol::debugging::DebuggingHypothesisStatus::Live
            && h.consistent_results == 0));
        assert!(run.narrative.unwrap().patch.is_none());
    }
}
#[tokio::test]
async fn repairs_skips_quorum_and_loop_endings() {
    for (marker, reason, state) in [
        (
            "REPAIR_MEMBER REPAIR_MERGE",
            "isolated",
            DebuggingStatus::Complete,
        ),
        (
            "FAIL_MEMBERS",
            "insufficient_hypotheses",
            DebuggingStatus::Failed,
        ),
        ("FAIL_JUDGE", "judge_failed", DebuggingStatus::Failed),
        ("NO_TESTS", "no_tests", DebuggingStatus::Complete),
        (
            "NO_SPLIT",
            "no_discriminating_test",
            DebuggingStatus::Complete,
        ),
        ("SKIP_ONE", "isolated", DebuggingStatus::Complete),
        ("QUORUM", "insufficient_hypotheses", DebuggingStatus::Failed),
    ] {
        let f = Fixture::new().await;
        let d = Direct::new(&f);
        d.send(
            &f,
            &format!("@all $debug Network requests fail intermittently. {marker}"),
        );
        d.pump(&f).await;
        if d.state().state == DebuggingStatus::AwaitingHuman {
            d.send(&f, "/answer O1 resolver failed");
            d.pump(&f).await;
        }
        assert_eq!(d.state().state, state, "{marker}");
        assert_eq!(
            d.state().terminal_reason.as_ref().map(|t| t.as_str()),
            Some(reason),
            "{marker}"
        );
    }
}
#[tokio::test]
async fn stop_fences_queued_work_and_preserves_partial_evidence() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, "@all $debug Network requests fail intermittently.");
    d.pump_until(&f, Some(DebuggingStatus::Selecting)).await;
    d.send(&f, "/stop");
    assert_eq!(d.state().state, DebuggingStatus::Stopped);
    let before = f.fake.captured_bodies().await.len();
    d.pump(&f).await;
    assert_eq!(before, f.fake.captured_bodies().await.len());
    d.store
        .call(|db| {
            let queued: i64 = db.query_row(
                "SELECT COUNT(*) FROM work WHERE status IN ('queued','running')",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(queued, 0);
            Ok(())
        })
        .unwrap();
}

#[tokio::test]
async fn last_consumed_attempt_after_restart_settles_without_another_call() {
    let f = Fixture::new().await;
    let d = Direct::new(&f);
    d.send(&f, "@all $debug Network requests fail intermittently.");
    d.pump_until(&f, Some(DebuggingStatus::Merging)).await;
    d.store.call(|db|{db.execute("UPDATE work SET attempt=3,status='queued',lease_owner=NULL,lease_until=NULL WHERE kind='debug.merge'",[])?;Ok(())}).unwrap();
    squad_core::router::recovery::recover_on_open(&d.store).unwrap();
    let before = f.fake.captured_bodies().await.len();
    d.pump(&f).await;
    assert_eq!(before, f.fake.captured_bodies().await.len());
    assert_eq!(d.state().state, DebuggingStatus::Failed);
    assert_eq!(d.state().terminal_reason.unwrap().as_str(), "judge_failed");
}
