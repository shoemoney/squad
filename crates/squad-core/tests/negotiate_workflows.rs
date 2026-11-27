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
}
fn nick(s: &str) -> MemberNick {
    s.parse().unwrap()
}
#[derive(Clone, Copy)]
enum Scenario {
    Happy,
    Deal,
    MoveAbandon,
    TransportRetry,
    PartialMaps,
    BriefRepair,
    WalkAway,
    Abandon,
    MapQuorum,
    BriefFail,
    PlaybookFail,
    Repair,
    NoPackages,
    InterruptedMap,
    InterruptedJudge,
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
fn map() -> serde_json::Value {
    serde_json::json!({"ourInterests":["Reliable delivery"],"ourBatna":"PRIVATE_BATNA_42","counterpartInterests":[{"text":"Steady cash","basis":"inferred"}],"counterpartBatna":"Other buyers","zopa":[{"issue":"price","ourLimit":"PRIVATE_LIMIT_17","theirLikelyLimit":"unknown"}],"tactics":["Ask first"]})
}
fn brief(n: usize) -> serde_json::Value {
    serde_json::json!({"strategySheet":"STRATEGY_ONLY protect fallback","publicScenario":"Public supplier delivery negotiation","counterpartBriefs":(0..n).map(|i|serde_json::json!({"persona":format!("SECRET_BRIEF_{i}"),"priorities":["Cash"],"reservations":[{"issue":"price","limit":"100"}],"hiddenConstraint":format!("HIDDEN_CONSTRAINT_{i}"),"walkAwayRule":"Below 100"})).collect::<Vec<_>>(),"assumptionsToConfirm":["Cash pressure is inferred"]})
}
fn playbook(n: usize) -> serde_json::Value {
    serde_json::json!({"opening":"Ask about priorities","anchors":["Predictable delivery"],"concessionLadder":[{"give":"Earlier payment","onlyFor":"Delivery guarantee"}],"objections":[{"objection":"Too low","response":"Trade timing"}],"walkAwayLine":"Prefer fallback","zopaMap":[{"issue":"price","ourLimit":"PRIVATE_LIMIT_17","theirModelledLimit":"100","overlap":"unknown"}],"questionsToAsk":["What timing helps?"],"tableNotes":(1..=n).map(|i|serde_json::json!({"table":i,"leftOnTable":"Timing","unrealisticConcessions":[]})).collect::<Vec<_>>(),"dependsOnAssumptions":["Cash pressure is inferred"]})
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"][1]["content"].as_str().unwrap();
        let d = authorized(user);
        let stage = if d.get("tables").is_some() {
            "playbook"
        } else if d.get("maps").is_some() {
            "brief"
        } else if d.get("hiddenBrief").is_some() {
            "counter"
        } else if d.get("table").is_some() {
            "move"
        } else if d.get("strategySheet").is_some() {
            "package"
        } else {
            "map"
        };
        if matches!(self.scenario, Scenario::TransportRetry)
            && stage == "counter"
            && self
                .report_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                == 0
        {
            return ResponseTemplate::new(503);
        }
        if matches!(self.scenario, Scenario::BriefRepair)
            && stage == "brief"
            && self
                .report_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                == 0
        {
            return streaming("{}", "stop");
        }
        if (matches!(self.scenario, Scenario::MoveAbandon) && stage == "move")
            || (matches!(self.scenario, Scenario::PartialMaps)
                && stage == "map"
                && !body["model"].as_str().unwrap().ends_with(['0', '1']))
        {
            return streaming("{}", "stop");
        }
        if (matches!(self.scenario, Scenario::MapQuorum)
            && stage == "map"
            && !body["model"].as_str().unwrap().ends_with('0'))
            || (matches!(self.scenario, Scenario::Abandon) && stage == "counter")
            || (matches!(self.scenario, Scenario::BriefFail) && stage == "brief")
            || (matches!(
                self.scenario,
                Scenario::PlaybookFail | Scenario::InterruptedJudge
            ) && stage == "playbook")
            || (matches!(self.scenario, Scenario::NoPackages) && stage == "package")
            || (matches!(self.scenario, Scenario::InterruptedMap) && stage == "map")
        {
            return streaming("{}", "stop");
        }
        if matches!(self.scenario, Scenario::Repair)
            && stage == "move"
            && self
                .report_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                == 0
        {
            return streaming("{}", "stop");
        }
        let value = match stage {
            "map" => map(),
            "brief" => brief(d["tableCount"].as_u64().unwrap() as usize),
            "package" => {
                serde_json::json!({"opener":"Ask for schedule","gives":["Payment timing"],"gets":["Delivery reliability"],"walkAway":"Prefer fallback"})
            }
            "move" => {
                serde_json::json!({"message":format!("TABLE_{}_ONLY round {}",d["table"],d["round"]),"offer":"100 on delivery"})
            }
            "counter" => {
                serde_json::json!({"message":"Discuss delivery","response":if matches!(self.scenario,Scenario::WalkAway){"walk_away"}else if matches!(self.scenario,Scenario::Deal){"accept"}else{"counter"},"counterOffer":"110 on delivery","violatedInterest":null})
            }
            "playbook" => playbook(d["tableCount"].as_u64().unwrap() as usize),
            _ => unreachable!(),
        };
        streaming(&value.to_string(), "stop")
    }
}
const BRIEF: &str = "@all $negotiate Reliable delivery; PRIVATE_BATNA_42 fallback; PRIVATE_LIMIT_17 -- Supplier needs cash";
fn terminal(f: &Fixture) -> bool {
    f.db()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM contests WHERE kind='negotiate' AND active=0)",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn report(f: &Fixture) -> negotiate::NegotiateReport {
    let raw: String = f
        .db()
        .query_row(
            "SELECT body FROM contest_artifacts WHERE artifact_kind='negotiate.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}
// Owned runtime integration uses the same scoped readers and acknowledged router
// ingress while bypassing the shared outer JudgeExecutor gate (see docs/modes/negotiate.md).
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
async fn controlled_run_count(
    s: Scenario,
    stop: Option<WorkKind>,
    recover: bool,
    count: usize,
) -> Fixture {
    let f = Fixture::scenario(count, s).await;
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
    let parsed = ParsedInput::Negotiate {
        input: parser::ConversationalInput {
            raw: BRIEF.into(),
            quote: vec![],
            variable_updates: vec![],
        },
        participants: roster.clone(),
        our_side: "Reliable delivery; PRIVATE_BATNA_42 fallback; PRIVATE_LIMIT_17".into(),
        counterpart: Some("Supplier needs cash".into()),
        rounds: 3,
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
            squad_core::router::negotiate::prepare(
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
    for _ in 0..80 {
        squad_core::orchestrator::negotiate::drive(&mut router.lock().unwrap()).unwrap();
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
            if (matches!(s, Scenario::InterruptedMap)
                && grant.work.kind == WorkKind::NegotiateMap
                && grant.work.attempt == 2)
                || (matches!(s, Scenario::InterruptedJudge)
                    && grant.work.kind == WorkKind::NegotiatePlaybook
                    && grant.work.attempt == 3)
            {
                store
                    .call(|db| squad_store::leases::recover(db, true, chrono::Utc::now()))
                    .unwrap();
                continue;
            }
            if grant.work.kind == WorkKind::NegotiatePlaybook
                && matches!(s, Scenario::WalkAway | Scenario::MoveAbandon)
            {
                let db = f.db();
                let raw: String = db
                    .query_row("SELECT plan_json FROM contests", [], |r| r.get(0))
                    .unwrap();
                let plan: ContestPreflight = serde_json::from_str(&raw).unwrap();
                let initial =
                    squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
                        .unwrap();
                let split:i64=db.query_row("SELECT COALESCE(SUM(json_extract(w.scheduling_meta,'$.attemptReservationMicros')),0) FROM attempts a JOIN work w USING(work_id)",[],|r|r.get(0)).unwrap();
                let (moves, counters) = if matches!(s, Scenario::MoveAbandon) {
                    (6, 9)
                } else {
                    (6, 6)
                };
                let released = 2
                    * (moves
                        * plan.settings["attemptShares"]["negotiate.move"]
                            .as_i64()
                            .unwrap()
                        + counters
                            * plan.settings["attemptShares"]["negotiate.counter"]
                                .as_i64()
                                .unwrap());
                let remaining: i64 = db
                    .query_row(
                        "SELECT amount_micros FROM reservations WHERE work_id IS NULL",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(
                    remaining,
                    initial - split - released,
                    "future turns must be released before the playbook call"
                );
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
                                kind: Some(ContestKind::Negotiate),
                                round: Some(0),
                                actor: Some(Actor::System),
                                audience: Some(ArtifactAudience::Human),
                                body: Some(serde_json::json!({"body":"late"})),
                                contest_id: grant.work.contest_id.unwrap(),
                                step_id: grant.work.step_id.unwrap(),
                                artifact_kind: "negotiate.map".parse().unwrap(),
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
            squad_core::runtime::negotiate::execute(
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

async fn run(s: Scenario) -> Fixture {
    controlled_run_count(s, None, false, 6).await
}
#[tokio::test]
async fn six_members_twenty_nine_calls_sixty_attempts_and_private_requests() {
    let f = run(Scenario::Happy).await;
    let r = report(&f);
    assert_eq!(r.counts["unfinished"], 3);
    assert!(!r.no_rehearsal_evidence);
    let db = f.db();
    let cases: i64 = db
        .query_row("SELECT COUNT(*) FROM cases", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cases, 1);
    let reports: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM contest_artifacts WHERE artifact_kind='negotiate.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reports, 1);
    let n: i64 = db
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 29);
    let p: i64 = db
        .query_row(
            "SELECT json_extract(plan_json,'$.settings.reservedAttempts') FROM contests",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(p, 60);
    let remaining: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 0);
    for b in f.fake.captured_bodies().await {
        let user = b["messages"][1]["content"].as_str().unwrap();
        let d = authorized(user);
        if d.get("hiddenBrief").is_some() {
            assert!(!user.contains("PRIVATE_BATNA_42"));
            assert!(!user.contains("PRIVATE_LIMIT_17"));
            assert!(!user.contains("STRATEGY_ONLY"));
            let table = d["table"].as_u64().unwrap();
            for i in 1..=3 {
                if i != table {
                    assert!(!user.contains(&format!("TABLE_{i}_ONLY")));
                    assert!(!user.contains(&format!("SECRET_BRIEF_{}", i - 1)));
                }
            }
        }
        if d.get("strategySheet").is_some() {
            assert!(!user.contains("SECRET_BRIEF_"));
            assert!(!user.contains("HIDDEN_CONSTRAINT_"));
        }
        if d.get("maps").is_some() {
            for a in &f.config.agents {
                assert!(!user.contains(a.nick.as_str()));
                assert!(!user.contains(&a.persona));
                assert!(!user.contains(&a.model));
            }
        }
    }
    let c = r.setup.tables.clone();
    assert_eq!(c.len(), 3);
    let id: ContestId = db
        .query_row("SELECT contest_id FROM contests", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap()
        .parse()
        .unwrap();
    let snapshot = squad_core::export::ExportSnapshot {
        events: squad_store::negotiate::events(&db, id).unwrap(),
        ..Default::default()
    };
    let export = squad_core::export::negotiate::render(&snapshot, id).unwrap();
    assert!(export.contains("Hypothetical counterpart, not a fact"));
    assert!(export.contains("TABLE 3"));
    assert!(export.contains("negotiate.turn"));
    assert!(!export.contains("repairSystem"));
}
#[tokio::test]
async fn four_members_twenty_calls() {
    let f = controlled_run_count(Scenario::Happy, None, false, 4).await;
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 20);
}
#[tokio::test]
async fn walk_away_ends_tables_and_releases_future_turns() {
    let f = run(Scenario::WalkAway).await;
    let r = report(&f);
    assert_eq!(r.counts["no_deal"], 3);
    assert!(
        r.tables
            .iter()
            .all(|t| t.rounds_used == 1 && t.turns.len() == 2)
    );
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 17);
}
#[tokio::test]
async fn all_counterparts_abandoned_still_writes_playbook() {
    let f = run(Scenario::Abandon).await;
    let r = report(&f);
    assert_eq!(r.counts["abandoned"], 3);
    assert!(r.no_rehearsal_evidence);
    assert!(r.playbook.is_some());
    assert!(r.tables.iter().all(|t| t.turns.len() == 1));
}
#[tokio::test]
async fn skipped_packages_use_strategy_sheet() {
    let f = run(Scenario::NoPackages).await;
    let r = report(&f);
    assert!(r.packages.is_empty());
    assert_eq!(r.skipped.len(), 3);
    for b in f.fake.captured_bodies().await {
        let d = authorized(b["messages"][1]["content"].as_str().unwrap());
        if d.get("table").is_some() && d.get("strategySheet").is_some() {
            assert!(d["package"].is_null());
            assert_eq!(d["fallback"], "Open from the strategy sheet");
        }
    }
}
#[tokio::test]
async fn invalid_member_output_gets_one_repair() {
    let f = run(Scenario::Repair).await;
    assert!(report(&f).playbook.is_some());
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
    assert!(f.fake.captured_bodies().await.iter().any(|b| {
        b["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("validation")
    }));
}
#[tokio::test]
async fn quorum_and_judge_failures_keep_evidence() {
    for (s, reason) in [
        (Scenario::MapQuorum, "insufficient_maps"),
        (Scenario::BriefFail, "judge_failed"),
        (Scenario::PlaybookFail, "judge_failed"),
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
        let partial = report(&f);
        assert!(partial.playbook.is_none());
        if matches!(s, Scenario::PlaybookFail) {
            assert!(partial.tables.iter().all(|t| t.outcome.is_some()));
        }
        if matches!(s, Scenario::MapQuorum) {
            assert_eq!(partial.skipped.len(), 5);
        }
    }
}
#[tokio::test]
async fn recovery_keeps_draw_inputs_and_accepted_turns() {
    let f = controlled_run_count(Scenario::Happy, None, true, 6).await;
    assert_eq!(report(&f).counts["unfinished"], 3);
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 29);
}
#[tokio::test]
async fn stop_fences_every_phase() {
    for kind in [
        WorkKind::NegotiateMap,
        WorkKind::NegotiateBrief,
        WorkKind::NegotiatePackage,
        WorkKind::NegotiateMove,
        WorkKind::NegotiateCounter,
        WorkKind::NegotiatePlaybook,
    ] {
        let f = controlled_run_count(Scenario::Happy, Some(kind), false, 6).await;
        let reason: String = f
            .db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reason, "stopped");
        let partial = report(&f);
        assert!(partial.playbook.is_none());
    }
}
#[tokio::test]
async fn interrupted_final_member_and_judge_attempts_terminate() {
    for s in [Scenario::InterruptedMap, Scenario::InterruptedJudge] {
        let f = controlled_run_count(s, None, false, 6).await;
        assert!(terminal(&f));
    }
}

#[tokio::test]
async fn acceptance_is_an_early_deal_not_a_timeout() {
    let f = run(Scenario::Deal).await;
    let r = report(&f);
    assert_eq!(r.counts["deal"], 3);
    assert!(r.tables.iter().all(|t| t.rounds_used == 1));
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 17);
}
#[tokio::test]
async fn failed_negotiator_never_schedules_a_counterpart() {
    let f = run(Scenario::MoveAbandon).await;
    let r = report(&f);
    assert_eq!(r.counts["abandoned"], 3);
    assert!(r.tables.iter().all(|t| t.turns.is_empty()));
    let n: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM work WHERE kind='negotiate.counter'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
    let n: i64 = f
        .db()
        .query_row(
            "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}
#[tokio::test]
async fn transport_failure_is_plain_retry_and_judge_schema_failure_is_one_repair() {
    let f = run(Scenario::TransportRetry).await;
    assert!(report(&f).playbook.is_some());
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 30);
    let f = run(Scenario::BriefRepair).await;
    assert!(report(&f).playbook.is_some());
    let n: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}
#[tokio::test]
async fn exactly_two_maps_satisfy_quorum_and_skips_are_attributed() {
    let f = run(Scenario::PartialMaps).await;
    let r = report(&f);
    assert_eq!(r.maps.len(), 2);
    assert_eq!(r.skipped.len(), 4);
    assert!(r.playbook.is_some());
}
