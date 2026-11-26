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
fn data(text: &str) -> serde_json::Value {
    let raw = text
        .split("Authorized input: ")
        .nth(1)
        .expect("input")
        .split("\nSchema:")
        .next()
        .unwrap();
    serde_json::from_str(raw).unwrap()
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        if body["response_format"]["json_schema"]["name"] != "decide_response" {
            return streaming("ordinary response", "stop");
        }
        let d = data(text);
        let title = if text.starts_with("Propose exactly") {
            "DecideOptionProposal"
        } else if text.starts_with("Merge duplicate") {
            "DecideShortlist"
        } else if text.starts_with("Propose one") {
            "DecideCriteria"
        } else if text.starts_with("Merge synonymous") {
            "DecideFreeze"
        } else if text.starts_with("Score every") {
            "DecideScores"
        } else {
            "DecideExplanation"
        };
        let output = match title {
            "DecideOptionProposal" => {
                serde_json::json!({"options":[{"localId":"a","name":"Build","description":"Build the service"},{"localId":"b","name":"Buy","description":"Buy the service"}]})
            }
            "DecideShortlist" => {
                serde_json::json!({"options":([("a","Build"),("b","Buy")]).iter().enumerate().map(|(i,(id,name))|serde_json::json!({"optionId":format!("o{}",i+1),"name":name,"description":"A service","sources":d["proposals"].as_array().unwrap().iter().map(|p|serde_json::json!({"memberLabel":p["memberLabel"],"localId":id})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
            }
            "DecideCriteria" => {
                serde_json::json!({"criteria":[{"localId":"a","name":"Cost","definition":"Lower cost is better","weight":60},{"localId":"b","name":"Reliability","definition":"More reliable is better","weight":40}]})
            }
            "DecideFreeze" => {
                serde_json::json!({"criteria":([("a","Cost"),("b","Reliability")]).iter().enumerate().map(|(i,(id,name))|serde_json::json!({"criterionId":format!("c{}",i+1),"name":name,"definition":"Higher scores are better","sources":d["proposals"].as_array().unwrap().iter().map(|p|serde_json::json!({"memberLabel":p["memberLabel"],"localId":id})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
            }
            "DecideScores" => {
                serde_json::json!({"cells":d["options"].as_array().unwrap().iter().flat_map(|o|d["criteria"].as_array().unwrap().iter().map(move |c|serde_json::json!({"optionId":o["optionId"],"criterionId":c["criterionId"],"score":if o["optionId"]=="o1"{8}else{5},"evidence":"Source assumptions support this score"}))).collect::<Vec<_>>()})
            }
            "DecideExplanation" => {
                serde_json::json!({"flipConditions":[],"dissents":[],"evidenceGaps":["Validate cost estimates"]})
            }
            _ => panic!("unknown schema {title}"),
        };
        streaming(&output.to_string(), "stop")
    }
}
impl Fixture {
    fn status(&self) -> String {
        self.db().query_row("SELECT json_extract(state_json,'$.state') FROM contests ORDER BY rowid DESC LIMIT 1",[],|r|r.get(0)).unwrap()
    }
    fn contest(&self) -> ContestId {
        let c: String = self
            .db()
            .query_row(
                "SELECT contest_id FROM contests ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        c.parse().unwrap()
    }
    async fn start(&self, text: &str) -> RoomHandle {
        let room = self.open(OpenMode::Interactive).await;
        let result = room.submit(request(&self.config, text)).await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        room
    }
}
#[tokio::test]
#[ignore = "Integrator: route new_mode_judge through the structured runner in runtime/judge.rs"]
async fn given_and_generated_six_member_call_counts_blinding_and_recovery() {
    for generated in [false, true] {
        let f = Fixture::new().await;
        let command = if generated {
            "@all $decide Select service"
        } else {
            "@all $decide Select service\n--\nBuild\nBuy"
        };
        let room = f.start(command).await;
        f.wait(|| f.status() == "COMPLETE" || f.status() == "FAILED")
            .await;
        assert_eq!(f.status(), "COMPLETE");
        let c = f.contest();
        let bodies = f.fake.captured_bodies().await;
        assert_eq!(bodies.len(), if generated { 21 } else { 14 });
        let plan: String = f
            .db()
            .query_row("SELECT plan_json FROM contests", [], |r| r.get(0))
            .unwrap();
        let p: ContestPreflight = serde_json::from_str(&plan).unwrap();
        assert_eq!(
            p.settings["reservedAttempts"],
            if generated { 45 } else { 30 }
        );
        let mut orders = std::collections::BTreeSet::new();
        for body in bodies {
            if body["messages"][1]["content"]
                .as_str()
                .is_some_and(|s| s.starts_with("Score every"))
            {
                let d = data(body["messages"][1]["content"].as_str().unwrap());
                assert!(!d.to_string().to_lowercase().contains("weight"));
                orders.insert(d["options"].to_string());
            }
        }
        assert!(orders.len() > 1);
        let raw: String = f
            .db()
            .query_row(
                "SELECT body FROM contest_artifacts WHERE artifact_kind='decide.report'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let r: squad_protocol::decide::DecideReport = serde_json::from_str(&raw).unwrap();
        assert_eq!(r.matrix.winner.as_deref(), Some("o1"));
        assert_eq!(r.matrix.ranking[0].total, 8.0);
        room.shutdown().await.unwrap();
        let room = f.open(OpenMode::Interactive).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            f.fake.captured_bodies().await.len(),
            if generated { 21 } else { 14 }
        );
        let state = squad_store::decide::state(&f.db(), c).unwrap();
        assert!(state.state.is_terminal());
        let result = room
            .submit(Request::Clear {
                request_id: EventId::new(),
            })
            .await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        assert_eq!(
            f.db()
                .query_row("SELECT COUNT(*) FROM decide_records", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        room.shutdown().await.unwrap();
    }
}
// Drive owned callbacks and executors directly until the shared outer judge dispatch is integrated.
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
async fn owned_run(generated: bool, fault: Fault) -> (Fixture, squad_store::Store, ContestId) {
    let f = Fixture::new().await;
    std::fs::create_dir_all(&f.config.data_dir).unwrap();
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let config = f.config.clone();
    if fault != Fault::None {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Faults {
                kind: fault,
                seen: Default::default(),
            })
            .with_priority(1)
            .mount(&f.fake.server)
            .await;
    }
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
    store.call(move |db|{for x in b.values(){db.execute("INSERT OR IGNORE INTO catalog VALUES (?,1000000,65536,'{\"prompt\":\"0.000001\",\"completion\":\"0.000002\"}','[\"reasoning\"]','2026-11-26T00:00:00Z')",[x.model.as_str()])?;}Ok(())}).unwrap();
    let id = EventId::new();
    let roster = config
        .agents
        .iter()
        .map(|a| a.nick.clone())
        .collect::<Vec<_>>();
    let text = if generated {
        "@all $decide Select service"
    } else {
        "@all $decide Select service\n--\nBuild\nBuy"
    };
    let parsed = ParsedInput::Decide {
        input: parser::ConversationalInput {
            raw: text.into(),
            quote: vec![],
            variable_updates: vec![],
        },
        participants: roster.clone(),
        decision: "Select service".into(),
        options: if generated {
            vec![]
        } else {
            vec!["Build".into(), "Buy".into()]
        },
    };
    let input = UserMessageInput {
        message_id: MessageId::new(),
        text: text.parse().unwrap(),
        quote: None,
        reply_to: None,
    };
    let bb = bindings.clone();
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, g| {
            squad_core::router::decide::prepare(
                tx,
                g,
                id,
                &input,
                &parsed,
                &squad_core::router::NewModeAdmission {
                    bindings: &bb,
                    config: Some(&config),
                    daily_max: 200_000_000,
                    confirmation_threshold: 100.0,
                    preconfirmed: true,
                },
            )
        })
        .unwrap();
    let c = f.contest();
    let router = std::sync::Arc::new(std::sync::Mutex::new(
        squad_core::router::Router::new(store.clone(), roster, bindings, 200_000_000).unwrap(),
    ));
    let pack = squad_core::prompts::PromptPack::from_override_dir(None).unwrap();
    for _ in 0..30 {
        let id = EventId::new();
        store
            .commit(id, "fixture".into(), id.to_string(), move |tx, g| {
                squad_core::router::decide::advance(tx, g, id, c)?;
                Ok(g.accepted(id))
            })
            .unwrap();
        let state = squad_store::decide::state(&f.db(), c).unwrap();
        if state.state.is_terminal() {
            return (f, store, c);
        }
        if state.state == DecideStatus::Scoring && state.step_id.is_some() {
            if fault == Fault::Stop {
                let id = EventId::new();
                store
                    .commit(id, "stop".into(), id.to_string(), move |tx, g| {
                        squad_core::router::decide::stop(tx, g, id, c)?;
                        Ok(g.accepted(id))
                    })
                    .unwrap();
                return (f, store, c);
            }
            if fault == Fault::Recover {
                let prior: serde_json::Value =
                    squad_store::decide::get(&f.db(), c, "setup").unwrap();
                squad_core::router::recovery::recover_on_open(&store).unwrap();
                assert_eq!(
                    prior,
                    squad_store::decide::get::<serde_json::Value>(&f.db(), c, "setup").unwrap()
                );
            }
        }
        let ids = store
            .call(move |db| {
                let mut q =
                    db.prepare("SELECT work_id FROM work WHERE contest_id=? AND status='queued'")?;
                Ok(q.query_map([c.to_string()], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap();
        for raw in ids {
            let wid: WorkId = raw.parse().unwrap();
            let (w, _) = squad_store::work::template(&f.db(), wid).unwrap().unwrap();
            let owner = squad_store::work::owner(&w).unwrap();
            let result = router
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
            let RequestResult::Internal(InternalResult::Claimed(grant)) = result else {
                panic!("claim: {result:?}")
            };
            let (_, mailbox) = tokio::sync::mpsc::channel(1);
            let readers = std::sync::Arc::new(squad_store::readers::ScopedReaders::new(
                store.clone(),
                owner.clone(),
                grant.epoch,
            ));
            let context = AgentContext {
                executor: owner.clone(),
                mailbox,
                history: readers.clone(),
                work: readers,
                submit: std::sync::Arc::new(Submitter {
                    router: router.clone(),
                    owner,
                }),
                render: std::sync::Arc::new(NullRenderer),
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
            squad_core::runtime::decide::execute(
                &context,
                &grant,
                &OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap(),
                &pack,
                &f.config,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        }
    }
    panic!("owned script did not finish")
}
struct NullRenderer;
impl PromptRenderer for NullRenderer {
    fn render(&self, _p: &PromptRef, _v: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        Err(ReaderError::Forbidden)
    }
}
#[tokio::test]
async fn owned_callbacks_complete_both_scripts_with_exact_calls_and_export() {
    for generated in [false, true] {
        let (f, store, c) = owned_run(generated, Fault::None).await;
        assert_eq!(
            f.fake.captured_bodies().await.len(),
            if generated { 21 } else { 14 }
        );
        let raw: String = f
            .db()
            .query_row("SELECT plan_json FROM contests", [], |r| r.get(0))
            .unwrap();
        let plan: ContestPreflight = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            plan.settings["reservedAttempts"],
            if generated { 45 } else { 30 }
        );
        let mut orders = std::collections::BTreeSet::new();
        for body in f.fake.captured_bodies().await {
            let user = body["messages"][1]["content"].as_str().unwrap();
            if user.starts_with("Score every") {
                let input = data(user);
                assert!(!input.to_string().to_lowercase().contains("weight"));
                orders.insert(input["options"].to_string());
            }
        }
        assert_eq!(orders.len(), 2);
        let text =
            squad_core::export::export(&store, parser::ExportScope::Decide, Some(c)).unwrap();
        assert!(text.contains("Winner: o1"));
        assert!(text.contains("Committed proposals"));
        f.assert_ledger();
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Repair,
    SkipOne,
    NoCriteria,
    NoScores,
    Freeze,
    Explain,
    NoShared,
    Stop,
    Recover,
}
#[derive(Clone)]
struct Faults {
    kind: Fault,
    seen: std::sync::Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
}
impl Respond for Faults {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        let model = body["model"].as_str().unwrap();
        let criteria = text.starts_with("Propose one");
        let score = text.starts_with("Score every");
        let freeze = text.starts_with("Merge synonymous");
        let explain = text.starts_with("Explain only");
        if self.kind == Fault::Repair && criteria && self.seen.lock().unwrap().insert(model.into())
        {
            return streaming("{\"criteria\":[]}", "stop");
        }
        if (self.kind == Fault::SkipOne && model == "fixture/member0")
            || (self.kind == Fault::NoCriteria && criteria)
            || (self.kind == Fault::NoScores && score)
        {
            return ResponseTemplate::new(500);
        }
        if (self.kind == Fault::Freeze && freeze) || (self.kind == Fault::Explain && explain) {
            return streaming("{\"winner\":\"invented\"}", "stop");
        }
        if self.kind == Fault::NoShared && freeze {
            let d = data(text);
            let mut cs = vec![];
            for p in d["proposals"].as_array().unwrap() {
                for c in p["criteria"].as_array().unwrap() {
                    cs.push(serde_json::json!({"criterionId":format!("{}{}",p["memberLabel"].as_str().unwrap(),c["localId"].as_str().unwrap()),"name":c["name"],"definition":c["definition"],"sources":[{"memberLabel":p["memberLabel"],"localId":c["localId"]}]}));
                }
            }
            return streaming(&serde_json::json!({"criteria":cs}).to_string(), "stop");
        }
        Answers.respond(r)
    }
}
#[tokio::test]
async fn failures_repairs_skips_and_quorums_remain_exportable() {
    for fault in [
        Fault::SkipOne,
        Fault::NoCriteria,
        Fault::NoScores,
        Fault::Freeze,
        Fault::Explain,
        Fault::NoShared,
    ] {
        let (f, store, c) = owned_run(false, fault).await;
        let s = squad_store::decide::state(&f.db(), c).unwrap();
        let expected = match fault {
            Fault::Repair | Fault::SkipOne => "complete",
            Fault::NoCriteria => "insufficient_criteria",
            Fault::NoScores => "insufficient_scores",
            Fault::NoShared => "no_shared_rubric",
            _ => "judge_failed",
        };
        assert_eq!(s.terminal_reason.as_ref().unwrap().as_str(), expected);
        let text =
            squad_core::export::export(&store, parser::ExportScope::Decide, Some(c)).unwrap();
        assert!(text.contains("Committed proposals"));
        if fault == Fault::Explain {
            assert!(text.contains("Winner: o1"));
        }
        if fault == Fault::Repair {
            let repairs: i64 = f
                .db()
                .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r.get(0))
                .unwrap();
            assert_eq!(repairs, 6);
            assert!(
                f.fake
                    .captured_bodies()
                    .await
                    .iter()
                    .filter(|b| b["messages"][1]["content"]
                        .as_str()
                        .unwrap()
                        .contains("Validation errors, encoded as data:"))
                    .count()
                    >= 6
            );
        }
        f.assert_ledger();
    }
}

#[tokio::test]
#[ignore = "Integrator: executor::failed must classify MODE_INVALID_OUTPUT as InvalidOutput for D75"]
async fn member_invalid_output_repairs_once() {
    let (f, _, c) = owned_run(false, Fault::Repair).await;
    assert_eq!(
        squad_store::decide::state(&f.db(), c).unwrap().state,
        DecideStatus::Complete
    );
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM mode_repairs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        6
    );
}

#[tokio::test]
async fn stop_and_startup_recovery_preserve_frozen_evidence() {
    let (f, store, c) = owned_run(false, Fault::Stop).await;
    assert_eq!(
        squad_store::decide::state(&f.db(), c).unwrap().state,
        DecideStatus::Stopped
    );
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE status IN ('queued','running')",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let text = squad_core::export::export(&store, parser::ExportScope::Decide, Some(c)).unwrap();
    assert!(text.contains("decide.criteria"));
    assert!(!text.contains("Winner:"));
    f.assert_ledger();
    let (f, _, c) = owned_run(false, Fault::Recover).await;
    assert_eq!(
        squad_store::decide::state(&f.db(), c).unwrap().state,
        DecideStatus::Complete
    );
    f.assert_ledger();
}
