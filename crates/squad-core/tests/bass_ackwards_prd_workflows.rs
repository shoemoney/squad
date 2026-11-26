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
        tokio::time::timeout(Duration::from_secs(90), async {
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

#[path = "../../squad-protocol/tests/bass_ackwards_prd.rs"]
mod fixtures;
use serde_json::{Value, json};
#[derive(Clone)]
struct Answers;
fn extract_data(text: &str) -> Value {
    let (_, tail) = text.split_once("Authorized input: ").expect("input");
    let mut stream = serde_json::Deserializer::from_str(tail).into_iter::<Value>();
    stream.next().unwrap().unwrap()
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        let d = extract_data(text);
        let schema = &body["response_format"]["json_schema"]["schema"]["properties"];
        let mut output = if schema.get("items").is_some() {
            json!({"items":[{"localId":"a","test":"Public page is reachable","verifiedBy":"HTTP request"},{"localId":"b","test":"Outage alerts reach operators","verifiedBy":"Simulated outage"},{"localId":"c","test":"History persists after restart","verifiedBy":"Restart test"}]})
        } else if schema.get("groups").is_some() {
            let mut groups = vec![];
            for local in ["a", "b", "c"] {
                let items = d["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|i| i["ref"].as_str().unwrap().ends_with(&format!(":{local}")))
                    .collect::<Vec<_>>();
                groups.push(json!({"text":items[0]["test"],"sourceRefs":items.iter().map(|i|i["ref"].clone()).collect::<Vec<_>>()}));
            }
            json!({"groups":groups})
        } else if schema.get("nodes").is_some() {
            fixtures::tree()
        } else if schema.get("milestones").is_some() {
            fixtures::document()
        } else if schema.get("comments").is_some() {
            json!({"build":if d["lens"]=="READBACK"{json!("Build the status page and alerts.")}else{Value::Null},"comments":[{"localId":"c","quote":"Deploy the public status page","comment":"Clarify verification","proposed":null},{"localId":"bad","quote":"This quote does not exist anywhere","comment":"Dropped","proposed":null}]})
        } else if schema.get("additions").is_some() {
            json!({"additions":[{"localId":"a","text":"Add operations guide","under":d["keys"]["milestones"][0]["key"],"dependsOn":[],"serves":["D1"],"ownerRole":"Operator","risk":"Missing documentation","why":"Make operations repeatable"}]})
        } else if schema.get("applied").is_some() {
            json!({"applied":d["notes"].as_array().unwrap().iter().map(|n|json!({"note":n["ref"],"appliedTo":[],"conflict":"This fixture records a conflict"})).collect::<Vec<_>>(),"edits":[]})
        } else {
            json!({"decisions":d["sourceIds"].as_array().unwrap().iter().map(|id|json!({"ref":id,"verdict":"reject","reason":"Already covered","appliedTo":[],"removes":[]})).collect::<Vec<_>>(),"edits":[]})
        };
        let scenario = d["endResult"].as_str().unwrap_or("");
        if scenario.contains("ACCEPT_PATCH") && schema.get("decisions").is_some() {
            let refs = d["sourceIds"].as_array().unwrap();
            if d.get("comments").is_some() {
                let key = &d["document"]["milestones"][0]["phases"][0]["lineItems"][0]["key"];
                output["decisions"][0] = json!({"ref":refs[0],"verdict":"accept","reason":"Clarify verification","appliedTo":[key],"removes":[]});
                output["edits"] = json!([{"op":"setText","key":key,"field":"risk","value":"Tested sparse revision"}]);
            } else {
                let mut line = d["document"]["milestones"][0]["phases"][0]["lineItems"][0].clone();
                line["key"] = Value::Null;
                line["localId"] = json!("operations-guide");
                line["text"] = json!("Publish operations guide");
                line["sources"] = json!([refs[0]]);
                output["decisions"][0] = json!({"ref":refs[0],"verdict":"accept","reason":"Document operations","appliedTo":["operations-guide"],"removes":[]});
                output["edits"] = json!([{"op":"addLine","phaseKey":d["document"]["milestones"][0]["phases"][0]["key"],"line":line}]);
            }
        }
        if scenario.contains("BROKEN_DONE") && schema.get("items").is_some() {
            output = json!({"items":[]});
        }
        if scenario.contains("BROKEN_REVISE")
            && schema.get("decisions").is_some()
            && d.get("comments").is_some()
        {
            output["edits"] = json!([{"op":"setText","key":d["document"]["milestones"][0]["phases"][0]["lineItems"][0]["key"],"field":"text","value":"Unauthorized edit"}]);
        }
        if scenario.contains("EMPTY_ROUNDS") {
            if schema.get("comments").is_some() {
                output["comments"] = json!([]);
            }
            if schema.get("additions").is_some() {
                output["additions"] = json!([]);
            }
        }
        // Streaming framing is covered by the transport suite; one chunk keeps
        // this suite focused on durable workflow transitions.
        raw_sse(format!(
            "{}{}data: [DONE]\r\n\r\n",
            event(
                json!({"id":GENERATION,"model":MODEL,"choices":[{"index":0,"delta":{"content":output.to_string()},"finish_reason":"stop"}]})
            ),
            event(json!({"choices":[],"usage":usage()}))
        ))
    }
}
#[tokio::test]
async fn default_six_members_save_one_file_blind_and_restart_without_calls() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(
            &f.config,
            "@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts",
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| {
        f.db()
            .query_row("SELECT COUNT(*) FROM contests WHERE active=0", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            == 1
    })
    .await;
    let state: String = f
        .db()
        .query_row("SELECT state_json FROM contests", [], |r| r.get(0))
        .unwrap();
    let state: BassAckwardsPrdState = serde_json::from_str(&state).unwrap();
    assert_eq!(state.state, BassAckwardsPrdStatus::Complete, "{state:?}");
    assert_eq!(
        state.terminal_reason.unwrap().as_str(),
        "saved_with_findings"
    );
    let raw: String = f
        .db()
        .query_row(
            "SELECT body FROM bass_ackwards_prd_records WHERE record_key='saved'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let saved: squad_protocol::bass_ackwards_prd::BassAckwardsPrdSaved =
        serde_json::from_str(&raw).unwrap();
    let bytes = std::fs::read(&saved.path).unwrap();
    assert_eq!(bytes.len() as u64, saved.bytes);
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("## Dependency order")
    );
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 28);
    for b in &bodies {
        let text = b["messages"].to_string();
        for a in &f.config.agents {
            assert!(!text.contains(&a.nick.to_string()));
            assert!(!text.contains(&a.model));
            assert!(!text.contains(&a.persona));
        }
        let d = extract_data(b["messages"][1]["content"].as_str().unwrap());
        if d.get("lens").is_some() {
            let lens = d["lens"].as_str().unwrap();
            assert!(["GAPS", "ATTACK", "READBACK"].contains(&lens));
            assert!(d.get("trees").is_none());
        }
        if d.get("comments").is_some() {
            assert!(
                d["comments"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|c| c["localId"].is_null())
            );
            assert!(!d.to_string().contains("This quote does not exist anywhere"));
        }
    }
    let exported = room
        .export(ExportScope::BassAckwardsPrd, Some(state.contest_id))
        .await
        .unwrap();
    assert!(exported.contains("PRD saved to"));
    assert!(exported.contains("This quote does not exist anywhere"));
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 28);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

use squad_core::router::Router;
use std::sync::{Arc, Mutex};
struct Submit {
    router: Arc<Mutex<Router>>,
    owner: ExecutorKey,
}
impl ExecutorSubmit for Submit {
    fn submit(&self, r: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            self.router
                .lock()
                .unwrap()
                .process(
                    Request::Internal(InternalRequest::Executor(r)),
                    Some(self.owner.clone()),
                )
                .expect("executor commit")
                .result
        })
    }
}
struct NoRender;
impl PromptRenderer for NoRender {
    fn render(&self, _: &PromptRef, _: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        panic!("mode uses its frozen step prompt")
    }
}
struct Harness {
    router: Arc<Mutex<Router>>,
    store: squad_store::Store,
    config: SquadConfig,
    client: OpenRouterClient,
}
impl Harness {
    fn new(f: &Fixture) -> Self {
        std::fs::create_dir_all(&f.config.data_dir).unwrap();
        let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
        let models = f
            .config
            .agents
            .iter()
            .map(|a| a.model.clone())
            .chain(std::iter::once(f.config.judge.model.clone()))
            .collect::<Vec<_>>();
        store.call(move|db|{for model in models{db.execute("INSERT OR IGNORE INTO catalog VALUES(?,1000000,65536,'{\"prompt\":\"0.000001\",\"completion\":\"0.000002\"}','[]','2026-11-26T00:00:00Z')",[model])?;}Ok(())}).unwrap();
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
                    effort: ReasoningEffort::Low,
                },
            )))
            .collect();
        let mut router = Router::new(
            store.clone(),
            f.config.agents.iter().map(|a| a.nick.clone()).collect(),
            bindings,
            200000000,
        )
        .unwrap();
        router.engine_config = Some(f.config.clone());
        router.engine_scope = Some(Default::default());
        Self {
            router: Arc::new(Mutex::new(router)),
            store,
            config: f.config.clone(),
            client: OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap(),
        }
    }
    fn send(&self, text: &str) -> RequestResult {
        // Exercise owned admission while the public switch remains off until
        // the shared judge-dispatch prerequisite has been integrated.
        if let Some(end) = text.strip_prefix("@all $bass_ackwards_prd ") {
            let config = self.config.clone();
            let raw = text.to_owned();
            let end = end.to_owned();
            let id = EventId::new();
            return self
                .store
                .commit(
                    id,
                    "fixture-admission".into(),
                    id.to_string(),
                    move |tx, g| {
                        let participants = config
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
                                    effort: ReasoningEffort::Low,
                                },
                            )))
                            .collect();
                        let input = UserMessageInput {
                            message_id: MessageId::from_event(id),
                            text: raw.parse().unwrap(),
                            quote: None,
                            reply_to: None,
                        };
                        let parsed = ParsedInput::BassAckwardsPrd {
                            input: parser::ConversationalInput {
                                raw,
                                quote: vec![],
                                variable_updates: vec![],
                            },
                            participants,
                            end_result: end,
                            today: None,
                        };
                        squad_core::router::bass_ackwards_prd::prepare(
                            tx,
                            g,
                            id,
                            &input,
                            &parsed,
                            &squad_core::router::NewModeAdmission {
                                bindings: &bindings,
                                config: Some(&config),
                                daily_max: 200000000,
                                confirmation_threshold: 100.,
                                preconfirmed: true,
                            },
                        )
                    },
                )
                .unwrap()
                .result;
        }
        let mut router = self.router.lock().unwrap();
        let ctx = router.parse_context().unwrap();
        let meta = Default::default();
        let parsed = parse(text, &meta, &ctx).unwrap();
        router
            .process(
                Request::from_parsed_with_context(EventId::new(), parsed, &meta, &ctx, true)
                    .unwrap(),
                None,
            )
            .unwrap()
            .result
    }
    async fn pump(&self) {
        self.pump_until(None).await;
    }
    async fn pump_until(&self, stop: Option<BassAckwardsPrdStatus>) {
        for _ in 0..64 {
            let state = self.state();
            if stop == Some(state.state) {
                return;
            }
            if state.state.is_terminal() || state.state == BassAckwardsPrdStatus::AwaitingHuman {
                return;
            }
            if state.state == BassAckwardsPrdStatus::Saving {
                squad_core::runtime::bass_ackwards_prd::save_file(
                    &mut self.router.lock().unwrap(),
                    state.contest_id,
                )
                .unwrap();
            } else {
                let id = EventId::new();
                self.store
                    .commit(id, "fixture-engine".into(), id.to_string(), move |tx, g| {
                        squad_core::router::bass_ackwards_prd::drive_move(tx, g, id, state)
                    })
                    .unwrap();
            }
            if stop == Some(self.state().state) {
                return;
            }
            let ids=self.store.call(|db|{let mut q=db.prepare("SELECT work_id FROM work WHERE status='queued' AND runnable=1 ORDER BY rowid")?;Ok(q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)}).unwrap();
            if ids.is_empty() {
                continue;
            }
            for raw in ids {
                let wid = raw.parse().unwrap();
                let (w, _) = self
                    .store
                    .call(move |db| Ok(squad_store::work::template(db, wid)?.unwrap()))
                    .unwrap();
                let owner = match &w.target {
                    Target::Member(n) => ExecutorKey::Member(n.clone()),
                    Target::Judge => ExecutorKey::Case(w.case_id.unwrap()),
                };
                let result = self
                    .router
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
                    panic!("{result:?}")
                };
                let readers = Arc::new(squad_store::readers::ScopedReaders::new(
                    self.store.clone(),
                    owner.clone(),
                    grant.epoch,
                ));
                let (_, mailbox) = tokio::sync::mpsc::channel(1);
                let context = AgentContext {
                    executor: owner.clone(),
                    mailbox,
                    history: readers.clone(),
                    work: readers,
                    submit: Arc::new(Submit {
                        router: self.router.clone(),
                        owner,
                    }),
                    render: Arc::new(NoRender),
                    cancel: tokio_util::sync::CancellationToken::new(),
                };
                let grant = squad_agents::worker::start_attempt_sized(
                    &context,
                    &grant,
                    grant.work.attempt,
                    None,
                )
                .await
                .unwrap();
                let pack = squad_core::prompts::PromptPack::embedded().unwrap();
                squad_core::runtime::bass_ackwards_prd::execute(
                    &context,
                    &grant,
                    &self.client,
                    &pack,
                    &self.config,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .unwrap();
            }
        }
        panic!("script did not settle");
    }
    fn state(&self) -> BassAckwardsPrdState {
        self.store
            .call(|db| {
                let raw: String = db.query_row(
                    "SELECT state_json FROM contests ORDER BY rowid DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )?;
                Ok(serde_json::from_str(&raw)?)
            })
            .unwrap()
    }
}
#[tokio::test]
async fn owned_engine_and_handlers_complete_28_calls_and_persist_file() {
    let f = Fixture::new().await;
    let h = Harness::new(&f);
    assert!(matches!(
        h.send(
            "@all $bass_ackwards_prd a self-hosted status page for a small SaaS, live with alerts"
        ),
        RequestResult::Accepted { .. }
    ));
    let preflight: Value = h
        .store
        .call(|db| {
            let raw: String = db.query_row("SELECT plan_json FROM contests", [], |r| r.get(0))?;
            Ok(serde_json::from_str(&raw)?)
        })
        .unwrap();
    assert_eq!(preflight["settings"]["memberCalls"], 24);
    assert_eq!(preflight["settings"]["executorCalls"], 0);
    assert_eq!(preflight["settings"]["judgeCalls"], 4);
    h.pump().await;
    let s = h.state();
    assert_eq!(s.state, BassAckwardsPrdStatus::Complete, "{s:?}");
    assert_eq!(s.terminal_reason.unwrap().as_str(), "saved_with_findings");
    for body in f.fake.captured_bodies().await {
        let text = body["messages"].to_string();
        for a in &f.config.agents {
            assert!(!text.contains(&a.nick.to_string()));
            assert!(!text.contains(&a.model));
            assert!(!text.contains(&a.persona));
        }
        let user = body["messages"][1]["content"].as_str().unwrap();
        let data = extract_data(user);
        if let Some(lens) = data["lens"].as_str() {
            for other in ["GAPS", "ATTACK", "READBACK"] {
                assert_eq!(user.contains(other), other == lens);
            }
        }
        if data.get("comments").is_some() {
            assert!(
                !data
                    .to_string()
                    .contains("This quote does not exist anywhere")
            );
        }
    }
    assert_eq!(f.fake.captured_bodies().await.len(), 28);
    let saved = h
        .store
        .call(move |db| {
            squad_store::bass_ackwards_prd::get::<
                squad_protocol::bass_ackwards_prd::BassAckwardsPrdSaved,
            >(db, s.contest_id, "saved")
        })
        .unwrap();
    assert!(
        std::fs::read_to_string(saved.path)
            .unwrap()
            .contains("## Dependency order")
    );
    f.assert_ledger();
}

#[tokio::test]
async fn optional_human_notes_resume_and_conflicts_are_saved() {
    let f = Fixture::new().await;
    let h = Harness::new(&f);
    h.send("/set PRD_HUMAN_REVIEW 1");
    h.send("@all $bass_ackwards_prd live page with alerts");
    h.pump().await;
    assert_eq!(h.state().state, BassAckwardsPrdStatus::AwaitingHuman);
    assert_eq!(f.fake.captured_bodies().await.len(), 28);
    h.send("/answer Include the operations team's deployment constraints");
    assert_eq!(h.state().state, BassAckwardsPrdStatus::AwaitingHuman);
    h.send("/continue");
    h.pump().await;
    assert_eq!(h.state().state, BassAckwardsPrdStatus::Complete);
    assert_eq!(f.fake.captured_bodies().await.len(), 29);
    f.assert_ledger();
}
#[tokio::test]
async fn invalid_member_output_repairs_once_then_fails_quorum_without_file() {
    let f = Fixture::new().await;
    let h = Harness::new(&f);
    h.send("@all $bass_ackwards_prd BROKEN_DONE status page");
    h.pump().await;
    let s = h.state();
    assert_eq!(s.state, BassAckwardsPrdStatus::Failed);
    assert_eq!(s.terminal_reason.unwrap().as_str(), "insufficient_done");
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 12);
    assert!(bodies.iter().any(|b| {
        b["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("Repair your prior response")
    }));
    assert!(!f.config.data_dir.join("exports").exists());
    f.assert_ledger();
}
#[tokio::test]
async fn revision_exhaustion_saves_last_good_version_and_keeps_failure() {
    let f = Fixture::new().await;
    let h = Harness::new(&f);
    h.send("@all $bass_ackwards_prd BROKEN_REVISE status page");
    h.pump().await;
    let s = h.state();
    assert_eq!(s.state, BassAckwardsPrdStatus::Failed);
    assert_eq!(s.terminal_reason.unwrap().as_str(), "judge_failed");
    let saved = h
        .store
        .call(move |db| {
            squad_store::bass_ackwards_prd::get::<
                squad_protocol::bass_ackwards_prd::BassAckwardsPrdSaved,
            >(db, s.contest_id, "saved")
        })
        .unwrap();
    let text = std::fs::read_to_string(saved.path).unwrap();
    assert!(text.contains("Deploy the public status page"));
    assert!(!text.contains("Unauthorized edit"));
    f.assert_ledger();
}
#[tokio::test]
async fn empty_rounds_release_calls_and_save_failure_retains_report() {
    let f = Fixture::new().await;
    let h = Harness::new(&f);
    std::fs::write(f.config.data_dir.join("exports"), "blocks directory").unwrap();
    h.send("@all $bass_ackwards_prd EMPTY_ROUNDS status page");
    h.pump().await;
    let s = h.state();
    assert_eq!(s.state, BassAckwardsPrdStatus::Complete);
    assert_eq!(s.terminal_reason.unwrap().as_str(), "save_failed");
    assert_eq!(f.fake.captured_bodies().await.len(), 26);
    let report = h
        .store
        .call(move |db| {
            squad_store::bass_ackwards_prd::get::<
                squad_protocol::bass_ackwards_prd::BassAckwardsPrdReport,
            >(db, s.contest_id, "report")
        })
        .unwrap();
    assert!(report.save_error.is_some());
    assert!(report.markdown.contains("## Milestones"));
    f.assert_ledger();
}
#[tokio::test]
async fn stop_waiting_human_releases_claim_and_fences_future_work() {
    let f = Fixture::new().await;
    let h = Harness::new(&f);
    h.send("/set PRD_HUMAN_REVIEW 1");
    h.send("@all $bass_ackwards_prd EMPTY_ROUNDS status page");
    h.pump().await;
    assert_eq!(h.state().state, BassAckwardsPrdStatus::AwaitingHuman);
    h.send("/stop");
    assert_eq!(h.state().state, BassAckwardsPrdStatus::Stopped);
    h.pump().await;
    assert_eq!(f.fake.captured_bodies().await.len(), 26);
    f.assert_ledger();
}

#[tokio::test]
async fn every_stage_checkpoint_and_report_before_write_survive_reopen() {
    let f = Fixture::new().await;
    let mut h = Harness::new(&f);
    h.send("@all $bass_ackwards_prd status page with alerts");
    let mut frozen = None;
    for state in [
        BassAckwardsPrdStatus::DefiningDone,
        BassAckwardsPrdStatus::FreezingDone,
        BassAckwardsPrdStatus::BuildingTrees,
        BassAckwardsPrdStatus::Merging,
        BassAckwardsPrdStatus::Reviewing,
        BassAckwardsPrdStatus::Revising,
        BassAckwardsPrdStatus::MustHaves,
        BassAckwardsPrdStatus::Finalizing,
        BassAckwardsPrdStatus::Checking,
        BassAckwardsPrdStatus::Saving,
    ] {
        h.pump_until(Some(state)).await;
        assert_eq!(h.state().state, state);
        let contest = h.state().contest_id;
        let rotation = h
            .store
            .call(move |db| Ok(squad_store::bass_ackwards_prd::setup(db, contest)?.rotation))
            .unwrap();
        if let Some(prior) = &frozen {
            assert_eq!(prior, &rotation);
        } else {
            frozen = Some(rotation);
        }
        if state == BassAckwardsPrdStatus::Saving {
            assert!(!f.config.data_dir.join("exports").exists());
            let report = h
                .store
                .call(move |db| {
                    squad_store::bass_ackwards_prd::get::<
                        squad_protocol::bass_ackwards_prd::BassAckwardsPrdReport,
                    >(db, contest, "report")
                })
                .unwrap();
            assert!(report.markdown.contains("## Milestones"));
        }
        drop(h);
        h = Harness::new(&f);
        squad_core::router::recovery::recover_on_open_mode(
            &h.store,
            OpenMode::Interactive,
            chrono::Utc::now(),
        )
        .unwrap();
    }
    h.pump().await;
    assert_eq!(h.state().state, BassAckwardsPrdStatus::Complete);
    assert_eq!(f.fake.captured_bodies().await.len(), 28);
    f.assert_ledger();
}

#[tokio::test]
async fn accepted_sparse_revision_and_final_addition_are_saved() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(
        &f.config,
        "@all $bass_ackwards_prd ACCEPT_PATCH status page",
    ))
    .await;
    f.wait(|| {
        f.db()
            .query_row("SELECT COUNT(*) FROM contests WHERE active=0", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            == 1
    })
    .await;
    let db = f.db();
    let state: String = db
        .query_row("SELECT state_json FROM contests", [], |r| r.get(0))
        .unwrap();
    let state: BassAckwardsPrdState = serde_json::from_str(&state).unwrap();
    assert_eq!(state.state, BassAckwardsPrdStatus::Complete, "{state:?}");
    let version: String = db
        .query_row(
            "SELECT body FROM bass_ackwards_prd_records WHERE record_key='version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, "3");
    let raw: String = db
        .query_row(
            "SELECT body FROM bass_ackwards_prd_records WHERE record_key='saved'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let saved: BassAckwardsPrdSaved = serde_json::from_str(&raw).unwrap();
    let text = std::fs::read_to_string(saved.path).unwrap();
    assert!(text.contains("Tested sparse revision"));
    assert!(text.contains("Publish operations guide"));
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
