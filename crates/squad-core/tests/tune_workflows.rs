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
    async fn scenario(count: usize, scenario: Scenario) -> Self {
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
        config.agents = (0..count)
            .map(|i| config::AgentConfig {
                nick: nick(&format!("member{i}")),
                model: format!("fixture/member{i}"),
                persona: format!("PERSONA_{i}"),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = "fixture/judge".into();
        config.budget.daily_max_usd = 20.;
        config.budget.debate_confirm_usd = 100.;
        let fake = FakeOpenRouter::start().await;
        let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
        fake.catalog(serde_json::json!({"data":models})).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Answers {
                scenario,
                calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                executions: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            })
            .mount(&fake.server)
            .await;
        Self {
            _dir: dir,
            config,
            fake,
        }
    }
    async fn open(&self, mode: OpenMode) -> RoomHandle {
        RoomHandle::open_with_launcher(
            self.config.clone(),
            "main",
            mode,
            OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
            &TuneLauncher,
        )
        .await
        .unwrap()
    }
    fn admit(&self, named: bool) -> RequestResult {
        let id = EventId::new();
        let raw = "@all $tune PROMPT answer clearly\n--\ntask";
        let input = UserMessageInput {
            message_id: id.to_string().parse().unwrap(),
            text: Text::new(raw).unwrap(),
            quote: None,
            reply_to: None,
        };
        let participants = self.config.agents.iter().map(|a| a.nick.clone()).collect();
        let parsed = ParsedInput::Tune {
            input: parser::ConversationalInput {
                raw: raw.into(),
                quote: vec![],
                variable_updates: vec![],
            },
            participants,
            prompt: parser::PlanSource::Pasted {
                text: "PROMPT answer clearly".into(),
            },
            task: "provide helpful concise answers".into(),
            executor: named.then(|| nick("member0")),
            generations: 1,
        };
        let bindings = self
            .config
            .agents
            .iter()
            .map(|a| {
                (
                    a.nick.to_string(),
                    ExecutorBinding {
                        model: Text::new(&a.model).unwrap(),
                        effort: ReasoningEffort::Low,
                    },
                )
            })
            .chain(std::iter::once((
                "judge".into(),
                ExecutorBinding {
                    model: Text::new(&self.config.judge.model).unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )))
            .collect();
        squad_store::commit::commit(&mut self.db(), id, "tune-test", "fixture", |tx, g| {
            squad_core::router::tune::prepare(
                tx,
                g,
                id,
                &input,
                &parsed,
                &squad_core::router::NewModeAdmission {
                    bindings: &bindings,
                    config: Some(&self.config),
                    daily_max: 20_000_000,
                    confirmation_threshold: 100.,
                    preconfirmed: true,
                },
            )
        })
        .unwrap()
        .result
    }
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    fn count(&self, kind: &str) -> i64 {
        self.db()
            .query_row("SELECT COUNT(*) FROM events WHERE kind=?", [kind], |r| {
                r.get(0)
            })
            .unwrap()
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

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Happy,
    Perfect,
    NoImprovement,
    HoldoutReject,
    LeakRepair,
    BadMember,
    Insufficient,
    FreezeFail,
    ScoreFail,
    ExecuteFail,
    HoldoutMissing,
    Incomplete,
    EmptyGeneration,
    Slow,
    Pause(&'static str),
}
#[derive(Clone)]
struct Answers {
    scenario: Scenario,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    executions: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
fn streaming(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(format!("data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",serde_json::json!({"id":"fixture-gen","model":"fixture/model","choices":[{"index":0,"delta":{"content":text}}]}),serde_json::json!({"id":"fixture-gen","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":30,"completion_tokens":30,"total_tokens":60,"cost":0.0001}})))
}
fn input_from(body: &serde_json::Value) -> Option<serde_json::Value> {
    let user = body["messages"].as_array()?.last()?["content"].as_str()?;
    user.char_indices()
        .filter(|(_, c)| *c == '{')
        .find_map(|(i, _)| {
            serde_json::Deserializer::from_str(&user[i..])
                .into_iter::<serde_json::Value>()
                .next()
                .and_then(Result::ok)
                .filter(|v| v.get("step").is_some())
        })
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        use serde_json::json;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let Some(input) = input_from(&body) else {
            if body["messages"].as_array().unwrap().len() == 2
                && body["messages"][0]["content"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("PROMPT")
            {
                let execution = self
                    .executions
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mutation = body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("improved");
                if self.scenario == Scenario::ExecuteFail
                    || (self.scenario == Scenario::HoldoutMissing && execution >= 12)
                    || (self.scenario == Scenario::Incomplete && mutation)
                {
                    return ResponseTemplate::new(500);
                }
                let improved = body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("improved");
                let response = streaming(if improved {
                    "IMPROVED output"
                } else {
                    "BASELINE output"
                });
                return if matches!(self.scenario, Scenario::Pause("execute")) {
                    response.set_delay(Duration::from_millis(300))
                } else {
                    response
                };
            }
            return streaming("startup answer");
        };
        let stage = input["step"].as_str().unwrap();
        let answer = match stage {
            "cases" => {
                if self.scenario == Scenario::Insufficient
                    || (self.scenario == Scenario::BadMember && body["model"] == "fixture/member0")
                {
                    return streaming("{}");
                }
                json!({"cases":[{"localId":"a","kind":"typical","input":"SECRET_CASE one two three four five six seven","expect":["SECRET_EXPECT useful answer"],"mustNot":["SECRET_FORBIDDEN fabrication"]},{"localId":"b","kind":"edge","input":"SECRET_EDGE input","expect":["SECRET_EXPECT concise"],"mustNot":[]}],"criteria":[{"localId":"r","name":"Correctness","description":"SECRET_RUBRIC correct response to every requirement"},{"localId":"s","name":"Conciseness","description":"SECRET_RUBRIC avoid padding"}]})
            }
            "freeze" => {
                if self.scenario == Scenario::FreezeFail {
                    return streaming("{}");
                }
                let proposals = input["proposals"].as_array().unwrap();
                let selected: Vec<_> = proposals
                    .iter()
                    .take(3)
                    .flat_map(|p| {
                        [
                            format!("{}:a", p["label"].as_str().unwrap()),
                            format!("{}:b", p["label"].as_str().unwrap()),
                        ]
                    })
                    .collect();
                let dropped:Vec<_>=proposals.iter().skip(3).flat_map(|p|[json!({"caseRef":format!("{}:a",p["label"].as_str().unwrap()),"why":"duplicate"}),json!({"caseRef":format!("{}:b",p["label"].as_str().unwrap()),"why":"duplicate"})]).collect();
                json!({"cases":selected,"dropped":dropped,"rubric":[{"name":"Correctness","description":"SECRET_RUBRIC correct response to every requirement","sourceRefs":["P1:r"]},{"name":"Conciseness","description":"SECRET_RUBRIC avoid padding","sourceRefs":["P1:s"]}]})
            }
            "mutate" => {
                if self.scenario == Scenario::EmptyGeneration {
                    return streaming("{}");
                }
                json!({"prompt":"PROMPT improved and concise; handle uncertainty.","changes":[{"what":"Add precision","why":"Avoid incomplete answers","targets":[input["digest"]["cases"][0]["caseLabel"].clone()]}]})
            }
            "score" => {
                if self.scenario == Scenario::ScoreFail {
                    return streaming("{}");
                }
                let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let rows:Vec<_>=input["cases"].as_array().unwrap().iter().map(|c|{
                    let outputs:Vec<_>=c["outputs"].as_array().unwrap().iter().map(|o|{
                        let improved=o["output"].as_str().unwrap().contains("IMPROVED");let holdout=c["caseLabel"].as_str().unwrap().starts_with('H');
                        let score=if self.scenario==Scenario::Perfect || (improved&&self.scenario!=Scenario::NoImprovement&&!(holdout&&self.scenario==Scenario::HoldoutReject)){4}else{2};
                        json!({"label":o["label"],"scores":[{"criterion":"Correctness","score":score},{"criterion":"Conciseness","score":score}],"note":if self.scenario==Scenario::LeakRepair&&n==0{"SECRET_RUBRIC correct response to every requirement"}else{"Partially addresses requested behavior"}})
                    }).collect();json!({"caseLabel":c["caseLabel"],"outputs":outputs})
                }).collect();
                json!({"cases":rows})
            }
            _ => panic!("unknown stage {stage}"),
        };
        let response = streaming(&answer.to_string());
        if matches!(self.scenario,Scenario::Pause(p) if p==stage) {
            response.set_delay(Duration::from_millis(300))
        } else if self.scenario == Scenario::Slow && stage == "cases" {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}
async fn run(scenario: Scenario) -> (Fixture, RoomHandle, ContestId) {
    let f = Fixture::scenario(6, scenario).await;
    let room = f.open(OpenMode::Interactive).await;
    let receipt = f.admit(false);
    assert!(
        matches!(receipt, RequestResult::Accepted { .. }),
        "{receipt:?}"
    );
    let id: String = f
        .db()
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='tune'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    f.wait(|| {
        f.db()
            .query_row("SELECT active=0 FROM contests WHERE kind='tune'", [], |r| {
                r.get::<_, bool>(0)
            })
            .unwrap_or(false)
    })
    .await;
    (f, room, id.parse().unwrap())
}
#[tokio::test]
async fn complete_blind_tune_reuses_outputs_and_balances_ledger() {
    let (f, room, id) = run(Scenario::Happy).await;
    let run = squad_store::tune::load(&f.db(), id).unwrap();
    assert_eq!(run.report.as_ref().unwrap().reason, "tuned", "{run:#?}");
    assert_eq!(run.candidates.len(), 4);
    assert_eq!(run.sheets.len(), 3);
    assert_eq!(run.setup.generations, 1);
    let requests = f.fake.server.received_requests().await.unwrap();
    let mut pairs = std::collections::BTreeSet::new();
    for r in requests.iter().filter(|r| r.method == "POST") {
        let b: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if let Some(input) = input_from(&b) {
            let text = b.to_string();
            match input["step"].as_str().unwrap() {
                "mutate" => {
                    for forbidden in [
                        "SECRET_CASE",
                        "SECRET_EXPECT",
                        "SECRET_FORBIDDEN",
                        "SECRET_RUBRIC",
                        "BASELINE output",
                        "IMPROVED output",
                        "H1",
                        "PERSONA_",
                    ] {
                        assert!(!text.contains(forbidden), "leak {forbidden}");
                    }
                }
                "score" => {
                    for forbidden in ["PROMPT", "member0", "PERSONA_", "generation", "champion"] {
                        assert!(
                            !input.to_string().contains(forbidden),
                            "score leak {forbidden}"
                        );
                    }
                }
                _ => {}
            }
        } else if b["messages"][0]["content"]
            .as_str()
            .unwrap_or("")
            .starts_with("PROMPT")
        {
            assert_eq!(b["messages"].as_array().unwrap().len(), 2);
            assert_eq!(b["max_tokens"], 1024);
            assert!(b.get("response_format").is_none());
            // Cases may have equal input text; the durable pair key, not input
            // text, is what must never repeat.
        }
    }
    for e in squad_store::tune::executions(&f.db(), id).unwrap() {
        assert!(pairs.insert((e.candidate_id, e.case_label)));
    }
    let calls:i64=f.db().query_row("SELECT COUNT(*) FROM attempts a JOIN work w USING(work_id) WHERE w.kind='tune.execute'",[],|r|r.get(0)).unwrap();
    assert_eq!(calls, pairs.len() as i64);
    assert!(calls <= 21);
    let export = room.export(ExportScope::Tune, Some(id)).await.unwrap();
    assert!(export.contains("Tuned:"));
    assert!(export.contains("Reusable suite"));
    assert!(export.contains("SECRET_CASE"));
    assert!(export.contains("maps"));
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn early_ends_and_failures_are_explicit() {
    for (scenario, reason) in [
        (Scenario::Perfect, "nothing_to_fix"),
        (Scenario::NoImprovement, "no_improvement"),
        (Scenario::HoldoutReject, "original_kept"),
        (Scenario::Insufficient, "insufficient_cases"),
        (Scenario::FreezeFail, "judge_failed"),
        (Scenario::ScoreFail, "judge_failed"),
        (Scenario::ExecuteFail, "insufficient_cases"),
        (Scenario::HoldoutMissing, "holdout_unavailable"),
        (Scenario::Incomplete, "no_improvement"),
        (Scenario::EmptyGeneration, "no_improvement"),
    ] {
        let (f, room, id) = run(scenario).await;
        let run = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(run.report.as_ref().unwrap().reason, reason);
        if scenario == Scenario::HoldoutMissing {
            let report = squad_core::export::tune::report(&run);
            for c in &run.suite.as_ref().unwrap().holdout {
                assert!(report.contains(&format!("Dropped held-out case {}", c.label)));
            }
        }

        if matches!(scenario, Scenario::Perfect | Scenario::NoImprovement) {
            assert!(run.holdout.is_none());
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn invalid_notes_repair_once_and_skipped_member_keeps_quorum() {
    for scenario in [Scenario::LeakRepair, Scenario::BadMember] {
        let (f, room, id) = run(scenario).await;
        let run = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(run.report.unwrap().reason, "tuned");
        if scenario == Scenario::LeakRepair {
            let repairs: i64 = f
                .db()
                .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
                .unwrap();
            assert_eq!(repairs, 1);
        } else {
            assert_eq!(run.proposals.len(), 5);
            assert_eq!(run.skipped.len(), 1);
        }
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn named_executor_is_removed_and_stop_fences_work() {
    let f = Fixture::scenario(4, Scenario::Slow).await;
    let room = f.open(OpenMode::Interactive).await;
    let receipt = f.admit(true);
    assert!(
        matches!(receipt, RequestResult::Accepted { .. }),
        "{receipt:?}"
    );
    f.wait(|| f.count("work.state") > 0).await;
    let id: String = f
        .db()
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='tune'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let run = squad_store::tune::load(&f.db(), id.parse().unwrap()).unwrap();
    assert_eq!(run.setup.participants.len(), 3);
    assert!(!run.setup.participants.contains(&nick("member0")));
    assert_eq!(run.setup.executor.model.as_str(), "fixture/member0");
    let held = room
        .submit(request(&f.config, "@member1 follow up after tune"))
        .await;
    assert!(matches!(held, RequestResult::Accepted { .. }));
    assert!(f.db().query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND held_by_contest_id=? AND runnable=0)",[id.clone()],|r|r.get::<_,bool>(0)).unwrap());
    let _ = room.submit(request(&f.config, "/stop")).await;
    f.wait(|| {
        f.db()
            .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
            .unwrap()
    })
    .await;
    assert_eq!(
        squad_core::router::tune::load_state(&f.db(), id.parse().unwrap())
            .unwrap()
            .state,
        TuneStatus::Stopped
    );
    room.shutdown().await.unwrap();
    f.assert_ledger();
}

// Exercise the owned runner through the supported runtime injection seam. The
// production judge wrapper still needs the integrator's new-mode dispatch arm.
struct TuneLauncher;
struct TuneRuntime {
    cancel: tokio_util::sync::CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl squad_core::room::RoomRuntime for TuneRuntime {
    fn ready(&mut self) -> squad_core::room::RoomFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn wake(&self, _: &[WorkId]) {}
    fn shutdown(self: Box<Self>) -> squad_core::room::RoomFuture<'static, ()> {
        Box::pin(async move {
            self.cancel.cancel();
            self.task.await.unwrap();
            Ok(())
        })
    }
}
impl squad_core::room::RuntimeLauncher for TuneLauncher {
    fn recover_on_open_mode(
        &self,
        store: &squad_store::Store,
        mode: OpenMode,
        now: DateTime,
    ) -> Result<(), squad_core::room::RoomError> {
        squad_core::router::recovery::recover_on_open_mode(store, mode, now)
            .map(|_| ())
            .map_err(Into::into)
    }
    fn start_with_mode(
        &self,
        config: &SquadConfig,
        store: squad_store::Store,
        bus: squad_core::bus::InProcessBus,
        client: OpenRouterClient,
        _resolution: squad_agents::resolver::ResolutionSnapshot,
        _mode: OpenMode,
    ) -> Result<Box<dyn squad_core::room::RoomRuntime>, squad_core::room::RoomError> {
        let config = config.clone();
        let cancel = tokio_util::sync::CancellationToken::new();
        let stop = cancel.clone();
        let task = tokio::spawn(async move {
            let pack = std::sync::Arc::new(
                squad_core::prompts::PromptPack::from_override_dir(None).unwrap(),
            );
            let roster = config.agents.iter().map(|a| a.nick.clone()).collect();
            let mut router =
                squad_core::router::Router::new(store.clone(), roster, BTreeMap::new(), 20_000_000)
                    .unwrap();
            router.engine_config = Some(config.clone());
            loop {
                if stop.is_cancelled() {
                    break;
                }
                squad_core::orchestrator::tune::drive(&mut router).unwrap();
                let next=store.call(|db|{let mut q=db.prepare("SELECT work_id FROM work WHERE status='queued' AND runnable=1 AND kind LIKE 'tune.%' ORDER BY rowid LIMIT 1")?;let ids=q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;Ok(ids.first().cloned())}).unwrap();
                if let Some(raw) = next {
                    let wid: WorkId = raw.parse().unwrap();
                    let (work, _) = store
                        .call(move |db| Ok(squad_store::work::template(db, wid)?.unwrap()))
                        .unwrap();
                    let owner = match &work.target {
                        Target::Member(n) => ExecutorKey::Member(n.clone()),
                        Target::Judge => ExecutorKey::Case(work.case_id.unwrap()),
                    };
                    let epoch = store
                        .call(|db| {
                            Ok(db.query_row("SELECT epoch FROM room_meta", [], |r| {
                                r.get::<_, i64>(0)
                            })?)
                        })
                        .unwrap();
                    let context = squad_core::runtime::contexts::agent_context(
                        store.clone(),
                        &bus,
                        owner,
                        epoch as u64,
                        pack.clone(),
                        stop.child_token(),
                    );
                    if let RequestResult::Internal(InternalResult::Claimed(grant)) = context
                        .submit
                        .submit(ExecutorRequest::Claim { work_id: wid })
                        .await
                    {
                        let Ok(grant) = squad_agents::worker::start_attempt(
                            &context,
                            &grant,
                            grant.work.attempt,
                        )
                        .await
                        else {
                            continue;
                        };
                        let result = squad_core::runtime::tune::execute(
                            &context,
                            &grant,
                            &client,
                            &pack,
                            &config,
                            stop.child_token(),
                        )
                        .await;
                        assert!(
                            result.is_ok() || stop.is_cancelled(),
                            "owned runtime failed: {result:?}"
                        );
                    }
                } else {
                    tokio::select! {_=stop.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(2))=>{}}
                }
            }
        });
        Ok(Box::new(TuneRuntime { cancel, task }))
    }
}
#[tokio::test]
async fn restart_keeps_seed_suite_and_accepted_execution_pairs() {
    for (phase, state) in [
        ("cases", "BUILDING_CASES"),
        ("freeze", "FREEZING"),
        ("execute", "EXECUTING"),
        ("score", "SCORING"),
        ("mutate", "MUTATING"),
    ] {
        let f = Fixture::scenario(6, Scenario::Pause(phase)).await;
        let room = f.open(OpenMode::Interactive).await;
        f.admit(false);
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT json_extract(state_json,'$.state')=? FROM contests",
                    [state],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false)
        })
        .await;
        let id: String = f
            .db()
            .query_row("SELECT contest_id FROM contests", [], |r| r.get(0))
            .unwrap();
        let id: ContestId = id.parse().unwrap();
        let before = squad_store::tune::load(&f.db(), id).unwrap();
        let accepted = squad_store::tune::executions(&f.db(), id).unwrap();
        room.shutdown().await.unwrap();
        let room = f.open(OpenMode::Interactive).await;
        f.wait(|| {
            f.db()
                .query_row("SELECT active=0 FROM contests", [], |r| r.get::<_, bool>(0))
                .unwrap_or(false)
        })
        .await;
        let after = squad_store::tune::load(&f.db(), id).unwrap();
        assert_eq!(before.setup, after.setup);
        if let Some(suite) = before.suite {
            assert_eq!(suite, after.suite.unwrap());
        }
        for e in accepted {
            assert_eq!(
                squad_store::tune::execution(&f.db(), id, &e.candidate_id, &e.case_label).unwrap(),
                Some(e)
            );
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
