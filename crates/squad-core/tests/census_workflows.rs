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
#[derive(Clone)]
struct Answers;
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"][1]["content"].as_str().unwrap();
        if body["response_format"]["json_schema"]["name"] != "census_response" {
            return streaming("ordinary response", "stop");
        }
        let material = extract_data(text);
        let output = if text.contains("Hunt independently") {
            serde_json::json!({"gaps":[{"localId":"one","omission":"No backup process","consequence":"Data may be lost"}]})
        } else if text.contains("Merge the independent findings") {
            serde_json::json!({"ledger":[{"id":"gap","omission":"No backup process","consequence":"Data may be lost","lenses":["failure modes"],"sourceFindIds":material["sourceIds"]}]})
        } else if text.contains("Independently rate") {
            let covered = material["plan"].as_str().unwrap().contains("back up daily");
            let additions = if material["plan"].as_str().unwrap().contains("ADDITION_TEST") {
                vec![
                    serde_json::json!({"localId":"extra","omission":"No sunset process","consequence":"Unbounded operating costs","lens":"the day after success"}),
                ]
            } else {
                vec![]
            };
            serde_json::json!({"ratings":material["ledger"].as_array().unwrap().iter().map(|g|serde_json::json!({"gapId":g["id"],"rating":if covered{"covered"}else{"real"},"reason":"Evidence","citation":if covered{serde_json::json!({"location":"plan","quote":"back up daily"})}else{serde_json::Value::Null}})).collect::<Vec<_>>(),"additions":additions})
        } else if text.contains("Finalize the ledger") {
            let outcomes = material["outcomes"].as_array().unwrap();
            let additions = if material["additionIds"].as_array().unwrap().is_empty() {
                vec![]
            } else {
                vec![
                    serde_json::json!({"id":"addition","omission":"No sunset process","consequence":"Unbounded operating costs","sourceAdditionIds":material["additionIds"]}),
                ]
            };
            serde_json::json!({"confirmed":outcomes.iter().filter(|g|matches!(g["disposition"].as_str(),Some("confirmed"|"unresolved"))).map(|g|serde_json::json!({"gapId":g["gap"]["id"],"priority":"Prevent permanent loss"})).collect::<Vec<_>>(),"covered":outcomes.iter().filter(|g|g["disposition"]=="covered").map(|g|g["gap"]["id"].clone()).collect::<Vec<_>>(),"trivial":outcomes.iter().filter(|g|g["disposition"]=="trivial").map(|g|g["gap"]["id"].clone()).collect::<Vec<_>>(),"additions":additions})
        } else {
            return streaming("ordinary response", "stop");
        };
        streaming(&output.to_string(), "stop")
    }
}
fn extract_data(text: &str) -> serde_json::Value {
    for (i, c) in text.char_indices() {
        if c == '{' {
            let mut stream =
                serde_json::Deserializer::from_str(&text[i..]).into_iter::<serde_json::Value>();
            if let Some(Ok(v)) = stream.next()
                && v.get("plan").is_some()
            {
                return v;
            }
        }
    }
    panic!("No mode data in prompt: {text}")
}
#[tokio::test]
async fn complete_pass_wait_continue_restart_and_clear() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@all $census Build a database service."))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("census.report") == 1).await;
    let (contest, version): (String, u64) = f
        .db()
        .query_row(
            "SELECT contest_id,state_version FROM contests WHERE active=1",
            [],
            |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)),
        )
        .unwrap();
    let count = f.fake.captured_bodies().await.len();
    assert_eq!(count, 14);
    let raw: String = f
        .db()
        .query_row(
            "SELECT payload FROM events WHERE kind='census.report'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let report: CensusReport = serde_json::from_str(&raw).unwrap();
    assert_eq!(report.f1, 0);
    assert_eq!(report.gaps[0].finders.len(), 6);
    assert_eq!(report.estimate_low, None);
    assert!(!report.quiet());
    report.validate().unwrap();
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 14);
    let result = room
        .submit(Request::CensusContinue {
            request_id: EventId::new(),
            contest_id: contest.parse().unwrap(),
            expected_state_version: version,
            plan_event_id: None,
            replacement_text: None,
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("census.report") == 2).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 28);
    let raw: String = f
        .db()
        .query_row(
            "SELECT payload FROM events WHERE kind='census.report' ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let report: CensusReport = serde_json::from_str(&raw).unwrap();
    assert_eq!(report.discovery[1].new_confirmed, 0);
    let bodies = f.fake.captured_bodies().await;
    for body in bodies {
        let text = body["messages"].to_string();
        if text.contains("Independently rate") {
            let material = extract_data(body["messages"][1]["content"].as_str().unwrap());
            let data = material.to_string();
            assert!(!data.contains("sourceFindIds"));
            assert!(!data.contains("lenses"));
            assert!(!data.contains("finders"));
            for i in 0..6 {
                assert!(!data.contains(&format!("member{i}")));
            }
        }
    }
    assert!(matches!(
        room.submit(Request::Clear {
            request_id: EventId::new()
        })
        .await,
        RequestResult::Accepted { .. }
    ));
    let count: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM census_passes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct JudgeFault {
    remaining: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl Respond for JudgeFault {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        use std::sync::atomic::Ordering;
        if self
            .remaining
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            streaming("{\"unseen\":0}", "stop")
        } else {
            Answers.respond(r)
        }
    }
}
impl Fixture {
    fn status(&self) -> String {
        self.db().query_row("SELECT json_extract(state_json,'$.status') FROM contests ORDER BY rowid DESC LIMIT 1",[],|r|r.get(0)).unwrap()
    }
    fn report(&self) -> CensusReport {
        let raw: String = self
            .db()
            .query_row(
                "SELECT payload FROM events WHERE kind='census.report' ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }
    async fn judge_fault(&self, count: usize) {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .and(wiremock::matchers::body_partial_json(
                serde_json::json!({"model":"fixture/judge"}),
            ))
            .respond_with(JudgeFault {
                remaining: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(count)),
            })
            .with_priority(1)
            .mount(&self.fake.server)
            .await;
    }
}
#[tokio::test]
async fn skipped_member_and_judge_repair_preserve_missing_evidence() {
    let f = Fixture::new().await;
    f.judge_fault(1).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"model":"fixture/member0"}),
        ))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(serde_json::json!({"error":{"message":"upstream exploded"}})),
        )
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@all $census Build a database service."))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("census.report") == 1).await;
    let report = f.report();
    assert_eq!(report.missing.len(), 2);
    assert_eq!(report.gaps[0].finders.len(), 5);
    assert_eq!(report.gaps[0].votes, 5);
    assert_eq!(report.gaps[0].disposition, CensusDisposition::Unresolved);
    assert!(!report.quiet());
    let repairs: i64 = f
        .db()
        .query_row("SELECT COUNT(*) FROM judge_repairs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(repairs, 1);
    assert_provider_failure(&f.db(), 500, Some("upstream exploded"));
    assert_eq!(f.fake.captured_bodies().await.len(), 17);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn judge_exhaustion_and_daily_refusal() {
    let f = Fixture::new().await;
    f.judge_fault(3).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $census Build a database service."))
        .await;
    f.wait(|| f.status() == "FAILED").await;
    assert_eq!(f.fake.captured_bodies().await.len(), 8);
    assert_eq!(f.count("census.report"), 0);
    room.shutdown().await.unwrap();
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.001;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@all $census Build a database service."))
        .await;
    assert!(
        matches!(
            result,
            RequestResult::Rejected {
                error: ErrorPayload {
                    code: ErrorCode::DAILY_MAX_REACHED,
                    ..
                },
                ..
            }
        ),
        "{result:?}"
    );
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct SlowOnce(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Respond for SlowOnce {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        if !self.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
            Answers.respond(r).set_delay(Duration::from_secs(5))
        } else {
            Answers.respond(r)
        }
    }
}
#[tokio::test]
async fn restart_mid_hunt_reuses_accepted_outputs_and_stop_fences_work() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"model":"fixture/member0"}),
        ))
        .respond_with(SlowOnce(std::sync::Arc::new(
            std::sync::atomic::AtomicBool::new(false),
        )))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $census Build a database service."))
        .await;
    f.wait(|| {
        let n: i64 = f
            .db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE kind='census.hunt' AND status='completed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        n == 5
    })
    .await;
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let epoch: i64 = f
        .db()
        .query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))
        .unwrap();
    let own: String = f
        .db()
        .query_row(
            "SELECT work_id FROM work WHERE kind='census.hunt' AND target='member0'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let peer: String = f
        .db()
        .query_row(
            "SELECT work_id FROM work WHERE kind='census.hunt' AND target='member1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let readers = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member(nick("member0")),
        epoch as u64,
    );
    let values = readers
        .prompt_values_for_work(own.parse().unwrap())
        .await
        .unwrap();
    let projection: serde_json::Value = serde_json::from_str(&values["mode_data_json"]).unwrap();
    assert_eq!(projection.as_object().unwrap().len(), 3);
    assert!(!projection.to_string().contains("finds"));
    assert!(matches!(
        readers.prompt_values_for_work(peer.parse().unwrap()).await,
        Err(ReaderError::Forbidden)
    ));
    assert!(matches!(
        readers.for_work(peer.parse().unwrap()).await,
        Err(ReaderError::Forbidden)
    ));
    assert!(matches!(readers.get(WorkId::new()).await, Ok(None)));
    for h in readers.for_work(own.parse().unwrap()).await.unwrap() {
        assert!(matches!(h.meta, HistoryMetadata::Anonymous { .. }));
        assert_eq!(h.name.as_deref(), Some("squad"));
    }
    drop(store);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.count("census.report") == 1).await;
    let bodies = f.fake.captured_bodies().await;
    for i in 1..6 {
        assert_eq!(
            bodies
                .iter()
                .filter(|b| b["model"] == format!("fixture/member{i}")
                    && b["messages"][1]["content"]
                        .as_str()
                        .unwrap()
                        .contains("Hunt independently"))
                .count(),
            1
        );
    }
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
    assert_eq!(f.status(), "STOPPED");
    let contest: String = f
        .db()
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='census'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let export = room
        .export(ExportScope::Census, Some(contest.parse().unwrap()))
        .await
        .unwrap();
    assert!(export.contains(CENSUS_RANGE_UNAVAILABLE));
    assert!(export.contains("Finder"));
    assert!(!export.contains("tieBreakSeed"));
    assert!(!export.contains("census.prompts"));
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn revised_plan_preflight_idempotency_and_persisted_variable_precedence() {
    let mut f = Fixture::new().await;
    f.config.defaults.census_passes = 2;
    let room = f.open(OpenMode::Interactive).await;
    let first = request(&f.config, "@all $census Build a database service.");
    let accepted = room.submit(first.clone()).await;
    let duplicate = room.submit(first).await;
    assert_eq!(accepted, duplicate);
    f.wait(|| f.count("census.report") == 1).await;
    let (contest, version): (String, i64) = f
        .db()
        .query_row(
            "SELECT contest_id,state_version FROM contests WHERE active=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let continuation = Request::CensusContinue {
        request_id: EventId::new(),
        contest_id: contest.parse().unwrap(),
        expected_state_version: version as u64,
        plan_event_id: None,
        replacement_text: Some(
            Text::new(
                "Build a database service with replication, monitoring, and tested daily backups.",
            )
            .unwrap(),
        ),
    };
    let result = room.submit(continuation.clone()).await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert_eq!(result, room.submit(continuation).await);
    f.wait(|| f.count("census.report") == 2).await;
    assert_eq!(f.count("contest.preflight"), 2);
    assert_eq!(f.report().plan_version, 2);
    assert_eq!(f.fake.captured_bodies().await.len(), 28);
    let raw: String = f
        .db()
        .query_row(
            "SELECT payload FROM events WHERE kind='census.report' ORDER BY seq LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<CensusReport>(&raw)
            .unwrap()
            .plan_version,
        1
    );
    let state = room.state().await;
    assert!(state.pending_preflights.is_empty());
    assert!(matches!(
        room.submit(Request::SetVars {
            request_id: EventId::new(),
            updates: vec![VariableUpdate {
                name: VariableName::CensusPasses,
                value: 3
            }]
        })
        .await,
        RequestResult::Accepted { .. }
    ));
    room.shutdown().await.unwrap();
    f.config.defaults.census_passes = 5;
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.state().await.vars.census_passes, 3);
    room.submit(Request::Clear {
        request_id: EventId::new(),
    })
    .await;
    assert_eq!(room.state().await.vars.census_passes, 5);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn stop_during_hunt_cancels_all_outstanding_slots() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(
            serde_json::json!({"model":"fixture/member0"}),
        ))
        .respond_with(SlowOnce(std::sync::Arc::new(
            std::sync::atomic::AtomicBool::new(false),
        )))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $census Build a database service."))
        .await;
    f.wait(|| {
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE kind='census.hunt' AND status='completed'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            == 5
    })
    .await;
    let result = room
        .submit(Request::Stop {
            request_id: EventId::new(),
            target: Some(nick("member0")),
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert_eq!(f.status(), "STOPPED");
    assert_eq!(f.count("census.report"), 0);
    let pending: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM work WHERE status IN ('queued','running')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending, 0);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(f.status(), "STOPPED");
    assert_eq!(f.count("census.report"), 0);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn latest_member_and_quoted_plans_use_all_room_members() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@member0 Propose a service."))
        .await;
    f.wait(|| f.count("message.final") == 1).await;
    let event = room
        .page_events(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
        .unwrap();
    let EnvelopePayload::MessageFinal(reply) = &event.content else {
        unreachable!()
    };
    let mut ctx = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    ctx.records.push(RecordSummary {
        event_id: event.base.id,
        message_id: Some(reply.completed.generation.message_id),
        seq: event.commit.seq,
        kind: Text::new("message.final").unwrap(),
        author: event.base.sender.clone(),
        recipients: event.base.recipients.clone(),
        content: reply.content.clone(),
        in_reply_to_message_id: Some(reply.in_reply_to_message_id),
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    });
    for (index, command) in ["@member0 $census", "@all $census"].into_iter().enumerate() {
        let meta = if index == 0 {
            parser::SubmissionMeta::default()
        } else {
            parser::SubmissionMeta {
                quote: vec![event.base.id],
                reply_to: vec![],
            }
        };
        let parsed = parse(command, &meta, &ctx).unwrap();
        let result = room
            .submit(Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap())
            .await;
        assert!(
            matches!(result, RequestResult::Accepted { .. }),
            "{result:?}"
        );
        f.wait(|| f.count("census.report") == index as i64 + 1)
            .await;
        let raw: String = f
            .db()
            .query_row(
                "SELECT payload FROM events WHERE kind='census.setup' ORDER BY seq DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let setup: CensusSetup = serde_json::from_str(&raw).unwrap();
        assert_eq!(setup.participants.len(), 6);
        assert_eq!(setup.plan.text.as_str(), reply.content);
        assert_eq!(setup.plan.source_event_ids, vec![event.base.id]);
        if index == 1 {
            let (contest, version): (String, i64) = f
                .db()
                .query_row(
                    "SELECT contest_id,state_version FROM contests WHERE active=1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let result = room
                .submit(Request::CensusContinue {
                    request_id: EventId::new(),
                    contest_id: contest.parse().unwrap(),
                    expected_state_version: version as u64,
                    plan_event_id: Some(event.base.id),
                    replacement_text: None,
                })
                .await;
            assert!(
                matches!(result, RequestResult::Accepted { .. }),
                "{result:?}"
            );
            f.wait(|| f.count("census.report") == 3).await;
            assert_eq!(f.report().plan_version, 2);
        }
        room.submit(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    }
    assert_eq!(f.fake.captured_bodies().await.len(), 43);
    let report_event = room
        .page_events(None, 500)
        .await
        .unwrap()
        .into_iter()
        .rev()
        .find(|e| matches!(e.content, EnvelopePayload::CensusReport(_)))
        .unwrap();
    let automatic:i64=f.db().query_row("SELECT COUNT(*) FROM history WHERE context_id='member0' AND content LIKE '%CENSUS · pass%'",[],|r|r.get(0)).unwrap();
    assert_eq!(automatic, 0);
    ctx.records.push(RecordSummary {
        event_id: report_event.base.id,
        message_id: None,
        seq: report_event.commit.seq,
        kind: Text::new("census.report").unwrap(),
        author: Actor::System,
        recipients: vec![Recipient::Me],
        content: "Census report".into(),
        in_reply_to_message_id: None,
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    });
    let meta = parser::SubmissionMeta {
        quote: vec![report_event.base.id],
        reply_to: vec![],
    };
    let parsed = parse("@member0 Inspect this report.", &meta, &ctx).unwrap();
    let result = room
        .submit(Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap())
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("message.final") == 2).await;
    let quoted:i64=f.db().query_row("SELECT COUNT(*) FROM history WHERE context_id='member0' AND content LIKE '%CENSUS · pass%'",[],|r|r.get(0)).unwrap();
    assert_eq!(quoted, 1);

    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn additions_never_inflate_initial_finders_and_covered_majority_keeps_citations() {
    for plan in [
        "ADDITION_TEST Build a database.",
        "Build a database and back up daily.",
    ] {
        let f = Fixture::new().await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, &format!("@all $census {plan}")))
            .await;
        f.wait(|| f.count("census.report") == 1).await;
        let report = f.report();
        report.validate().unwrap();
        assert_eq!(report.gaps[0].finders.len(), 6);
        if plan.contains("ADDITION_TEST") {
            assert_eq!(report.additions.len(), 1);
            assert_eq!(report.additions[0].source_addition_ids.len(), 6);
            assert_eq!(report.discovery[0].new_confirmed, 2);
        } else {
            assert_eq!(report.gaps[0].disposition, CensusDisposition::Covered);
            assert_eq!(report.gaps[0].citations.len(), 6);
            assert!(report.input_gap_ids.is_empty());
            assert_eq!(report.unseen, 0.0);
            assert!(!report.quiet());
        }
        let finders: i64 = f
            .db()
            .query_row("SELECT COUNT(*) FROM census_finders", [], |r| r.get(0))
            .unwrap();
        assert_eq!(finders, 6);
        room.shutdown().await.unwrap();
    }
}
#[derive(Clone)]
struct DelayStep {
    needle: &'static str,
    seen: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Respond for DelayStep {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let matches = body["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains(self.needle)
            && (body["model"] == "fixture/judge" || body["model"] == "fixture/member0");
        let response = Answers.respond(r);
        if matches && !self.seen.swap(true, std::sync::atomic::Ordering::SeqCst) {
            response.set_delay(Duration::from_secs(5))
        } else {
            response
        }
    }
}
#[tokio::test]
async fn restart_and_stop_each_later_step_preserve_committed_inputs_and_fences() {
    for needle in [
        "Merge the independent findings",
        "Independently rate",
        "Finalize the ledger",
    ] {
        for restart in [true, false] {
            let f = Fixture::new().await;
            let seen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            Mock::given(method("POST"))
                .and(path("/api/v1/chat/completions"))
                .respond_with(DelayStep {
                    needle,
                    seen: seen.clone(),
                })
                .with_priority(1)
                .mount(&f.fake.server)
                .await;
            let room = f.open(OpenMode::Interactive).await;
            room.submit(request(
                &f.config,
                "@all $census Build for MEMBER0 and PERSONA_0 using fixture/member0.",
            ))
            .await;
            f.wait(|| seen.load(std::sync::atomic::Ordering::SeqCst) && (needle!="Independently rate" || f.db().query_row("SELECT COUNT(*) FROM work WHERE kind='census.rate' AND status='completed'",[],|r|r.get::<_,i64>(0)).unwrap()==5)).await;
            if restart {
                room.shutdown().await.unwrap();
                let changed = f._dir.path().join("changed-prompts");
                std::fs::create_dir_all(&changed).unwrap();
                let (file, original) = match needle {
                    "Merge the independent findings" => (
                        "census.merge.md",
                        include_str!("../../../prompts/census.merge.md"),
                    ),
                    "Independently rate" => (
                        "census.rate.md",
                        include_str!("../../../prompts/census.rate.md"),
                    ),
                    _ => (
                        "census.final.md",
                        include_str!("../../../prompts/census.final.md"),
                    ),
                };
                std::fs::write(
                    changed.join(file),
                    format!("CHANGED_PROMPT_CANARY\n{original}"),
                )
                .unwrap();
                let mut changed_config = f.config.clone();
                changed_config.prompts.directory = Some(changed);
                let room = RoomHandle::open_with_client(
                    changed_config,
                    "main",
                    OpenMode::Interactive,
                    OpenRouterClient::new(&f.fake.base_url(), "fake-key").unwrap(),
                )
                .await
                .unwrap();

                f.wait(|| f.count("census.report") == 1).await;
                let bodies = f.fake.captured_bodies().await;
                assert!(
                    bodies
                        .iter()
                        .all(|b| !b["messages"].to_string().contains("CHANGED_PROMPT_CANARY"))
                );
                assert_eq!(bodies.len(), 15, "{needle}");
                let target = if needle == "Independently rate" {
                    "fixture/member0"
                } else {
                    "fixture/judge"
                };
                let calls = bodies
                    .iter()
                    .filter(|b| {
                        b["model"] == target
                            && b["messages"][1]["content"]
                                .as_str()
                                .unwrap()
                                .contains(needle)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0]["messages"], calls[1]["messages"]);
                for body in &bodies {
                    let data = extract_data(body["messages"][1]["content"].as_str().unwrap());
                    let plan = data["plan"].as_str().unwrap();
                    assert!(!plan.contains("MEMBER0"));
                    assert!(!plan.contains("PERSONA_0"));
                    assert!(!plan.contains("fixture/member0"));
                }
                room.shutdown().await.unwrap();
            } else {
                room.submit(Request::Stop {
                    request_id: EventId::new(),
                    target: None,
                })
                .await;
                assert_eq!(f.status(), "STOPPED");
                assert_eq!(f.count("census.report"), 0);
                let mut q = f.db();
                let tx = q.transaction().unwrap();
                let (work,attempt,fence,epoch):(String,i64,i64,i64)=tx.query_row("SELECT work_id,attempt,lease_fence,epoch FROM work WHERE status='cancelled' ORDER BY rowid DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
                let work: WorkId = work.parse().unwrap();
                let (template, _) = squad_store::work::template(&tx, work).unwrap().unwrap();
                let owner = squad_store::work::owner(&template).unwrap();
                assert!(
                    squad_store::work::check_fence(
                        &tx,
                        work,
                        &owner,
                        attempt as u32,
                        fence.saturating_sub(1) as u64,
                        epoch as u64,
                        chrono::Utc::now()
                    )
                    .is_err()
                );
                tx.commit().unwrap();
                room.shutdown().await.unwrap();
            }
        }
    }
}
#[derive(Clone)]
struct WordyOnce {
    judge: bool,
    seen: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Respond for WordyOnce {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"][1]["content"].as_str().unwrap();
        let selected = if self.judge {
            user.contains("Merge the independent findings")
        } else {
            user.contains("Hunt independently")
        };
        if selected && !self.seen.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let omission = "word ".repeat(1000);
            let output = if self.judge {
                let data = extract_data(user);
                serde_json::json!({"ledger":[{"id":"g1","omission":omission,"consequence":"Loss","lenses":["failure modes"],"sourceFindIds":data["sourceIds"]}]})
            } else {
                serde_json::json!({"gaps":[{"localId":"one","omission":omission,"consequence":"Loss"}]})
            };
            return streaming(&output.to_string(), "stop");
        }
        Answers.respond(r)
    }
}
#[tokio::test]
async fn prose_budget_failures_use_member_plain_retry_and_one_private_judge_repair() {
    for judge in [false, true] {
        let mut f = Fixture::new().await;
        f.config.prompts.limits.room_words = 40;
        f.config.prompts.limits.plan_words = 80;
        let model = if judge {
            "fixture/judge"
        } else {
            "fixture/member0"
        };
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .and(wiremock::matchers::body_partial_json(
                serde_json::json!({"model":model}),
            ))
            .respond_with(WordyOnce {
                judge,
                seen: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .with_priority(1)
            .mount(&f.fake.server)
            .await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, "@all $census Build a database service."))
            .await;
        f.wait(|| f.count("census.report") == 1).await;
        assert!(f.report().missing.is_empty());
        assert_eq!(f.fake.captured_bodies().await.len(), 15);
        let repairs: i64 = f
            .db()
            .query_row("SELECT COUNT(*) FROM judge_repairs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(repairs, i64::from(judge));
        room.shutdown().await.unwrap();
    }
}

fn assert_provider_failure(db: &rusqlite::Connection, status: i64, message: Option<&str>) {
    let rows: Vec<String> = db
        .prepare("SELECT error_json FROM work WHERE json_extract(error_json,'$.providerStatus')=?")
        .unwrap()
        .query_map([status], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        !rows.is_empty(),
        "no work row carries providerStatus {status}"
    );
    for raw in &rows {
        assert!(
            !raw.contains("Bearer") && !raw.contains("fake-key"),
            "{raw}"
        );
        if let Some(message) = message {
            assert!(raw.contains(message), "{raw}");
        }
    }
}
