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
                nick: format!("member{i}").parse().unwrap(),
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
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    fn contest(&self) -> ContestId {
        self.db()
            .query_row(
                "SELECT contest_id FROM contests ORDER BY rowid DESC LIMIT 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
            .parse()
            .unwrap()
    }
}
const SOURCE: &str = "Records are retained for thirty days. Backups are deleted every day.";
const FABRICATED: &str = "Imaginary fabricated support never in the packet";
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Fabricated,
    Skip,
    Quorum,
    Repair,
    Judge,
    Empty,
    Recover,
    Stop,
    Outer,
    Exhausted,
    Dormant,
}
struct Answers;
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
    panic!("missing frozen input")
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["response_format"]["json_schema"]["name"] != "trace_response" {
            return streaming("ordinary response", "stop");
        }
        let text = body["messages"][1]["content"].as_str().unwrap();
        let data = extract(text);
        let q = data["question"].as_str().unwrap();
        let output = if text.contains("Extract claims answering") {
            if q.contains("Quorum") && body["model"] != "fixture/member0"
                || q.contains("Skip") && body["model"] == "fixture/member5"
            {
                return ResponseTemplate::new(503);
            }
            if q.contains("Repair") && !text.contains("failed validation") {
                serde_json::json!({"wrong":true})
            } else {
                serde_json::json!({"claims":[{"localId":"L1","claim":"Records retained thirty days","quotes":[{"sourceId":"S1","quote":if q.contains("Fabricated"){FABRICATED}else{"Records are retained for thirty days."}}],"counterQuotes":[],"scopeNote":"Retention only"}]})
            }
        } else if text.contains("Merge duplicate extracted") {
            if q.contains("Judge") {
                serde_json::json!({"claims":[{"claimId":"C1","text":"Invented claim","sources":[],"sharedOrigin":[]}]})
            } else if q.contains("Empty") {
                serde_json::json!({"claims":[]})
            } else {
                serde_json::json!({"claims":[{"claimId":"C1","text":"Records retained thirty days","sources":[{"memberLabel":"Reader 1","localId":"L1"},{"memberLabel":"Reader 2","localId":"L1"}],"sharedOrigin":[]}]})
            }
        } else if text.contains("Review only your assigned") {
            serde_json::json!({"verdicts":data["claims"].as_array().unwrap().iter().map(|c|serde_json::json!({"claimId":c["claimId"],"entailment":"supported","scope":"ok","qualifierQuote":null,"note":"Exact scope retained"})).collect::<Vec<_>>()})
        } else {
            let supported = data["ledger"]["claims"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["status"] == "supported");
            serde_json::json!({"sections":if supported{vec![serde_json::json!({"heading":"Retention","paragraphs":[{"text":"Records persist thirty days.","claimIds":["C1"]}]})]}else{vec![]},"disputed":[],"gaps":[]})
        };
        streaming(&output.to_string(), "stop")
    }
}
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
async fn owned_run(fault: Fault) -> (Fixture, squad_store::Store, ContestId) {
    let f = Fixture::new().await;
    std::fs::create_dir_all(&f.config.data_dir).unwrap();
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
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
    let marker = match fault {
        Fault::Fabricated => "Fabricated",
        Fault::Skip => "Skip",
        Fault::Quorum => "Quorum",
        Fault::Repair => "Repair",
        Fault::Judge => "Judge",
        Fault::Empty => "Empty",
        _ => "",
    };
    let text = format!("@all $trace Retention {marker}\n--\n{SOURCE}");
    let parsed = ParsedInput::Trace {
        input: parser::ConversationalInput {
            raw: text.clone(),
            quote: vec![],
            variable_updates: vec![],
        },
        participants: roster.clone(),
        question: format!("Retention {marker}"),
        sources: vec![parser::TraceSource::Pasted {
            text: SOURCE.into(),
        }],
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
            squad_core::router::trace::prepare(
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
    if fault == Fault::Dormant {
        let member = roster[0].clone();
        let binding = bindings[member.as_str()].clone();
        store.call(move|db|{
       let event:String=db.query_row("SELECT id FROM events WHERE kind='trace.setup' ORDER BY seq DESC LIMIT 1",[],|r|r.get(0))?;
       let w=Work{work_id:WorkId::new(),message_id:MessageId::new(),in_reply_to_message_id:Some(MessageId::new()),kind:WorkKind::Reply,target:Target::Member(member.clone()),context_id:ContextId(member.to_string()),trigger_event_id:event.parse().unwrap(),history_through:HistoryPosition::new(1).unwrap(),attempt:1,debate_id:None,round:None,replay:None,case_id:None,prompt_ref:None,max_output_tokens:None,contest_id:None,step_id:None};
       squad_store::work::insert(db,&w,&binding,None)?;
       db.execute("UPDATE work SET runnable=0,scheduling_meta=json_set(scheduling_meta,'$.openModeDormant',1) WHERE work_id=?",[w.work_id.to_string()])?;Ok(())
      }).unwrap();
    }
    if fault == Fault::Outer {
        let room = squad_core::room::RoomHandle::open_with_client(
            f.config.clone(),
            "main",
            OpenMode::Interactive,
            OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap(),
        )
        .await
        .unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let state = squad_core::router::trace::load_state(&f.db(), c).unwrap();
                if state.state.is_terminal() {
                    assert_eq!(state.state, TraceStatus::Complete);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
        .await;
        let captured = f.fake.captured_bodies().await.len();
        let state = squad_core::router::trace::load_state(&f.db(), c).unwrap();
        room.shutdown().await.unwrap();
        assert!(
            result.is_ok(),
            "outer room dispatcher stalled after {captured} provider requests at {:?}",
            state.state
        );
        return (f, store, c);
    }
    let router = std::sync::Arc::new(std::sync::Mutex::new(
        squad_core::router::Router::new(store.clone(), roster, bindings, 200_000_000).unwrap(),
    ));
    let pack = squad_core::prompts::PromptPack::from_override_dir(None).unwrap();
    for _ in 0..30 {
        squad_core::orchestrator::trace::drive(&mut router.lock().unwrap()).unwrap();
        let state = squad_core::router::trace::load_state(&f.db(), c).unwrap();
        if state.state.is_terminal() {
            return (f, store, c);
        }
        if state.state == TraceStatus::Verifying && state.step_id.is_some() && fault == Fault::Stop
        {
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
                .unwrap()
                .result;
            assert!(
                matches!(result, RequestResult::Accepted { .. }),
                "{result:?}"
            );
            return (f, store, c);
        }
        if fault == Fault::Exhausted
            && state.state == TraceStatus::Verifying
            && state.step_id.is_some()
        {
            store.call(move|db|{db.execute("UPDATE work SET attempt=2 WHERE contest_id=? AND kind='trace.verify' AND status='queued'",[c.to_string()])?;Ok(())}).unwrap();
            squad_core::router::recovery::recover_on_open(&store).unwrap();
            continue;
        }
        if fault == Fault::Recover && state.step_id.is_some() {
            let prior = squad_store::trace::run(&f.db(), c).unwrap();
            squad_core::router::recovery::recover_on_open(&store).unwrap();
            let after = squad_store::trace::run(&f.db(), c).unwrap();
            assert_eq!(
                serde_json::to_value(&prior).unwrap(),
                serde_json::to_value(&after).unwrap()
            );
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
            squad_core::runtime::trace::execute(
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
async fn owned_real_executors_complete_fourteen_calls_and_export() {
    let (f, store, c) = owned_run(Fault::None).await;
    let state = squad_core::router::trace::load_state(&f.db(), c).unwrap();
    assert_eq!(state.state, TraceStatus::Complete);
    let stored_artifacts: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM contest_artifacts WHERE contest_id=?",
            [c.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state.artifact_event_ids.len() as i64, stored_artifacts);
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 14);
    let run = squad_store::trace::run(&f.db(), c).unwrap();
    let maps:i64=f.db().query_row("SELECT COUNT(*) FROM contest_private_maps WHERE contest_id=? AND scope_key IN ('trace.seed','trace.readers','trace.verifiers')",[c.to_string()],|r|r.get(0)).unwrap();
    assert_eq!(maps, 3);
    assert_eq!(
        run.ledger.claims[0].status,
        trace::TraceClaimStatus::Supported
    );
    assert_eq!(run.assignments.values().map(Vec::len).sum::<usize>(), 2);
    let plan: String = f
        .db()
        .query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [c.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    let p: ContestPreflight = serde_json::from_str(&plan).unwrap();
    assert_eq!(p.estimate.model_calls, 14);
    assert_eq!(p.settings["reservedAttempts"], 30);
    for b in &bodies {
        let t = b["messages"][1]["content"].as_str().unwrap();
        if t.contains("Review only your assigned") {
            assert!(!t.contains("memberLabel"));
            assert!(!t.contains("Reader 1"));
        }
        if t.contains("Merge duplicate extracted") {
            let sys = b["messages"][0]["content"].as_str().unwrap();
            assert!(!sys.contains("@fable"));
            assert!(!sys.contains("fixture/judge"));
        }
    }
    let reserved: i64 = f
        .db()
        .query_row(
            "SELECT COALESCE(SUM(amount_micros),0) FROM reservations",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reserved, 0);
    let events = store
        .call(move |db| squad_store::export::contest(db, c))
        .unwrap();
    let snapshot = squad_core::export::ExportSnapshot {
        events,
        ..Default::default()
    };
    let markdown = squad_core::export::trace::render(&snapshot, c).unwrap();
    assert!(markdown.contains("[C1]"));
    assert!(markdown.contains(SOURCE));
    assert!(markdown.contains("Every verdict"));
}
#[tokio::test]
async fn fabricated_quotes_have_no_repair_and_never_reach_later_prompts() {
    let (f, _, c) = owned_run(Fault::Fabricated).await;
    let r = squad_store::trace::run(&f.db(), c).unwrap();
    assert_eq!(r.quote_checks.len(), 6);
    assert!(r.quote_checks.iter().all(|q| q.status == "quote_not_found"));
    assert_eq!(
        r.ledger.claims[0].status,
        trace::TraceClaimStatus::Unsupported
    );
    let bodies = f.fake.captured_bodies().await;
    assert_eq!(bodies.len(), 14);
    for b in bodies {
        let text = b["messages"][1]["content"].as_str().unwrap();
        assert!(!text.contains(FABRICATED));
    }
}
#[tokio::test]
async fn skips_quorum_empty_judge_failure_and_member_repairs() {
    for (fault, status, reason, calls) in [
        (Fault::Skip, TraceStatus::Complete, "complete", 15),
        (
            Fault::Quorum,
            TraceStatus::Failed,
            "insufficient_extractions",
            11,
        ),
        (Fault::Empty, TraceStatus::Complete, "empty_claims", 7),
        (Fault::Judge, TraceStatus::Failed, "judge_failed", 8),
        (Fault::Repair, TraceStatus::Complete, "complete", 20),
    ] {
        let (f, _, c) = owned_run(fault).await;
        let s = squad_core::router::trace::load_state(&f.db(), c).unwrap();
        assert_eq!(s.state, status);
        assert_eq!(s.terminal_reason.unwrap().as_str(), reason);
        assert_eq!(f.fake.captured_bodies().await.len(), calls);
    }
}
#[tokio::test]
async fn stop_and_startup_recovery_preserve_private_assignments() {
    let (f, _, c) = owned_run(Fault::Stop).await;
    assert_eq!(
        squad_core::router::trace::load_state(&f.db(), c)
            .unwrap()
            .state,
        TraceStatus::Stopped
    );
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE contest_id=? AND status IN ('queued','running')",
                [c.to_string()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let db = f.db();
    let mut query = db
        .prepare("SELECT work_id FROM work WHERE contest_id=? AND status='cancelled'")
        .unwrap();
    for raw in query
        .query_map([c.to_string()], |r| r.get::<_, String>(0))
        .unwrap()
    {
        let id: WorkId = raw.unwrap().parse().unwrap();
        let work = squad_store::work::template(&db, id).unwrap().unwrap().0;
        let owner = squad_store::work::owner(&work).unwrap();
        assert!(matches!(
            squad_store::work::check_fence(&db, id, &owner, 1, 1, 1, chrono::Utc::now()),
            Err(squad_store::StoreError::Domain(
                ErrorCode::STALE_WORK_ATTEMPT,
                _
            ))
        ));
    }
    let (f, _, c) = owned_run(Fault::Recover).await;
    assert_eq!(
        squad_core::router::trace::load_state(&f.db(), c)
            .unwrap()
            .state,
        TraceStatus::Complete
    );
}

#[tokio::test]
#[ignore = "Integrator-owned runtime/judge.rs must route new_mode_judge() through its structured runner"]
async fn full_room_runtime_completes_trace_after_outer_dispatch_integration() {
    let (f, _, _) = owned_run(Fault::Outer).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 14);
}

#[tokio::test]
async fn recovered_exhausted_verifiers_settle_without_replaying_provider_calls() {
    let (f, _, c) = owned_run(Fault::Exhausted).await;
    assert_eq!(
        squad_core::router::trace::load_state(&f.db(), c)
            .unwrap()
            .state,
        TraceStatus::Complete
    );
    let run = squad_store::trace::run(&f.db(), c).unwrap();
    assert!(run.verifications.is_empty());
    assert_eq!(
        run.ledger.claims[0].status,
        trace::TraceClaimStatus::Unverified
    );
    assert_eq!(run.skipped.len(), 6);
    assert_eq!(f.fake.captured_bodies().await.len(), 8);
    let queued: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM work WHERE contest_id=? AND status IN ('queued','running')",
            [c.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(queued, 0);
}

#[tokio::test]
async fn dormant_ordinary_work_does_not_block_trace_admission() {
    let (f, _, c) = owned_run(Fault::Dormant).await;
    assert_eq!(
        squad_core::router::trace::load_state(&f.db(), c)
            .unwrap()
            .state,
        TraceStatus::Complete
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 14);
    assert_eq!(f.db().query_row("SELECT COUNT(*) FROM work WHERE kind='reply' AND status='queued' AND runnable=0 AND json_extract(scheduling_meta,'$.openModeDormant')=1",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
