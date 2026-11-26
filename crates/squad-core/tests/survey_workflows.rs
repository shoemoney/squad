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
        config.budget.daily_max_usd = 100.;
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
fn task_data(body: &serde_json::Value) -> serde_json::Value {
    body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .flat_map(|m| m["content"].as_str().unwrap().lines())
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .find(|v| v.get("answers").is_some() || v.get("claims").is_some() || v.get("map").is_some())
        .unwrap()
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        use serde_json::json;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let text = body["messages"].to_string();
        let value = if text.contains("extract atomic claims") {
            let d = task_data(&body);
            json!({"claims":[{"id":"c1","claim":"Testing catches regressions.","sourceAnswerIds":[d["answers"][0]["messageId"].clone()]}]})
        } else if text.contains("independent claim vote")
            || text.contains("reconsider selected claims")
        {
            let second = text.contains("reconsider selected claims");
            let d = task_data(&body);
            json!({"votes":d["claims"].as_array().unwrap().iter().map(|c| {let mut b=json!({"claimId":c["claimId"],"vote":"AGREE","reason":"Tests detect changed behavior.","confidence":4});if second {b["changeEvidence"]=json!("A controlled counterexample");}b}).collect::<Vec<_>>()})
        } else if text.contains("write the human's claim map") {
            let d = task_data(&body);
            let entries = |band: &str| {
                d["map"].as_array().unwrap().iter().filter(|m|m["bands"].as_array().unwrap().contains(&json!(band))).map(|m|json!({"claimId":m["claimId"],"explanation":"Recorded votes support this classification.","settlingQuestion":"What reproducible test would distinguish the positions?"})).collect::<Vec<_>>()
            };
            json!({"consensus":entries("consensus"),"contested":entries("contested"),"loneInsights":entries("loneInsights"),"loneDissents":entries("loneDissents"),"rejectedOutliers":entries("rejectedOutliers"),"limitations":"Models share blind spots; missing answers, votes and revotes remain missing."})
        } else {
            return streaming("Testing catches regressions. PRIVATE_WHOLE_ANSWER", "stop");
        };
        structured_judge(value)
    }
}
#[tokio::test]
async fn question_first_survey_runs_real_engine_and_private_fanouts() {
    let _ = tracing_subscriber::fmt().try_init();
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(
            &f.config,
            "@all $survey Which checks improve reliability?",
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("survey.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 20);
    assert_eq!(f.count("survey.vote"), 6);
    assert_eq!(f.count("survey.revote"), 6);
    f.assert_ledger();
    let events = room.events_after(None, 500).await.unwrap();
    let report = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::SurveyReport(r) = &e.content {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(&report.map[0].claim_id)
            .unwrap()
            .get_version_num(),
        4
    );
    assert_eq!(report.map[0].final_tally.agree, 6);
    assert_eq!(report.report.consensus.len(), 1);
    let mut groups =
        std::collections::BTreeMap::<i64, (u32, std::collections::BTreeSet<u32>)>::new();
    for e in &events {
        let start = e.commit.seq.get() - i64::from(e.commit.commit_index);
        let group = groups
            .entry(start)
            .or_insert((e.commit.commit_size, Default::default()));
        assert_eq!(group.0, e.commit.commit_size);
        assert!(group.1.insert(e.commit.commit_index));
    }
    for (size, indexes) in groups.values() {
        assert_eq!(indexes.len(), *size as usize);
    }

    for b in f.fake.captured_bodies().await {
        let text = b["messages"].to_string();
        if text.contains("extract atomic claims") {
            // Providers reject bound-heavy schemas (Gemini: "too many states"),
            // so the wire schema carries no maxItems. The prompt states the cap
            // and the local validator enforces it.
            assert_eq!(
                b["response_format"]["json_schema"]["schema"]["properties"]["claims"]["type"],
                "array"
            );
            assert!(
                b["response_format"]["json_schema"]["schema"]["properties"]["claims"]
                    .get("maxItems")
                    .is_none()
            );
            assert!(text.contains("up to\\n25 distinct claims"), "{text}");
        }
        if b["model"] == "fixture/judge" {
            assert!(!text.contains("PERSONA_"));
            assert_eq!(b["messages"].as_array().unwrap().len(), 2);
        }
        if text.contains("independent claim vote") || text.contains("reconsider selected claims") {
            assert!(!text.contains("PRIVATE_WHOLE_ANSWER"));
            assert!(!text.contains("sourceAnswerIds"));
            assert!(!text.contains("finalTally"));
            let schema = &b["response_format"]["json_schema"]["schema"];
            // Bounds stay out of the wire schema; the ballot validator enforces them.
            assert_eq!(schema["properties"]["votes"]["type"], "array");
            assert!(schema["properties"]["votes"].get("maxItems").is_none());
            assert_eq!(
                schema["$defs"]["SurveyBallotItem"]["properties"]["confidence"]["type"],
                "integer"
            );
            assert_eq!(b["messages"].as_array().unwrap().len(), 2);
        }
    }
    room.shutdown().await.unwrap();
}
async fn reuse_request(room: &RoomHandle, config: &SquadConfig) -> squad_protocol::Request {
    let events = room.events_after(None, 500).await.unwrap();
    let records = events
        .iter()
        .filter_map(|e| {
            let (kind, message_id, content, in_reply_to_message_id) = match &e.content {
                EnvelopePayload::UserMessage(u) => (
                    "user.message",
                    Some(u.input.message_id),
                    u.body.clone(),
                    None,
                ),
                EnvelopePayload::MessageFinal(v) => (
                    "message.final",
                    Some(v.completed.generation.message_id),
                    v.content.clone(),
                    Some(v.in_reply_to_message_id),
                ),
                _ => return None,
            };
            Some(parser::RecordSummary {
                event_id: e.base.id,
                message_id,
                seq: e.commit.seq,
                kind: Text::new(kind).unwrap(),
                author: e.base.sender.clone(),
                recipients: e.base.recipients.clone(),
                content,
                in_reply_to_message_id,
                contest_id: None,
                case_id: None,
                plan_event_id: None,
                grid_event_id: None,
                cell_ids: vec![],
                state_version: None,
                superseded: false,
                eligible: true,
            })
        })
        .collect();
    let ctx = parser::ParseContext {
        roster: config.agents.iter().map(|a| a.nick.clone()).collect(),
        records,
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    squad_protocol::Request::from_parsed(
        EventId::new(),
        parse("@all $survey", &meta, &ctx).unwrap(),
        &meta,
        false,
    )
    .unwrap()
}
#[tokio::test]
async fn reuse_freezes_only_latest_question_without_replacement_answers_and_exports() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all unrelated old question"))
        .await;
    f.wait(|| f.count("message.final") == 6).await;
    room.submit(request(&f.config, "@all current question"))
        .await;
    f.wait(|| f.count("message.final") == 12).await;
    let req = reuse_request(&room, &f.config).await;
    assert!(matches!(
        room.submit(req).await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| f.count("survey.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 26);
    let events = room.events_after(None, 500).await.unwrap();
    let setup = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::SurveySetup(s) = &e.content {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert!(!setup.collect);
    assert_eq!(setup.answers.len(), 6);
    assert_eq!(setup.question.as_str(), "current question");
    let exported = squad_core::export::render(
        &squad_core::export::ExportSnapshot {
            events: events.clone(),
            ..Default::default()
        },
        ExportScope::Survey,
        Some(setup.contest_id),
    )
    .unwrap();
    for word in [
        "Consensus",
        "Contested",
        "Lone insights",
        "Lone dissents",
        "Rejected outliers",
        "First ballot",
        "Second ballot",
        "confidence 4/5",
        "Origins:",
        "Change evidence:",
        "MISSING 0",
    ] {
        assert!(exported.contains(word), "missing {word}: {exported}");
    }
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct Broken {
    stage: &'static str,
    remaining: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl Respond for Broken {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let b: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if b["messages"].to_string().contains(self.stage)
            && self
                .remaining
                .try_update(
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                    |n| n.checked_sub(1),
                )
                .is_ok()
        {
            return streaming("{invalid JSON", "stop");
        }
        Answers.respond(r)
    }
}
#[tokio::test]
async fn judge_repair_succeeds_and_exhaustion_fails_without_ballot_calls() {
    for failures in [1, 3] {
        let f = Fixture::new().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Broken {
                stage: "extract atomic claims",
                remaining: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(failures)),
            })
            .with_priority(1)
            .mount(&f.fake.server)
            .await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, "@all $survey Which checks help?"))
            .await;
        if failures == 1 {
            f.wait(|| f.count("survey.report") == 1).await;
            assert_eq!(f.fake.captured_bodies().await.len(), 21);
            assert_eq!(
                f.db()
                    .query_row("SELECT COUNT(*) FROM judge_repairs", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        } else {
            f.wait(||f.db().query_row("SELECT COUNT(*) FROM contests WHERE active=0 AND json_extract(state_json,'$.state')='FAILED'",[],|r|r.get::<_,i64>(0)).unwrap()==1).await;
            assert_eq!(f.fake.captured_bodies().await.len(), 9);
            assert_eq!(f.count("survey.vote"), 0);
        }
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}
#[derive(Clone)]
struct MissingMember;
impl Respond for MissingMember {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let b: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if b["model"] == "fixture/member5"
            && (b["messages"].to_string().contains("claim vote")
                || b["messages"]
                    .to_string()
                    .contains("reconsider selected claims"))
        {
            return provider_error(503);
        }
        Answers.respond(r)
    }
}
#[tokio::test]
async fn failed_member_remains_missing_and_never_becomes_unsure_or_a_switch() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(MissingMember)
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $survey Which checks help?"))
        .await;
    f.wait(|| f.count("survey.report") == 1).await;
    let events = room.events_after(None, 500).await.unwrap();
    let r = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::SurveyReport(r) = &e.content {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        r.map[0].final_tally,
        SurveyTally {
            agree: 5,
            disagree: 0,
            unsure: 0,
            missing: 1
        }
    );
    assert!(r.map[0].switches.is_empty());
    assert_eq!(r.map[0].not_revoted, vec![nick("member5")]);
    assert!(r.report.consensus.is_empty());
    let c = r.completeness.as_ref().unwrap();
    assert!(!c.complete);
    assert_eq!(c.missing_votes, vec![nick("member5")]);
    assert_eq!(c.missing_revotes, vec![nick("member5")]);
    assert_provider_failure(&f.db(), 503, None);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn restart_after_assignment_reuses_sources_and_completes_each_call_once() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    let req = request(&f.config, "@all $survey Which checks help?");
    let receipt = room.submit_commit_only(req.clone()).await;
    assert!(
        matches!(receipt, RequestResult::Accepted { .. }),
        "{receipt:?}"
    );
    assert!(f.fake.captured_bodies().await.is_empty());
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.submit(req).await, receipt);
    f.wait(|| f.count("survey.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 20);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 20);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn daily_max_refuses_survey_before_any_provider_call() {
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.001;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@all $survey Which checks help?"))
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
    let RequestResult::Rejected { error, .. } = &result else {
        unreachable!()
    };
    let text = error.text.as_str();
    assert!(
        text.starts_with("Daily max of $") && text.contains(" reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow. (Spent $0.000000 today; this request needs up to $"),
        "{text}"
    );
    assert!(
        text.contains("today; this request needs up to $") && text.contains("reserved"),
        "{text}"
    );
    assert!(f.fake.captured_bodies().await.is_empty());
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn stop_fences_all_survey_work_and_clear_removes_mode_records() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    room.submit_commit_only(request(&f.config, "@all $survey Which checks help?"))
        .await;
    assert!(matches!(
        room.submit(squad_protocol::Request::Stop {
            request_id: EventId::new(),
            target: None
        })
        .await,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE status='cancelled'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        6
    );
    assert_eq!(f.count("survey.report"), 0);
    f.assert_ledger();
    room.submit(squad_protocol::Request::Clear {
        request_id: EventId::new(),
    })
    .await;
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM survey_sources", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct SlowBallots;
impl Respond for SlowBallots {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let response = Answers.respond(r);
        if String::from_utf8_lossy(&r.body).contains("independent claim vote") {
            response.set_delay(Duration::from_secs(1))
        } else {
            response
        }
    }
}
#[tokio::test]
async fn real_step_readers_reject_guessed_foreign_and_prior_context_access() {
    let f = Fixture::new().await;
    // Initialize the independent reader store before starting the live writer;
    // opening/migrating another store during a ballot can contend on SQLite DDL.
    std::fs::create_dir_all(&f.config.data_dir).unwrap();
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(SlowBallots)
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $survey Which checks help?"))
        .await;
    f.wait(|| {
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE kind='survey.vote' AND status='running'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            == 6
    })
    .await;
    let reader =
        squad_store::readers::ScopedReaders::new(store, ExecutorKey::Member(nick("member0")), 1);
    let wid = |kind: &str, target: &str| -> WorkId {
        f.db()
            .query_row(
                "SELECT work_id FROM work WHERE kind=? AND target=?",
                [kind, target],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
            .parse()
            .unwrap()
    };
    assert!(matches!(
        reader.get(wid("survey.vote", "member1")).await,
        Err(ReaderError::Forbidden)
    ));
    assert!(matches!(reader.get(WorkId::new()).await, Ok(None)));
    assert!(matches!(
        reader.for_work(wid("reply", "member0")).await,
        Err(ReaderError::NotRunnable)
    ));
    let own = wid("survey.vote", "member0");
    let history = reader.for_work(own).await.unwrap();
    assert_eq!(history.len(), 1);
    assert!(matches!(history[0].meta, HistoryMetadata::Anonymous { .. }));
    assert_eq!(history[0].name.as_deref(), Some("squad"));
    let values = reader.prompt_values_for_work(own).await.unwrap();
    let data: serde_json::Value = serde_json::from_str(&values["mode_data_json"]).unwrap();
    assert_eq!(data.as_object().unwrap().len(), 2);
    assert!(!values["mode_data_json"].contains("PRIVATE_WHOLE_ANSWER"));
    f.wait(|| f.count("survey.report") == 1).await;
    assert!(reader.for_work(own).await.is_err());
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct EmptyClaims;
impl Respond for EmptyClaims {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        if String::from_utf8_lossy(&r.body).contains("extract atomic claims") {
            structured_judge(serde_json::json!({"claims":[]}))
        } else {
            Answers.respond(r)
        }
    }
}
#[tokio::test]
async fn empty_claims_skip_ballots_and_release_unused_reservations() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(EmptyClaims)
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $survey Which checks help?"))
        .await;
    f.wait(|| f.count("survey.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 8);
    assert_eq!(f.count("survey.vote"), 0);
    assert_eq!(f.count("survey.revote"), 0);
    f.assert_ledger();
    assert_eq!(room.state().await.daily_spend.reserved_usd, 0.0);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn survey_preflight_survives_restart_and_confirmation_dispatches_once() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.0;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@all $survey Which checks help?"))
        .await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Contest(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(plan.estimate.model_calls, 20);
    assert_eq!(plan.settings["reservedAttempts"], 42);
    assert!(f.fake.captured_bodies().await.is_empty());
    assert_eq!(room.state().await.pending_preflights.len(), 1);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.state().await.pending_preflights.len(), 1);
    let confirm = squad_protocol::Request::ContestConfirm {
        request_id: EventId::new(),
        contest_id: plan.contest_id,
        plan_id: plan.plan_id,
    };
    assert!(matches!(
        room.submit(confirm.clone()).await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| f.count("survey.report") == 1).await;
    room.submit(confirm).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 20);
    assert!(room.state().await.pending_preflights.is_empty());
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn parser_authorized_single_member_survey_uses_adjusted_counts() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@member0 $survey Which checks help?"))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("survey.report") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 5);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct PausedStage(&'static str);
impl Respond for PausedStage {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let response = Answers.respond(r);
        if String::from_utf8_lossy(&r.body).contains(self.0) {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}
#[tokio::test]
async fn stop_fences_each_typed_survey_kind_and_clear_is_foreign_key_safe() {
    for (kind, stage) in [
        ("survey.extract", "extract atomic claims"),
        ("survey.vote", "independent claim vote"),
        ("survey.revote", "reconsider selected claims"),
        ("survey.report", "write the human's claim map"),
    ] {
        let f = Fixture::new().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(PausedStage(stage))
            .with_priority(1)
            .mount(&f.fake.server)
            .await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, "@all $survey Which checks help?"))
            .await;
        f.wait(|| {
            f.db()
                .query_row(
                    "SELECT COUNT(*) FROM work WHERE kind=? AND status='running'",
                    [kind],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0
        })
        .await;
        let (wid, target, case): (String, String, Option<String>) = f
            .db()
            .query_row(
                "SELECT work_id,target,case_id FROM work WHERE kind=? AND status='running' LIMIT 1",
                [kind],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let wid: WorkId = wid.parse().unwrap();
        let owner = if let Some(case) = case {
            ExecutorKey::Case(case.parse().unwrap())
        } else {
            ExecutorKey::Member(target.parse().unwrap())
        };
        let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
        let own = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
        let foreign = squad_store::readers::ScopedReaders::new(
            store,
            if matches!(owner, ExecutorKey::Case(_)) {
                ExecutorKey::Case(CaseId::new())
            } else {
                ExecutorKey::Member(nick("intruder"))
            },
            1,
        );
        assert!(matches!(
            foreign.get(wid).await,
            Err(ReaderError::Forbidden)
        ));
        assert!(foreign.for_work(wid).await.is_err());
        assert!(foreign.prompt_values_for_work(wid).await.is_err());
        let history = own.for_work(wid).await.unwrap();
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].meta, HistoryMetadata::Anonymous { .. }));
        assert!(matches!(
            room.submit(squad_protocol::Request::Stop {
                request_id: EventId::new(),
                target: None
            })
            .await,
            RequestResult::Accepted { .. }
        ));
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
        assert_eq!(f.count("survey.report"), 0);
        f.assert_ledger();
        assert!(matches!(
            room.submit(squad_protocol::Request::Clear {
                request_id: EventId::new()
            })
            .await,
            RequestResult::Accepted { .. }
        ));
        assert_eq!(
            f.db()
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        assert!(matches!(own.get(wid).await, Err(ReaderError::StaleWork)));
        room.shutdown().await.unwrap();
    }
}
#[derive(Clone)]
struct PausedMember;
impl Respond for PausedMember {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let b: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let response = Answers.respond(r);
        if b["model"] == "fixture/member5"
            && b["messages"].to_string().contains("independent claim vote")
        {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}
#[tokio::test]
async fn recovery_mid_ballot_keeps_accepted_ballots_and_selected_sources() {
    let f = Fixture::new().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(PausedMember)
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@all $survey Which checks help?"))
        .await;
    f.wait(|| f.count("survey.vote") == 5).await;
    let claim: String = f
        .db()
        .query_row("SELECT claim_id FROM survey_claims", [], |r| r.get(0))
        .unwrap();
    let source: String = f
        .db()
        .query_row("SELECT setup_json FROM survey_sources", [], |r| r.get(0))
        .unwrap();
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.count("survey.report") == 1).await;
    assert_eq!(
        f.db()
            .query_row("SELECT claim_id FROM survey_claims", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        claim
    );
    assert_eq!(
        f.db()
            .query_row("SELECT setup_json FROM survey_sources", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        source
    );
    for i in 0..5 {
        let model = format!("fixture/member{i}");
        let bodies = f.fake.captured_bodies().await;
        assert_eq!(
            bodies
                .iter()
                .filter(|b| b["model"] == model
                    && b["messages"].to_string().contains("independent claim vote"))
                .count(),
            1
        );
    }
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn expired_preflight_releases_the_active_claim_on_restart_without_dispatch() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(&f.config, "@all $survey Which checks help?"))
        .await;
    assert!(matches!(result, RequestResult::PendingConfirmation { .. }));
    room.shutdown().await.unwrap();
    f.db().execute("UPDATE contests SET expires_at='2000-01-01T00:00:00Z',plan_json=json_set(plan_json,'$.expiresAt','2000-01-01T00:00:00Z')",[]).unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert!(room.state().await.active_contest.is_none());
    assert!(room.state().await.pending_preflights.is_empty());
    assert!(f.fake.captured_bodies().await.is_empty());
    f.assert_ledger();
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn recovery_at_artifact_barrier_assignment_and_report_commit_boundaries() {
    let cases = [
        (
            "BEFORE INSERT ON events WHEN NEW.kind='contest.report' AND json_extract(NEW.payload,'$.data.surveyStage')='COLLECTING'",
            "COLLECTING",
            6,
            0,
        ),
        (
            "BEFORE INSERT ON work WHEN NEW.kind='survey.vote'",
            "VOTING",
            6,
            0,
        ),
        (
            "BEFORE UPDATE OF status ON work WHEN NEW.kind='survey.vote' AND NEW.status='running'",
            "VOTING",
            6,
            6,
        ),
        (
            "BEFORE INSERT ON events WHEN NEW.kind='survey.state' AND json_extract(NEW.payload,'$.state')='COMPLETE'",
            "REPORTING",
            6,
            6,
        ),
    ];
    for (point, stage, answers, vote_work) in cases {
        let f = Fixture::new().await;
        std::fs::create_dir_all(&f.config.data_dir).unwrap();
        let mut db = rusqlite::Connection::open(f.config.data_dir.join("squad.db")).unwrap();
        squad_store::schema::migrate(&mut db).unwrap();
        db.execute_batch(&format!("CREATE TRIGGER survey_crash {point} BEGIN SELECT RAISE(ABORT,'fixture interruption'); END;")).unwrap();
        drop(db);
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, "@all $survey Which checks help?"))
            .await;
        f.wait(|| {
            let db = f.db();
            let state: Option<String> = db
                .query_row(
                    "SELECT json_extract(state_json,'$.state') FROM contests",
                    [],
                    |r| r.get(0),
                )
                .ok();
            let work: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM work WHERE kind='survey.vote'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let pending: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM work WHERE status='running'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let report = if stage == "REPORTING" {
                f.count("survey.report") == 1
            } else {
                true
            };
            state.as_deref() == Some(stage)
                && f.count("message.final") == answers
                && work == vote_work
                && pending == 0
                && report
        })
        .await;
        let db = f.db();
        let mut q = db
            .prepare("SELECT work_id FROM work ORDER BY rowid")
            .unwrap();
        let work = q
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        drop(q);
        drop(db);
        let sources: String = f
            .db()
            .query_row("SELECT setup_json FROM survey_sources", [], |r| r.get(0))
            .unwrap();
        room.shutdown().await.unwrap();
        f.db().execute_batch("DROP TRIGGER survey_crash").unwrap();
        let room = f.open(OpenMode::Interactive).await;
        f.wait(|| {
            f.count("survey.report") == 1
                && f.db()
                    .query_row("SELECT active FROM contests", [], |r| r.get::<_, i64>(0))
                    .unwrap()
                    == 0
        })
        .await;
        assert_eq!(f.fake.captured_bodies().await.len(), 20, "boundary {point}");
        let after: SurveySetup = serde_json::from_str(
            &f.db()
                .query_row("SELECT setup_json FROM survey_sources", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
        )
        .unwrap();
        let before: SurveySetup = serde_json::from_str(&sources).unwrap();
        assert_eq!(after.question, before.question);
        assert_eq!(after.question_message_id, before.question_message_id);
        assert_eq!(after.participants, before.participants);
        if !before.answers.is_empty() {
            assert_eq!(after.answers, before.answers);
        } else {
            let db = f.db();
            let mut q = db
                .prepare("SELECT message_id FROM work WHERE kind='reply' ORDER BY rowid")
                .unwrap();
            let expected = q
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(
                after
                    .answers
                    .iter()
                    .map(|a| a.message_id.to_string())
                    .collect::<Vec<_>>(),
                expected
            );
        }
        for id in &work {
            assert_eq!(
                f.db()
                    .query_row("SELECT COUNT(*) FROM work WHERE work_id=?", [id], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
        assert_eq!(f.count("survey.claims"), 1);
        assert_eq!(f.count("survey.selection"), 1);
        f.assert_ledger();
        room.shutdown().await.unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum RevoteShape {
    SloppyValid,
    MissingEvidence,
    WordBudget,
    Duplicate,
    ForeignClaim,
    LowercaseVote,
    ExtraField,
}
impl Respond for RevoteShape {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        use serde_json::json;
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["messages"]
            .to_string()
            .contains("extract atomic claims")
        {
            let data = task_data(&body);
            return structured_judge(json!({"claims":[
                {"id":"c1","claim":"Testing catches regressions.","sourceAnswerIds":[data["answers"][0]["messageId"]]},
                {"id":"c2","claim":"Independent checks improve reliability.","sourceAnswerIds":[data["answers"][0]["messageId"]]}
            ]}));
        }
        if !body["messages"]
            .to_string()
            .contains("reconsider selected claims")
        {
            return Answers.respond(r);
        }
        let data = task_data(&body);
        let mut value = json!({"votes":data["claims"].as_array().unwrap().iter().map(|c|json!({
            "confidence":4, "changeEvidence":" A controlled counterexample ",
            "reason":" Tests detect regressions. ", "vote":"AGREE", "claimId":c["claimId"]
        })).collect::<Vec<_>>()});
        match self {
            Self::SloppyValid => {}
            Self::MissingEvidence => {
                value["votes"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("changeEvidence");
            }
            Self::WordBudget => {
                for vote in value["votes"].as_array_mut().unwrap() {
                    vote["reason"] = json!("Tests detect regressions with meaningful fixtures.");
                    vote["changeEvidence"] =
                        json!("A controlled counterexample would change this assessment.");
                }
            }
            Self::Duplicate => {
                let duplicate = value["votes"][0].clone();
                value["votes"].as_array_mut().unwrap().push(duplicate);
            }
            Self::ForeignClaim => {
                value["votes"][0]["claimId"] = json!("foreign-claim");
            }
            Self::LowercaseVote => {
                value["votes"][0]["vote"] = json!("agree");
            }
            Self::ExtraField => {
                value["votes"][0]["count"] = json!(6);
            }
        }
        // Ordinary formatting and field ordering are harmless. This still travels
        // through the real SSE client, schema decoder, router and SQLite writer.
        streaming(
            &format!(" \n{}\n ", serde_json::to_string_pretty(&value).unwrap()),
            "stop",
        )
    }
}
#[tokio::test]
async fn revote_wire_shapes_preserve_valid_whitespace_and_flag_invalid_cycles() {
    for shape in [
        RevoteShape::SloppyValid,
        RevoteShape::MissingEvidence,
        RevoteShape::WordBudget,
        RevoteShape::Duplicate,
        RevoteShape::ForeignClaim,
        RevoteShape::LowercaseVote,
        RevoteShape::ExtraField,
    ] {
        let mut f = Fixture::new().await;
        f.config.prompts.limits.room_words = 20;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(shape)
            .with_priority(1)
            .mount(&f.fake.server)
            .await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(request(&f.config, "@all $survey Which checks help?"))
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
        let events = room.events_after(None, 500).await.unwrap();
        let report = events
            .iter()
            .find_map(|e| match &e.content {
                EnvelopePayload::SurveyReport(r) => Some(r),
                _ => None,
            })
            .unwrap();
        let state = events
            .iter()
            .rev()
            .find_map(|e| match &e.content {
                EnvelopePayload::SurveyState(s) => Some(s),
                _ => None,
            })
            .unwrap();
        let valid = matches!(shape, RevoteShape::SloppyValid);
        let participation = report.completeness.as_ref().unwrap();
        assert_eq!(participation.complete, valid, "{shape:?}");
        assert_eq!(participation.is_complete(), valid);
        assert_eq!(state.state, SurveyStatus::Complete);
        assert_eq!(state.completeness.as_ref(), Some(participation));
        assert!(participation.missing_answers.is_empty());
        assert!(participation.missing_votes.is_empty());
        assert_eq!(report.map[0].final_tally.agree, 6);
        assert!(report.map[0].switches.is_empty());
        assert_eq!(f.count("survey.revote"), if valid { 6 } else { 0 });
        assert_eq!(
            f.fake.captured_bodies().await.len(),
            if valid { 20 } else { 26 }
        );
        if valid {
            assert!(report.map[0].not_revoted.is_empty());
            assert!(state.terminal_reason.is_none());
        } else {
            assert_eq!(participation.missing_revotes.len(), 6);
            assert_eq!(
                report.map[0].not_revoted,
                f.config
                    .agents
                    .iter()
                    .map(|a| a.nick.clone())
                    .collect::<Vec<_>>()
            );
            let reason = state.terminal_reason.as_ref().unwrap().as_str();
            assert!(reason.contains("DEGRADED"));
            // CLI text uses this exact core renderer; JSON carries completeness.
            for p in [
                EnvelopePayload::SurveyReport(report.clone()),
                EnvelopePayload::SurveyState(state.clone()),
            ] {
                let text = squad_core::export::survey::render(&p).unwrap();
                assert!(text.contains("DEGRADED") && text.contains("member5"));
                assert_eq!(
                    serde_json::to_value(p).unwrap()["payload"]["completeness"]["complete"],
                    false
                );
            }
            let expected_rule = match shape {
                RevoteShape::MissingEvidence => "invalid survey change evidence",
                RevoteShape::WordBudget => "prose word budget exceeded: 26 > 20",
                RevoteShape::Duplicate | RevoteShape::ForeignClaim => {
                    "survey ballot coverage mismatch"
                }
                RevoteShape::LowercaseVote => "/votes/*/vote: enum",
                RevoteShape::ExtraField => "/votes/*: additionalProperties",
                RevoteShape::SloppyValid => unreachable!(),
            };
            let errors = events
                .iter()
                .filter_map(|e| match &e.content {
                    EnvelopePayload::WorkState(w) if w.status == WorkStatus::Failed => {
                        w.error.as_ref()
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(!errors.is_empty());
            assert!(
                errors
                    .iter()
                    .all(|e| e.text.as_str().contains(expected_rule)),
                "{shape:?}: {errors:?}"
            );
        }
        let stored = squad_store::survey::state(&f.db(), state.contest_id).unwrap();
        assert_eq!(stored.completeness, state.completeness);
        f.assert_ledger();
        room.shutdown().await.unwrap();
        let reopened = f.open(OpenMode::OneShot).await;
        assert_eq!(
            squad_store::survey::state(&f.db(), state.contest_id).unwrap(),
            stored
        );
        assert_eq!(f.count("survey.report"), 1);
        reopened.shutdown().await.unwrap();
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
