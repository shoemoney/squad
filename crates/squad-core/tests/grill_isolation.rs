//! M5.P6.L6.T6 (real-store shared-floor isolation, deterministic redaction,
//! preserved human transcript) and M5.P6.L4.T1/T3 (grill restarts after a
//! pass, mid-rebuild and after a plan commit).
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
#[derive(Clone, Copy)]
enum Scenario {
    SlowRebuild,
    SlowAnswer,
    Happy,
    SlowQuestion,
}
#[derive(Clone)]
struct Answers {
    scenario: Scenario,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl Respond for Answers {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        let user = body["messages"].as_array().unwrap().last().unwrap()["content"]
            .as_str()
            .unwrap();
        if user.contains("Return exactly PASS") {
            let count = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return match self.scenario {
                Scenario::SlowQuestion => {
                    streaming("PASS", "stop").set_delay(Duration::from_secs(2))
                }
                _ => streaming(
                    &serde_json::json!({"questions":[format!("How does mechanism {count} work?")]})
                        .to_string(),
                    "stop",
                ),
            };
        }
        if user.contains("Return a JSON object with only answers") {
            let answer = streaming(
                r#"{"answers":[{"questionIndex":0,"answer":"Use a bounded queue.","planChanges":"Add backpressure."}]}"#,
                "stop",
            );
            return if matches!(self.scenario, Scenario::SlowAnswer) {
                answer.set_delay(Duration::from_secs(1))
            } else {
                answer
            };
        }
        if user.contains("Return only the complete standalone revised plan") {
            let plan = streaming(
                "A complete revised plan with bounded queues and backpressure.",
                "stop",
            );
            return if matches!(self.scenario, Scenario::SlowRebuild) {
                plan.set_delay(Duration::from_secs(2))
            } else {
                plan
            };
        }
        streaming("Initial plan with one writer and a bounded queue.", "stop")
    }
}
async fn send(f: &Fixture, room: &RoomHandle, text: &str) -> RequestResult {
    send_meta(f, room, text, parser::SubmissionMeta::default()).await
}
async fn send_meta(
    f: &Fixture,
    room: &RoomHandle,
    text: &str,
    meta: parser::SubmissionMeta,
) -> RequestResult {
    let state = room.state().await;
    let events = room.page_events(None, 500).await.unwrap();
    let mut ctx = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        active_contest: state.active_contest.clone(),
        ..Default::default()
    };
    for e in events {
        let (message_id, body, reply) = match &e.content {
            EnvelopePayload::UserMessage(m) => (Some(m.input.message_id), m.body.clone(), None),
            EnvelopePayload::MessageFinal(m) => (
                Some(m.completed.generation.message_id),
                m.content.clone(),
                Some(m.in_reply_to_message_id),
            ),
            _ => (None, String::new(), None),
        };
        ctx.records.push(parser::RecordSummary {
            event_id: e.base.id,
            seq: e.commit.seq,
            message_id,
            author: e.base.sender,
            recipients: e.base.recipients,
            kind: Text::new(if reply.is_some() {
                "message.final"
            } else {
                "user.message"
            })
            .unwrap(),
            content: body,
            in_reply_to_message_id: reply,
            eligible: true,
            superseded: false,
            cell_ids: vec![],
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            state_version: None,
        });
    }

    if let Some(ContestState::Selection(s)) = &ctx.active_contest
        && s.state == ContestSelectionStatus::AwaitingPick
    {
        ctx.pending_pick = Some(parser::PendingPick {
            contest_id: s.contest_id,
            mode: parser::QuestionFirstMode::Grill,
            eligible: s.answers.iter().map(|a| a.nick.clone()).collect(),
        });
    }
    let parsed = parse(text, &meta, &ctx).unwrap();
    room.submit(
        Request::from_parsed_with_context(EventId::new(), parsed, &meta, &ctx, false).unwrap(),
    )
    .await
}
fn terminal(f: &Fixture) -> bool {
    f.db()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM contests WHERE kind='grill' AND active=0)",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
async fn seed(f: &Fixture, room: &RoomHandle) {
    assert!(matches!(
        send(f, room, "@member0 Design a plan").await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| f.count("message.final") == 1).await;
}

fn contest_id(f: &Fixture) -> ContestId {
    f.db()
        .query_row(
            "SELECT contest_id FROM contests WHERE kind='grill'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .parse()
        .unwrap()
}
fn calls(f: &Fixture, kind: &str) -> i64 {
    f.db()
        .query_row(
            "SELECT COUNT(*) FROM attempts a JOIN work w ON w.work_id=a.work_id WHERE w.kind=?",
            [kind],
            |r| r.get(0),
        )
        .unwrap()
}
fn state(f: &Fixture) -> String {
    f.db()
        .query_row(
            "SELECT json_extract(state_json,'$.state') FROM contests WHERE kind='grill'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
/// Exactly-once shape of a finished two-round, two-griller floor.
fn assert_complete_once(f: &Fixture) {
    assert_eq!(state(f), "COMPLETE");
    assert_eq!(f.count("grill.question"), 4);
    assert_eq!(f.count("grill.answer"), 4);
    assert_eq!(f.count("grill.plan"), 3);
    assert_eq!(f.count("grill.complete"), 1);
    // No accepted artifact was committed twice.
    let duplicates: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM (SELECT context_id,origin_event_id,projection_key FROM history GROUP BY 1,2,3 HAVING COUNT(*)>1)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(duplicates, 0);
    let held: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM reservations WHERE attempt IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(held, 0);
    f.assert_ledger();
}

#[tokio::test]
async fn shared_floor_reaches_exactly_the_participants_and_keeps_the_human_transcript_verbatim() {
    let f = Fixture::scenario(4, Scenario::Happy).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(
        &f,
        &room,
        "@member0 @member1 @member2 $grill $GRILL_ROUNDS=2",
    )
    .await;
    f.wait(|| terminal(&f)).await;
    let db = f.db();
    let floor = |ctx: &str| -> Vec<(String, String)> {
        let mut q = db
            .prepare("SELECT projection_key, content FROM history WHERE context_id=? AND projection_key LIKE 'grill.%' ORDER BY pos")
            .unwrap();
        q.query_map([ctx], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    // The outsider receives nothing, participants receive the floor.
    assert!(floor("member3").is_empty());
    for participant in ["member0", "member1", "member2"] {
        assert!(!floor(participant).is_empty(), "{participant} saw no floor");
    }
    // Every event on the floor is addressed to exactly the participants.
    let events = room.page_events(None, 500).await.unwrap();
    let mut transcript = String::new();
    for e in &events {
        if let EnvelopePayload::GrillQuestion(_)
        | EnvelopePayload::GrillAnswer(_)
        | EnvelopePayload::GrillPlan(_) = &e.content
        {
            assert!(
                !e.base
                    .recipients
                    .contains(&Recipient::Member(nick("member3")))
            );
            transcript.push_str(&serde_json::to_string(&e.content).unwrap());
        }
    }
    // Human transcript originals: model text is stored verbatim, never redacted.
    assert!(transcript.contains("How does mechanism"));
    assert!(transcript.contains("Use a bounded queue."));
    assert!(transcript.contains("A complete revised plan with bounded queues and backpressure."));
    assert!(!transcript.contains("[redacted]"));
    let export = room
        .export(ExportScope::Grill, Some(contest_id(&f)))
        .await
        .unwrap();
    assert!(export.contains("bounded queues") && !export.contains("[redacted]"));
    // Working contexts carry only the current round's exchange, not the full
    // earlier-round answers.
    let mut q = db
        .prepare("SELECT content FROM history WHERE projection_key='grill.working' ORDER BY pos")
        .unwrap();
    let working: Vec<String> = q
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!working.is_empty());
    let last = working.last().unwrap();
    assert!(last.contains("\"round\":2"), "{last}");
    assert!(
        !last.contains("\"round\":1"),
        "earlier-round exchange in working context: {last}"
    );
    room.shutdown().await.unwrap();
}

#[test]
fn blind_redaction_is_deterministic_and_leaves_unrelated_text_alone() {
    use squad_core::orchestrator::blind::{redact, value};
    let tokens = vec![
        "member0".to_string(),
        "fixture/member0".to_string(),
        "PERSONA_0".to_string(),
    ];
    let text = "MEMBER0 said fixture/member0 follows PERSONA_0, not member00 or amember0.";
    let once = redact(text, &tokens);
    assert_eq!(once, redact(text, &tokens));
    assert_eq!(
        once,
        "[redacted] said [redacted] follows [redacted], not member00 or amember0."
    );
    // Only declared prose keys are redacted; identifiers elsewhere are untouched.
    let json = value(
        serde_json::json!({"claim":"member0 is wrong","id":"member0","nested":[{"reason":"PERSONA_0"}]}),
        &tokens,
    );
    assert_eq!(json["claim"], "[redacted] is wrong");
    assert_eq!(json["id"], "member0");
    assert_eq!(json["nested"][0]["reason"], "[redacted]");
    assert_eq!(redact("nothing to hide", &[]), "nothing to hide");
}

#[tokio::test]
async fn restart_after_a_pass_does_not_replay_the_pass_or_the_questions() {
    let mut f = Fixture::scenario(3, Scenario::SlowQuestion).await;
    f.config.budget.daily_max_usd = 20.;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill $GRILL_ROUNDS=2").await;
    f.wait(|| f.count("grill.pass") == 1).await;
    room.shutdown().await.unwrap();
    drop(room);
    let passes = f.count("grill.pass");
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| terminal(&f)).await;
    // The committed pass stayed committed exactly once; the contest then ended
    // by its own rules without duplicating any floor artifact.
    assert!(f.count("grill.pass") >= passes);
    assert_eq!(f.count("grill.complete"), 1);
    let duplicate_floor: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM (SELECT context_id,origin_event_id,projection_key FROM history GROUP BY 1,2,3 HAVING COUNT(*)>1)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(duplicate_floor, 0);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_mid_rebuild_commits_one_plan_and_finishes() {
    let f = Fixture::scenario(3, Scenario::SlowRebuild).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill $GRILL_ROUNDS=2").await;
    // The rebuild is in flight: the provider call started, no new plan yet.
    f.wait(|| calls(&f, "grill.rebuild") >= 1).await;
    assert_eq!(f.count("grill.plan"), 1);
    room.shutdown().await.unwrap();
    drop(room);
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| terminal(&f)).await;
    assert_complete_once(&f);
    // The interrupted attempt was retried, not double-committed.
    assert!(calls(&f, "grill.rebuild") >= 3);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_after_a_plan_commit_continues_with_the_next_round() {
    let f = Fixture::scenario(3, Scenario::SlowAnswer).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill $GRILL_ROUNDS=2").await;
    f.wait(|| f.count("grill.plan") == 2).await;
    room.shutdown().await.unwrap();
    drop(room);
    let committed = f.count("grill.plan");
    assert!(committed >= 2);
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| terminal(&f)).await;
    assert_complete_once(&f);
    room.shutdown().await.unwrap();
}
