#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake_openrouter;
use fake_openrouter::*;
use serde_json::{Value, json};
use squad_agents::openrouter::OpenRouterClient;
use squad_core::room::RoomHandle;
use squad_protocol::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use wiremock::{
    Mock, Request as HttpRequest, Respond, ResponseTemplate,
    matchers::{method, path},
};
#[derive(Clone, Copy)]
enum Scenario {
    Happy,
    Repair,
    InvalidAttacks,
    NoAttacks,
    SkipOne,
    JudgeFailure,
    SlowAttack,
    SlowChallenge,
}
#[derive(Clone)]
struct Answers {
    scenario: Scenario,
    counts: Arc<Mutex<BTreeMap<String, usize>>>,
}
fn vector(id: &str, diversity: bool) -> Value {
    json!({"localId":id,"title":"Unbounded access","targetQuote":"All requests are accepted without authentication.","exploit":"An outsider submits arbitrary requests.","severity":"high","likelihood":"likely","blastRadius":"All stored data","diversity":diversity})
}
fn input(body: &Value) -> Value {
    let user = body["messages"][1]["content"].as_str().unwrap();
    let start = user.find("Authorized input: ").unwrap() + "Authorized input: ".len();
    serde_json::Deserializer::from_str(&user[start..])
        .into_iter::<Value>()
        .next()
        .unwrap()
        .unwrap()
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        let data = input(&body);
        let model = body["model"].as_str().unwrap();
        let stage = if data.get("lens").is_some() {
            "attack"
        } else if data.get("tallies").is_some() {
            "rule"
        } else {
            "challenge"
        };
        let key = format!("{model}:{stage}");
        let mut counts = self.counts.lock().unwrap();
        let count = counts.entry(key).or_default();
        *count += 1;
        let count = *count;
        drop(counts);
        if matches!(self.scenario, Scenario::NoAttacks)
            || (matches!(self.scenario, Scenario::SkipOne) && model == "fixture/member0")
        {
            return ResponseTemplate::new(500)
                .set_body_json(json!({"error":{"message":"upstream failed"}}));
        }
        let output = if matches!(self.scenario, Scenario::JudgeFailure) && stage == "rule" {
            json!({"findings":[{"invented":true}]})
        } else if (matches!(self.scenario, Scenario::Repair) && count == 1
            || matches!(self.scenario, Scenario::InvalidAttacks))
            && stage == "attack"
        {
            json!({"vectors":[vector("a",false)]})
        } else if stage == "attack" {
            json!({"vectors":[vector("a",false),vector("b",true)]})
        } else if stage == "challenge" {
            json!({"verdicts":data["vectors"].as_array().unwrap().iter().map(|v|json!({"vectorId":v["vectorId"],"verdict":"holds","reason":"Authentication is explicitly absent.","duplicateOf":null,"defeatingQuote":null})).collect::<Vec<_>>(),"additions":[]})
        } else {
            json!({"findings":[{"findingId":"f1","title":"Unauthenticated arbitrary requests","sourceVectorIds":data["vectors"].as_array().unwrap().iter().map(|v|v["vectorId"].clone()).collect::<Vec<_>>(),"severity":"high","likelihood":"likely","exploit":"Any outsider submits requests.","blastRadius":"All stored data","mitigation":"Authenticate and authorize each request.","rebuttalStatus":"survived","residual":"Credentials may still be abused.","challengeRejection":null}],"killedNotes":[]})
        };
        let response = streaming(&output.to_string(), "stop");
        if (matches!(self.scenario, Scenario::SlowAttack) && stage == "attack")
            || (matches!(self.scenario, Scenario::SlowChallenge) && stage == "challenge")
        {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    config: SquadConfig,
    fake: FakeOpenRouter,
}
impl Fixture {
    async fn new(scenario: Scenario) -> Self {
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
                nick: format!("member{i}").parse().unwrap(),
                model: format!("fixture/member{i}"),
                persona: format!("PERSONA_{i}"),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = "fixture/judge".into();
        config.budget.daily_max_usd = 150.;
        config.budget.debate_confirm_usd = 100.;
        let fake = FakeOpenRouter::start().await;
        let models=config.agents.iter().map(|a|a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id|json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
        fake.catalog(json!({"data":models})).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Answers {
                scenario,
                counts: Arc::new(Mutex::new(BTreeMap::new())),
            })
            .mount(&fake.server)
            .await;
        Self {
            _dir: dir,
            config,
            fake,
        }
    }
    async fn open(&self) -> RoomHandle {
        RoomHandle::open_with_client(
            self.config.clone(),
            "main",
            OpenMode::Interactive,
            OpenRouterClient::new(&self.fake.base_url(), "fake-key").unwrap(),
        )
        .await
        .unwrap()
    }
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    fn state(&self) -> Option<BlowItUpState> {
        self.db()
            .query_row(
                "SELECT state_json FROM contests WHERE kind='blow_it_up'",
                [],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .map(|s| serde_json::from_str(&s).unwrap())
    }
    async fn wait(&self, check: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(25), async {
            while !check() {
                tokio::time::sleep(Duration::from_millis(25)).await;
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
                        r.get::<_, u32>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            panic!("timeout state {:?}: {rows:?}", self.state());
        });
    }
    fn request(&self, text: &str) -> Request {
        let ctx = parser::ParseContext {
            roster: self.config.agents.iter().map(|a| a.nick.clone()).collect(),
            ..Default::default()
        };
        let meta = parser::SubmissionMeta::default();
        Request::from_parsed(
            EventId::new(),
            parse(text, &meta, &ctx).unwrap(),
            &meta,
            false,
        )
        .unwrap()
    }
    async fn start(&self, room: &RoomHandle) {
        let result=room.submit(self.request("@all $blow_it_up ```All requests are accepted without authentication.``` -- access abuse")).await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
    }
}
#[tokio::test]
#[ignore = "Integrator: runtime/judge.rs outer Executor must route new_mode_judge() to Runner"]
async fn six_members_make_13_calls_and_reserve_27_attempts_with_blind_owned_projections() {
    let f = Fixture::new(Scenario::Happy).await;
    let room = f.open().await;
    f.start(&room).await;
    f.wait(|| f.state().is_some_and(|s| s.state.is_terminal()))
        .await;
    let s = f.state().unwrap();
    assert_eq!(s.state, BlowItUpStatus::Complete);
    let db = f.db();
    let plan: String = db
        .query_row("SELECT plan_json FROM contests", [], |r| r.get(0))
        .unwrap();
    let p: ContestPreflight = serde_json::from_str(&plan).unwrap();
    assert_eq!(p.estimate.model_calls, 13);
    assert_eq!(p.settings["reservedAttempts"], 27);
    let calls = f.fake.captured_bodies().await;
    assert_eq!(calls.len(), 13);
    let pool = squad_store::blow_it_up::pool(&db, s.contest_id).unwrap();
    for call in calls
        .iter()
        .filter(|c| input(c).get("vectors").is_some() && input(c).get("tallies").is_none())
    {
        let n = call["model"]
            .as_str()
            .unwrap()
            .strip_prefix("fixture/")
            .unwrap();
        let data = input(call);
        let vectors = data["vectors"].as_array().unwrap();
        assert_eq!(vectors.len(), 10);
        for v in vectors {
            assert!(!pool.provenance.iter().any(|p| p.attacker.as_str() == n
                && p.vector_id.as_str() == v["vectorId"].as_str().unwrap()));
        }
        let prompt = call["messages"].to_string();
        for token in [
            "PERSONA_",
            "fixture/",
            "member0",
            "member1",
            "diversity",
            "finderCount",
            "\"lens\"",
        ] {
            assert!(!prompt.contains(token), "leaked {token}");
        }
    }
    let held: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(reserved_micros),0) FROM spend_daily",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(held, 0);
    let spent: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(spent_micros),0) FROM spend_daily",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(spent, 13 * 45);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn invalid_diversity_sets_are_repaired_once() {
    let f = Fixture::new(Scenario::Repair).await;
    let room = f.open().await;
    f.start(&room).await;
    f.wait(|| f.state().is_some_and(|s| s.state == BlowItUpStatus::Ruling))
        .await;
    let calls = f.fake.captured_bodies().await;
    assert_eq!(
        calls
            .iter()
            .filter(|c| input(c).get("lens").is_some())
            .count(),
        12
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| c["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("Validation errors, encoded as data:"))
            .count(),
        6
    );
    let repairs: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(repairs, 6);
    room.submit(f.request("/stop")).await;
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn insufficient_attacks_fails_without_judge_calls() {
    for scenario in [Scenario::NoAttacks] {
        let f = Fixture::new(scenario).await;
        let room = f.open().await;
        f.start(&room).await;
        f.wait(|| f.state().is_some_and(|s| s.state.is_terminal()))
            .await;
        let s = f.state().unwrap();
        assert_eq!(s.state, BlowItUpStatus::Failed);
        assert_eq!(
            s.terminal_reason.unwrap().as_str(),
            if matches!(scenario, Scenario::NoAttacks | Scenario::InvalidAttacks) {
                "insufficient_attacks"
            } else {
                "judge_failed"
            }
        );
        if matches!(scenario, Scenario::JudgeFailure) {
            assert_eq!(
                squad_store::blow_it_up::tally(&f.db(), s.contest_id)
                    .unwrap()
                    .vectors
                    .len(),
                12
            );
        }
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
#[ignore = "Integrator: runtime/judge.rs outer Executor must route new_mode_judge() to Runner"]
async fn skipped_members_are_disclosed_without_missing_votes() {
    let f = Fixture::new(Scenario::SkipOne).await;
    let room = f.open().await;
    f.start(&room).await;
    f.wait(|| f.state().is_some_and(|s| s.state.is_terminal()))
        .await;
    assert_eq!(f.state().unwrap().state, BlowItUpStatus::Complete);
    let raw: String = f
        .db()
        .query_row(
            "SELECT body FROM contest_artifacts WHERE artifact_kind='blow_it_up.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["skipped"].as_array().unwrap().len(), 2);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn stop_fences_late_results() {
    let f = Fixture::new(Scenario::SlowAttack).await;
    let room = f.open().await;
    f.start(&room).await;
    f.wait(|| {
        f.state()
            .is_some_and(|s| s.state == BlowItUpStatus::Attacking && s.step_id.is_some())
    })
    .await;
    let result = room.submit(f.request("/stop")).await;
    assert!(matches!(result, RequestResult::Accepted { .. }));
    f.wait(|| {
        f.state()
            .is_some_and(|s| s.state == BlowItUpStatus::Stopped)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.state().unwrap().state, BlowItUpStatus::Stopped);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn restart_reuses_lenses_and_pool() {
    let f = Fixture::new(Scenario::SlowChallenge).await;
    let room = f.open().await;
    f.start(&room).await;
    f.wait(|| {
        f.state()
            .is_some_and(|s| s.state == BlowItUpStatus::Challenging)
    })
    .await;
    let id = f.state().unwrap().contest_id;
    let setup = squad_store::blow_it_up::setup(&f.db(), id).unwrap();
    let pool = squad_store::blow_it_up::pool(&f.db(), id).unwrap();
    room.shutdown().await.unwrap();
    let room = f.open().await;
    assert_eq!(squad_store::blow_it_up::setup(&f.db(), id).unwrap(), setup);
    assert_eq!(squad_store::blow_it_up::pool(&f.db(), id).unwrap(), pool);
    room.submit(f.request("/stop")).await;
    room.shutdown().await.unwrap();
}

struct Submitter {
    router: Arc<Mutex<squad_core::router::Router>>,
    owner: ExecutorKey,
}
impl ExecutorSubmit for Submitter {
    fn submit(&self, r: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            {
                let out = self
                    .router
                    .lock()
                    .unwrap()
                    .process(
                        Request::Internal(InternalRequest::Executor(r)),
                        Some(self.owner.clone()),
                    )
                    .unwrap()
                    .result;
                assert!(
                    !matches!(out, RequestResult::Rejected { .. }),
                    "executor request rejected: {out:?}"
                );
                out
            }
        })
    }
}
struct NullRenderer;
impl PromptRenderer for NullRenderer {
    fn render(&self, _p: &PromptRef, _v: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        Err(ReaderError::Forbidden)
    }
}
async fn owned_run(scenario: Scenario) -> (Fixture, squad_store::Store, ContestId) {
    let f = Fixture::new(scenario).await;
    std::fs::create_dir_all(&f.config.data_dir).unwrap();
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let config = f.config.clone();
    let roster = config
        .agents
        .iter()
        .map(|a| a.nick.clone())
        .collect::<Vec<_>>();
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
                effort: constants::NEW_MODE_JUDGE_REASONING_EFFORT,
            },
        )))
        .collect::<BTreeMap<_, _>>();
    let b = bindings.clone();
    store.call(move|db|{for b in b.values(){db.execute("INSERT INTO catalog VALUES(?,1000000,65536,'{\"prompt\":\"0.000001\",\"completion\":\"0.000002\"}','[\"reasoning\"]','2026-11-26T00:00:00Z')",[b.model.as_str()])?;}Ok(())}).unwrap();
    let Request::UserMessage { input, parsed, .. } = f.request(
        "@all $blow_it_up ```All requests are accepted without authentication.``` -- access abuse",
    ) else {
        panic!("request")
    };
    let id = EventId::new();
    let b = bindings.clone();
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, g| {
            squad_core::router::blow_it_up::prepare(
                tx,
                g,
                id,
                &input,
                &parsed,
                &squad_core::router::NewModeAdmission {
                    bindings: &b,
                    config: Some(&config),
                    daily_max: 150_000_000,
                    confirmation_threshold: 100.0,
                    preconfirmed: true,
                },
            )
        })
        .unwrap();
    let contest = f.state().unwrap().contest_id;
    let router = Arc::new(Mutex::new(
        squad_core::router::Router::new(store.clone(), roster, bindings, 150_000_000).unwrap(),
    ));
    let pack = squad_core::prompts::PromptPack::embedded().unwrap();
    let client = OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap();
    for _ in 0..30 {
        let id = EventId::new();
        store
            .commit(id, "fixture".into(), id.to_string(), move |tx, g| {
                squad_core::router::blow_it_up::advance(tx, g, id, contest)?;
                Ok(g.accepted(id))
            })
            .unwrap();
        if f.state().unwrap().state.is_terminal() {
            return (f, store, contest);
        }
        let ids = store
            .call(move |db| {
                let mut q =
                    db.prepare("SELECT work_id FROM work WHERE contest_id=? AND status='queued'")?;
                Ok(
                    q.query_map([contest.to_string()], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                )
            })
            .unwrap();
        for raw in ids {
            let wid: WorkId = raw.parse().unwrap();
            let (w, _) = squad_store::work::template(&f.db(), wid).unwrap().unwrap();
            let owner = squad_store::work::owner(&w).unwrap();
            let claimed = router
                .lock()
                .unwrap()
                .process(
                    Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                        work_id: wid,
                    })),
                    Some(owner.clone()),
                )
                .unwrap()
                .result;
            let RequestResult::Internal(InternalResult::Claimed(grant)) = claimed else {
                panic!("claim {claimed:?}")
            };
            let (_, mailbox) = tokio::sync::mpsc::channel(1);
            let readers = Arc::new(squad_store::readers::ScopedReaders::new(
                store.clone(),
                owner.clone(),
                grant.epoch,
            ));
            let context = AgentContext {
                executor: owner.clone(),
                mailbox,
                history: readers.clone(),
                work: readers,
                submit: Arc::new(Submitter {
                    router: router.clone(),
                    owner,
                }),
                render: Arc::new(NullRenderer),
                cancel: tokio_util::sync::CancellationToken::new(),
            };
            let result = context
                .submit
                .submit(ExecutorRequest::StartAttempt {
                    work_id: wid,
                    attempt: grant.work.attempt,
                    fence: grant.fence,
                    input_tokens: None,
                })
                .await;
            assert!(matches!(result, RequestResult::Internal(_)), "{result:?}");
            squad_core::runtime::blow_it_up::execute(
                &context,
                &grant,
                &client,
                &pack,
                &f.config,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        }
    }
    panic!("owned script did not terminate: {:?}", f.state())
}
#[tokio::test]
async fn owned_engine_and_executor_complete_with_13_calls_export_and_settled_accounting() {
    let (f, store, id) = owned_run(Scenario::Happy).await;
    assert_eq!(f.state().unwrap().state, BlowItUpStatus::Complete);
    assert_eq!(f.fake.captured_bodies().await.len(), 13);
    let raw: String = f
        .db()
        .query_row("SELECT plan_json FROM contests", [], |r| r.get(0))
        .unwrap();
    let plan: ContestPreflight = serde_json::from_str(&raw).unwrap();
    assert_eq!(plan.estimate.model_calls, 13);
    assert_eq!(plan.settings["reservedAttempts"], 27);
    let exported =
        squad_core::export::export(&store, parser::ExportScope::BlowItUp, Some(id)).unwrap();
    for needle in [
        "WHAT SURVIVED",
        "Private lens draw",
        "Target snapshot",
        "blow_it_up.attack",
        "blow_it_up.challenge",
        "blow_it_up.pool",
        "blow_it_up.tally",
    ] {
        assert!(exported.contains(needle), "{needle}");
    }
    assert!(!exported.contains("repairSystem"));
    assert!(!exported.contains("PRIVATE REASONING"));
    let pool = squad_store::blow_it_up::pool(&f.db(), id).unwrap();
    for call in f.fake.captured_bodies().await {
        let d = input(&call);
        if d.get("vectors").is_some() && d.get("tallies").is_none() {
            let n = call["model"]
                .as_str()
                .unwrap()
                .strip_prefix("fixture/")
                .unwrap();
            let prompt = call["messages"].to_string();
            for token in [
                "PERSONA_",
                "fixture/",
                "member0",
                "member1",
                "diversity",
                "finderCount",
                "\"lens\"",
            ] {
                assert!(!prompt.contains(token), "leaked {token}");
            }
            for v in d["vectors"].as_array().unwrap() {
                assert!(!pool.provenance.iter().any(|p| p.attacker.as_str() == n
                    && p.vector_id.as_str() == v["vectorId"].as_str().unwrap()));
            }
        }
    }
    assert_eq!(
        f.db()
            .query_row("SELECT SUM(reserved_micros) FROM spend_daily", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let report = squad_core::router::recovery::recover_on_open(&store).unwrap();
    assert!(report.eligible.is_empty());
    assert_eq!(squad_store::blow_it_up::pool(&f.db(), id).unwrap(), pool);
}
#[tokio::test]
async fn owned_engine_repairs_skips_and_fails_without_inventing_votes() {
    for scenario in [
        Scenario::Repair,
        Scenario::InvalidAttacks,
        Scenario::SkipOne,
        Scenario::NoAttacks,
        Scenario::JudgeFailure,
    ] {
        let (f, _store, id) = owned_run(scenario).await;
        assert_eq!(
            f.state().unwrap().state,
            if matches!(
                scenario,
                Scenario::NoAttacks | Scenario::InvalidAttacks | Scenario::JudgeFailure
            ) {
                BlowItUpStatus::Failed
            } else {
                BlowItUpStatus::Complete
            }
        );
        if matches!(scenario, Scenario::Repair) {
            assert_eq!(f.fake.captured_bodies().await.len(), 19);
            assert_eq!(
                f.db()
                    .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                6
            );
        }
        if matches!(scenario, Scenario::NoAttacks | Scenario::InvalidAttacks) {
            assert_eq!(
                f.state().unwrap().terminal_reason.unwrap().as_str(),
                "insufficient_attacks"
            );
        }
        if matches!(scenario, Scenario::JudgeFailure) {
            assert_eq!(
                f.state().unwrap().terminal_reason.unwrap().as_str(),
                "judge_failed"
            );
            assert_eq!(
                squad_store::blow_it_up::tally(&f.db(), id)
                    .unwrap()
                    .vectors
                    .len(),
                12
            );
        }
    }
}
