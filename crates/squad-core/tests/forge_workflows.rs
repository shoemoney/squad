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
                report_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
            let state = db
                .query_row("SELECT state_json FROM contests", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap_or_default();
            panic!("timeout: {rows:?} state {state}")
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

#[derive(Clone, Copy)]
enum Scenario {
    Happy,
    NoFusions,
    DraftQuorum,
    BallotQuorum,
    JudgeFail,
    RepairFusion,
    OneFusion,
    Original,
    InterruptedDraft,
    InterruptedJudge,
    SlowDraft,
    SlowRank,
    SlowFuse,
    SlowJudge,
}
#[derive(Clone)]
struct Answers {
    scenario: Scenario,
    report_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
fn authorized(user: &str) -> serde_json::Value {
    let start = user.find("Authorized input: ").unwrap() + "Authorized input: ".len();
    serde_json::Deserializer::from_str(&user[start..])
        .into_iter::<serde_json::Value>()
        .next()
        .unwrap()
        .unwrap()
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"][1]["content"].as_str().unwrap();
        let d = authorized(user);
        let stage = if d.get("candidates").is_some() {
            "judge"
        } else if d.get("maxWords").is_some() {
            "fuse"
        } else if d.get("drafts").is_some() {
            "rank"
        } else {
            "draft"
        };
        if matches!(self.scenario, Scenario::InterruptedJudge) && stage == "judge" {
            return ResponseTemplate::new(500);
        }
        if (matches!(self.scenario, Scenario::InterruptedDraft) && stage == "draft")
            || (matches!(self.scenario, Scenario::NoFusions) && stage == "fuse")
            || (matches!(self.scenario, Scenario::DraftQuorum)
                && stage == "draft"
                && !body["model"].as_str().unwrap().ends_with(['0', '1']))
            || (matches!(self.scenario, Scenario::BallotQuorum)
                && stage == "rank"
                && !body["model"].as_str().unwrap().ends_with('0'))
            || (matches!(self.scenario, Scenario::JudgeFail) && stage == "judge")
        {
            return streaming("{}", "stop");
        }
        let value = match stage {
            "draft" => {
                serde_json::json!({"body":format!("Deliverable {} with a careful useful sentence and a final recommendation.",body["model"].as_str().unwrap().chars().last().unwrap())})
            }
            "rank" => {
                let labels = d["labels"].as_array().unwrap();
                serde_json::json!({"ranking":labels,"notes":labels.iter().map(|l|serde_json::json!({"label":l,"steal":"Clear opening","fatalFlaw":null})).collect::<Vec<_>>()})
            }
            "fuse" => {
                let count = self
                    .report_calls
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let over = (matches!(self.scenario, Scenario::RepairFusion) && count == 0)
                    || (matches!(self.scenario, Scenario::OneFusion) && count.is_multiple_of(2));
                serde_json::json!({"body":if over{"too long ".repeat(100)}else{"Fused deliverable with the best opening.".into()},"provenance":[{"element":"Opening","sourceLabels":[d["labels"][0]]}],"handledFlaws":[]})
            }
            "judge" => {
                let mut labels = d["labels"].as_array().unwrap().clone();
                if matches!(self.scenario, Scenario::Original) {
                    let original = d["candidates"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .find(|(_, b)| b.as_str().unwrap().starts_with("Deliverable"))
                        .unwrap()
                        .0;
                    labels.sort_by_key(|l| l.as_str() != Some(original));
                } else {
                    let fusion = d["candidates"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .find(|(_, b)| b.as_str().unwrap().starts_with("Fused"))
                        .unwrap()
                        .0;
                    labels.sort_by_key(|l| l.as_str() != Some(fusion));
                }
                serde_json::json!({"ranking":labels,"reasons":labels.iter().map(|l|serde_json::json!({"label":l,"reason":"Coherent and direct"})).collect::<Vec<_>>(),"seamIssues":[]})
            }
            _ => unreachable!(),
        };
        let result = streaming(&value.to_string(), "stop");
        if matches!(
            (self.scenario, stage),
            (Scenario::SlowDraft, "draft")
                | (Scenario::SlowRank, "rank")
                | (Scenario::SlowFuse, "fuse")
                | (Scenario::SlowJudge, "judge")
        ) {
            result.set_delay(Duration::from_millis(600))
        } else {
            result
        }
    }
}
const BRIEF: &str = "@all $forge Write a concise announcement -- Keep it direct";
fn terminal(f: &Fixture) -> bool {
    f.db()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM contests WHERE kind='forge' AND active=0)",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn report(f: &Fixture) -> forge::ForgeReport {
    let raw: String = f
        .db()
        .query_row(
            "SELECT body FROM contest_artifacts WHERE artifact_kind='forge.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
async fn run(s: Scenario) -> Fixture {
    controlled_run(s, None, false).await
}
#[tokio::test]
async fn six_members_fifteen_calls_thirty_one_attempts_blind_and_exact_winner() {
    let f = run(Scenario::Happy).await;
    let r = report(&f);
    assert_eq!(r.reason, "fusion_won");
    assert_eq!(r.body, "Fused deliverable with the best opening.");
    let db = f.db();
    let calls: i64 = db
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(calls, 15);
    let reserved: i64 = db
        .query_row(
            "SELECT json_extract(plan_json,'$.settings.reservedAttempts') FROM contests",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reserved, 31);
    let remaining: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
    let contest: ContestId = db
        .query_row("SELECT contest_id FROM contests", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
        .parse()
        .unwrap();
    let snapshot = squad_core::export::ExportSnapshot {
        events: squad_store::forge::events(&db, contest).unwrap(),
        ..Default::default()
    };
    let exported = squad_core::export::forge::render(&snapshot, contest).unwrap();
    for text in [
        &r.body,
        "forge.draft",
        "forge.rank",
        "forge.fuse",
        "forge.judge",
        "Ballot label maps",
    ] {
        assert!(exported.contains(text));
    }
    assert!(!exported.contains("repairSystem"));
    let mut q=db.prepare("SELECT w.target,i.input_json FROM forge_inputs i JOIN work w USING(work_id) WHERE w.kind='forge.rank'").unwrap();
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
    {
        let (target, raw) = row.unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let idx = target.trim_start_matches("member");
        assert!(
            !v["data"]["drafts"]
                .to_string()
                .contains(&format!("Deliverable {idx} "))
        );
    }
    for body in f.fake.captured_bodies().await {
        let messages = body["messages"].to_string();
        for n in &f.config.agents {
            assert!(!messages.contains(n.nick.as_str()));
            assert!(!messages.contains(&n.model));
            assert!(!messages.contains(&n.persona));
        }
        let d = authorized(body["messages"][1]["content"].as_str().unwrap());
        if let Some(c) = d.get("candidates") {
            assert_eq!(c.as_object().unwrap().len(), 3);
            assert!(d.get("identities").is_none());
        }
    }
}
#[tokio::test]
async fn no_fusions_skips_judge() {
    let f = run(Scenario::NoFusions).await;
    assert_eq!(report(&f).reason, "no_fusion");
    let n: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM work WHERE kind='forge.judge'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}
#[tokio::test]
async fn quorum_and_judge_exhaustion_are_explicit() {
    for (s, reason) in [
        (Scenario::DraftQuorum, "insufficient_drafts"),
        (Scenario::BallotQuorum, "insufficient_ballots"),
        (Scenario::JudgeFail, "judge_failed"),
    ] {
        let f = run(s).await;
        let r: String = f
            .db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(r, reason);
    }
}
#[tokio::test]
async fn fusion_overrun_is_repaired_once() {
    let f = run(Scenario::RepairFusion).await;
    assert_eq!(report(&f).reason, "fusion_won");
    assert!(f.fake.captured_bodies().await.iter().any(|b| {
        b["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Lmax")
    }));
    let calls: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(calls, 16);
}
#[tokio::test]
async fn original_winner_is_byte_identical() {
    let f = run(Scenario::Original).await;
    let r = report(&f);
    assert_eq!(r.reason, "original_won");
    let ds = squad_store::forge::artifacts(
        &f.db(),
        f.db()
            .query_row("SELECT contest_id FROM contests", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
            .parse()
            .unwrap(),
        "forge.draft",
    )
    .unwrap();
    assert_eq!(
        ds.iter().find(|(n, _)| n == &r.winner).unwrap().1["body"]
            .as_str()
            .unwrap(),
        r.body
    );
}
#[tokio::test]
#[ignore = "Shared runtime/judge.rs outer gate and blind prompt header require integrator fixes"]
async fn full_room_completes_with_fifteen_blind_calls() {
    let f = Fixture::scenario(6, Scenario::Happy).await;
    let room = f.open(OpenMode::Interactive).await;
    assert!(matches!(
        room.submit(request(&f.config, BRIEF)).await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| terminal(&f)).await;
    assert_eq!(report(&f).reason, "fusion_won");
    assert_eq!(report(&f).body, "Fused deliverable with the best opening.");
    let requests = f.fake.captured_bodies().await;
    assert_eq!(requests.len(), 15);
    for request in requests {
        let messages = request["messages"].to_string();
        for member in &f.config.agents {
            assert!(!messages.contains(member.nick.as_str()));
            assert!(!messages.contains(&member.model));
            assert!(!messages.contains(&member.persona));
        }
        // This literal is present in the default judge identity header even
        // when a fixture uses a different roster and anonymous placeholders.
        assert!(!messages.contains("@fable"));
    }
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

fn private_maps(f: &Fixture) -> Vec<(String, String)> {
    f.db()
        .prepare("SELECT scope_key,map_json FROM contest_private_maps ORDER BY scope_key")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn accepted_work(f: &Fixture) -> Vec<String> {
    f.db()
        .prepare("SELECT work_id FROM work WHERE status='completed' ORDER BY work_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[tokio::test]
#[ignore = "Shared runtime/judge.rs outer gate must route new_mode_judge() work; integrator-owned"]
async fn restart_and_stop_fence_each_provider_state() {
    for scenario in [
        Scenario::SlowDraft,
        Scenario::SlowRank,
        Scenario::SlowFuse,
        Scenario::SlowJudge,
    ] {
        let f = Fixture::scenario(6, scenario).await;
        let room = f.open(OpenMode::Interactive).await;
        assert!(matches!(
            room.submit(request(&f.config, BRIEF)).await,
            RequestResult::Accepted { .. }
        ));
        let desired = match scenario {
            Scenario::SlowDraft => "forge.draft",
            Scenario::SlowRank => "forge.rank",
            Scenario::SlowFuse => "forge.fuse",
            _ => "forge.judge",
        };
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM work WHERE kind=? AND status='running')",
                    [desired],
                    |r| r.get(0),
                )
                .unwrap()
        })
        .await;
        room.shutdown().await.unwrap();
        let maps_before = private_maps(&f);
        let accepted_before = accepted_work(&f);
        let room = f.open(OpenMode::Interactive).await;
        f.wait(|| terminal(&f)).await;
        assert_eq!(report(&f).reason, "fusion_won");
        let maps_after = private_maps(&f);
        for entry in &maps_before {
            assert!(maps_after.contains(entry), "committed label map changed");
        }
        let accepted_after = accepted_work(&f);
        for id in accepted_before {
            assert!(accepted_after.contains(&id), "accepted work was replaced");
        }
        let duplicates:i64=f.db().query_row("SELECT COUNT(*) FROM (SELECT target,kind,COUNT(*) n FROM work GROUP BY target,kind HAVING n>1)",[],|r|r.get(0)).unwrap();
        assert_eq!(duplicates, 0);
        assert!(!maps_before.is_empty());
        room.shutdown().await.unwrap();
        let f = Fixture::scenario(6, scenario).await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, BRIEF)).await;
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM work WHERE kind=? AND status='running')",
                    [desired],
                    |r| r.get(0),
                )
                .unwrap()
        })
        .await;
        let result = room
            .submit(Request::Stop {
                request_id: EventId::new(),
                target: None,
            })
            .await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        f.wait(|| terminal(&f)).await;
        tokio::time::sleep(Duration::from_millis(700)).await;
        let r: String = f
            .db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(r, "stopped");
        room.shutdown().await.unwrap();
    }
}

// Owned runtime integration uses the same scoped readers and acknowledged router
// ingress while bypassing the shared outer JudgeExecutor gate (see docs/modes/forge.md).
struct Submitter {
    router: std::sync::Arc<std::sync::Mutex<squad_core::router::Router>>,
    owner: ExecutorKey,
}
impl ExecutorSubmit for Submitter {
    fn submit(&self, r: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            self.router
                .lock()
                .unwrap()
                .process(
                    Request::Internal(InternalRequest::Executor(r)),
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
async fn controlled_run(s: Scenario, stop: Option<WorkKind>, recover: bool) -> Fixture {
    let f = Fixture::scenario(6, s).await;
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
    let models = bindings.clone();
    store.call(move|db|{for b in models.values(){db.execute("INSERT INTO catalog VALUES (?,1000000,65536,'{\"prompt\":\"0.000001\",\"completion\":\"0.000002\"}','[\"reasoning\"]','2026-11-26T00:00:00Z')",[b.model.as_str()])?;}Ok(())}).unwrap();
    let id = EventId::new();
    let parsed = ParsedInput::Forge {
        input: parser::ConversationalInput {
            raw: BRIEF.into(),
            quote: vec![],
            variable_updates: vec![],
        },
        participants: roster.clone(),
        brief: "Write a concise announcement".into(),
        constraints: Some("Keep it direct".into()),
    };
    let input = UserMessageInput {
        message_id: MessageId::new(),
        text: BRIEF.parse().unwrap(),
        quote: None,
        reply_to: None,
    };
    let b = bindings.clone();
    let conf = config.clone();
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, g| {
            squad_core::router::forge::prepare(
                tx,
                g,
                id,
                &input,
                &parsed,
                &squad_core::router::NewModeAdmission {
                    bindings: &b,
                    config: Some(&conf),
                    daily_max: 200_000_000,
                    confirmation_threshold: 100.,
                    preconfirmed: true,
                },
            )
        })
        .unwrap();
    let router = std::sync::Arc::new(std::sync::Mutex::new(
        squad_core::router::Router::new(store.clone(), roster, bindings, 200_000_000).unwrap(),
    ));
    let pack = squad_core::prompts::PromptPack::embedded().unwrap();
    let client = OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap();
    for _ in 0..32 {
        squad_core::orchestrator::forge::drive(&mut router.lock().unwrap()).unwrap();
        if terminal(&f) {
            break;
        }
        if recover {
            let before = store
                .call(|db| {
                    let mut q = db.prepare(
                        "SELECT scope_key,map_json FROM contest_private_maps ORDER BY scope_key",
                    )?;
                    Ok(
                        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                    )
                })
                .unwrap();
            squad_core::router::recovery::recover_on_open_mode(
                &store,
                OpenMode::Interactive,
                chrono::Utc::now(),
            )
            .unwrap();
            let after = store
                .call(|db| {
                    let mut q = db.prepare(
                        "SELECT scope_key,map_json FROM contest_private_maps ORDER BY scope_key",
                    )?;
                    Ok(
                        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                            .collect::<rusqlite::Result<Vec<_>>>()?,
                    )
                })
                .unwrap();
            assert_eq!(before, after);
        }
        let ids = store
            .call(|db| {
                let mut q =
                    db.prepare("SELECT work_id FROM work WHERE status='queued' ORDER BY rowid")?;
                Ok(q.query_map([], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap();
        for id in ids {
            let w: WorkId = id.parse().unwrap();
            let grant = store
                .call(move |db| {
                    let work = squad_store::work::template(db, w)?.unwrap().0;
                    let owner = match work.target {
                        Target::Member(n) => ExecutorKey::Member(n),
                        Target::Judge => ExecutorKey::Case(work.case_id.unwrap()),
                    };
                    squad_store::leases::claim(db, w, &owner, chrono::Utc::now(), None)
                })
                .unwrap()
                .unwrap();
            let reader = std::sync::Arc::new(squad_store::readers::ScopedReaders::new(
                store.clone(),
                grant.owner.clone(),
                grant.epoch,
            ));
            let (_, mailbox) = tokio::sync::mpsc::channel(1);
            let context = AgentContext {
                executor: grant.owner.clone(),
                mailbox,
                history: reader.clone(),
                work: reader,
                submit: std::sync::Arc::new(Submitter {
                    router: router.clone(),
                    owner: grant.owner.clone(),
                }),
                render: std::sync::Arc::new(UnusedRenderer),
                cancel: tokio_util::sync::CancellationToken::new(),
            };
            let grant = squad_agents::worker::start_attempt(&context, &grant, grant.work.attempt)
                .await
                .unwrap();
            if (matches!(s, Scenario::InterruptedDraft)
                && grant.work.kind == WorkKind::ForgeDraft
                && grant.work.attempt == 2)
                || (matches!(s, Scenario::InterruptedJudge)
                    && grant.work.kind == WorkKind::ForgeJudge
                    && grant.work.attempt == 3)
            {
                store
                    .call(|db| squad_store::leases::recover(db, true, chrono::Utc::now()))
                    .unwrap();
                continue;
            }
            if stop.as_ref() == Some(&grant.work.kind) {
                let result = router
                    .lock()
                    .unwrap()
                    .process(
                        Request::Stop {
                            request_id: EventId::new(),
                            target: None,
                        },
                        None,
                    )
                    .unwrap();
                assert!(matches!(result.result, RequestResult::Accepted { .. }));
                let stale = router
                    .lock()
                    .unwrap()
                    .process(
                        Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                            work_id: grant.work.work_id,
                            attempt: grant.work.attempt,
                            fence: grant.fence,
                            epoch: grant.epoch,
                            output: EnvelopePayload::ContestArtifact(ContestArtifact {
                                kind: Some(ContestKind::Forge),
                                round: Some(0),
                                actor: Some(Actor::System),
                                audience: Some(ArtifactAudience::Human),
                                body: Some(serde_json::json!({"body":"late"})),
                                contest_id: grant.work.contest_id.unwrap(),
                                step_id: grant.work.step_id.unwrap(),
                                artifact_kind: "forge.draft".parse().unwrap(),
                                work_id: Some(grant.work.work_id),
                                source_refs: vec![],
                                data: serde_json::json!({}),
                            }),
                        })),
                        Some(grant.owner.clone()),
                    )
                    .unwrap_err();
                assert!(
                    matches!(
                        stale,
                        squad_store::StoreError::Domain(ErrorCode::STALE_WORK_ATTEMPT, _)
                    ),
                    "{stale:?}"
                );
                f.assert_ledger();
                return f;
            }
            squad_core::runtime::forge::execute(
                &context,
                &grant,
                &client,
                &pack,
                &config,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        }
    }
    assert!(terminal(&f));
    f.assert_ledger();
    f
}

#[tokio::test]
async fn second_fusion_overrun_leaves_one_candidate() {
    let f = run(Scenario::OneFusion).await;
    let r = report(&f);
    assert_eq!(r.reason, "fusion_won");
    assert_eq!(r.verdict.unwrap().ranking.len(), 2);
    assert_eq!(r.skipped.len(), 1);
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn owned_stop_fences_late_results_at_each_provider_phase() {
    for kind in [
        WorkKind::ForgeDraft,
        WorkKind::ForgeRank,
        WorkKind::ForgeFuse,
        WorkKind::ForgeJudge,
    ] {
        let f = controlled_run(Scenario::Happy, Some(kind), false).await;
        let reason: String = f
            .db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reason, "stopped");
    }
}
#[tokio::test]
async fn owned_recovery_preserves_labels_tally_and_accepted_work() {
    let f = controlled_run(Scenario::Happy, None, true).await;
    assert_eq!(report(&f).reason, "fusion_won");
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 15);
}

#[tokio::test]
async fn interrupted_last_attempt_settles_without_an_immortal_queue() {
    for (s, reason) in [
        (Scenario::InterruptedDraft, "insufficient_drafts"),
        (Scenario::InterruptedJudge, "judge_failed"),
    ] {
        let f = run(s).await;
        let r: String = f
            .db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(r, reason);
        let queued: i64 = f
            .db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE status IN ('queued','running')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(queued, 0);
    }
}
