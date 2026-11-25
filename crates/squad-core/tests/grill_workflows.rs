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
        Self::scenario(3, Scenario::Happy).await
    }
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
    SlowAnswer,
    Happy,
    Pass,
    InvalidQuestion,
    LeaderFailure,
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
                Scenario::Pass => streaming("PASS", "stop"),
                Scenario::InvalidQuestion => streaming(" PASS ", "stop"),
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
            let answer = if matches!(self.scenario, Scenario::LeaderFailure) {
                streaming("{}", "stop")
            } else {
                streaming(
                    r#"{"answers":[{"questionIndex":0,"answer":"Use a bounded queue.","planChanges":"Add backpressure."}]}"#,
                    "stop",
                )
            };
            return if matches!(self.scenario, Scenario::SlowAnswer) {
                answer.set_delay(Duration::from_secs(1))
            } else {
                answer
            };
        }
        if user.contains("Return only the complete standalone revised plan") {
            return streaming(
                "A complete revised plan with bounded queues and backpressure.",
                "stop",
            );
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
#[tokio::test]
async fn sequential_floor_and_export() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    assert!(matches!(
        send(&f, &room, "@member0 $grill $GRILL_ROUNDS=2").await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| terminal(&f)).await;
    assert_eq!(f.count("grill.question"), 4);
    assert_eq!(f.count("grill.answer"), 4);
    assert_eq!(f.count("grill.plan"), 3);
    assert_eq!(f.count("grill.complete"), 1);
    let state: String = f
        .db()
        .query_row(
            "SELECT json_extract(state_json,'$.state') FROM contests WHERE kind='grill'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "COMPLETE");
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

fn status(f: &Fixture) -> String {
    f.db()
        .query_row(
            "SELECT json_extract(state_json,'$.state') FROM contests WHERE kind='grill'",
            [],
            |r| r.get(0),
        )
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
fn no_reservation(f: &Fixture) {
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
async fn voluntary_pass_finishes_early() {
    let f = Fixture::scenario(3, Scenario::Pass).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill").await;
    f.wait(|| terminal(&f)).await;
    assert_eq!(f.count("grill.pass"), 2);
    assert_eq!(f.count("grill.answer"), 0);
    assert_eq!(f.count("grill.plan"), 1);
    assert_eq!(calls(&f, "grill.question"), 2);
    assert_eq!(
        f.db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "survived"
    );
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn invalid_pass_is_retried_and_skipped_without_survival() {
    let f = Fixture::scenario(3, Scenario::InvalidQuestion).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill").await;
    f.wait(|| terminal(&f)).await;
    assert_eq!(calls(&f, "grill.question"), 8);
    assert_eq!(f.count("grill.pass"), 4);
    assert_eq!(f.count("grill.plan"), 1);
    assert_eq!(status(&f), "COMPLETE");
    let voluntary:i64=f.db().query_row("SELECT COUNT(*) FROM events WHERE kind='grill.pass' AND json_extract(payload,'$.reason')='pass'",[],|r|r.get(0)).unwrap();
    assert_eq!(voluntary, 0);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn leader_exhaustion_preserves_last_plan() {
    let f = Fixture::scenario(3, Scenario::LeaderFailure).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill").await;
    f.wait(|| terminal(&f)).await;
    assert_eq!(status(&f), "FAILED");
    assert_eq!(calls(&f, "grill.answer"), 2);
    assert_eq!(f.count("grill.plan"), 1);
    assert_eq!(f.count("grill.answer"), 0);
    let export = room
        .export(ExportScope::Grill, Some(contest(&f)))
        .await
        .unwrap();
    assert!(export.contains("Initial plan"));
    assert!(export.contains("How does mechanism"));
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn confirmation_cancel_and_restart_without_duplicate_calls() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    let result = send(&f, &room, "@member0 $grill").await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Contest(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    assert_eq!(calls(&f, "grill.question"), 0);
    assert_eq!(room.state().await.pending_preflights.len(), 1);
    room.shutdown().await.unwrap();
    drop(room);
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(calls(&f, "grill.question"), 0);
    assert!(matches!(
        room.submit(Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id
        })
        .await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| terminal(&f)).await;
    let before = calls(&f, "grill.question");
    assert_eq!(before, 4);
    room.shutdown().await.unwrap();
    drop(room);
    let room = f.open(OpenMode::Interactive).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(calls(&f, "grill.question"), before);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn cancellation_and_expiry_do_not_call_provider() {
    for expire in [false, true] {
        let mut f = Fixture::new().await;
        f.config.budget.debate_confirm_usd = 0.;
        let room = f.open(OpenMode::Interactive).await;
        seed(&f, &room).await;
        let result = send(&f, &room, "@member0 $grill").await;
        let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } = result
        else {
            panic!("{result:?}")
        };
        if expire {
            let mut changed = plan.clone();
            changed.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);
            f.db()
                .execute(
                    "UPDATE contests SET plan_json=?,expires_at=?",
                    rusqlite::params![
                        serde_json::to_string(&changed).unwrap(),
                        changed.expires_at.to_rfc3339()
                    ],
                )
                .unwrap();
        }
        let req = if expire {
            Request::ContestConfirm {
                request_id: EventId::new(),
                contest_id: plan.contest_id,
                plan_id: plan.plan_id,
            }
        } else {
            Request::ContestCancel {
                request_id: EventId::new(),
                contest_id: plan.contest_id,
                plan_id: plan.plan_id,
            }
        };
        room.submit(req).await;
        assert_eq!(status(&f), "STOPPED");
        assert_eq!(calls(&f, "grill.question"), 0);
        no_reservation(&f);
        room.shutdown().await.unwrap();
    }
}
#[tokio::test]
async fn daily_refusal_before_any_grill_attempt() {
    let mut f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    room.shutdown().await.unwrap();
    // The ordinary seed call must itself fit its priced reservation. Apply the
    // small cap after collecting the plan, before admitting the grill.
    f.config.budget.daily_max_usd = 0.05;
    let room = f.open(OpenMode::Interactive).await;
    let result = send(&f, &room, "@member0 $grill").await;
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
    assert_eq!(calls(&f, "grill.question"), 0);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn question_first_pick_reuses_ordinary_answers() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = send(&f, &room, "@all $grill Design a plan").await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("contest.pick_requested") == 1).await;
    assert_eq!(calls(&f, "reply"), 3);
    let result = send(&f, &room, "@member1").await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| terminal(&f)).await;
    assert_eq!(status(&f), "COMPLETE");
    assert_eq!(calls(&f, "reply"), 3);
    assert_eq!(f.count("contest.picked"), 1);
    assert_eq!(f.count("grill.question"), 4);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn stop_fences_inflight_floor_and_releases_hold() {
    let f = Fixture::scenario(3, Scenario::SlowQuestion).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill").await;
    f.wait(|| calls(&f, "grill.question") == 1).await;
    let later = send(&f, &room, "@member2 Later ordinary request").await;
    assert!(matches!(later, RequestResult::Accepted { .. }), "{later:?}");
    let held: i64 = f
        .db()
        .query_row(
            "SELECT COUNT(*) FROM work WHERE held_by_contest_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(held, 1);
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
    assert_eq!(status(&f), "STOPPED");
    f.wait(|| f.count("message.final") == 2).await;
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(f.count("grill.pass"), 0);
    assert_eq!(f.count("grill.question"), 0);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
fn contest(f: &Fixture) -> ContestId {
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
#[tokio::test]
async fn restart_preserves_accepted_turns_and_frozen_prompts() {
    let mut f = Fixture::scenario(3, Scenario::SlowAnswer).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill $GRILL_ROUNDS=1").await;
    f.wait(|| f.count("grill.question") == 1 && calls(&f, "grill.answer") == 1)
        .await;
    room.shutdown().await.unwrap();
    drop(room);
    f.config.agents[0].persona = "CHANGED_PERSONA_AFTER_ADMISSION".into();
    let overrides = f._dir.path().join("overrides");
    std::fs::create_dir(&overrides).unwrap();
    std::fs::write(
        overrides.join("grill.answer.md"),
        format!(
            "CHANGED_TEMPLATE_AFTER_ADMISSION\n{}",
            include_str!("../../../prompts/grill.answer.md")
        ),
    )
    .unwrap();
    f.config.prompts.directory = Some(overrides);
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| terminal(&f)).await;
    assert_eq!(f.count("grill.question"), 2);
    assert_eq!(calls(&f, "grill.question"), 2);
    assert_eq!(f.count("grill.answer"), 2);
    assert_eq!(calls(&f, "grill.answer"), 3);
    for r in f
        .fake
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path().ends_with("completions"))
    {
        let body = String::from_utf8(r.body).unwrap();
        if body.contains("Return a JSON object with only answers") {
            assert!(!body.contains("CHANGED_PERSONA_AFTER_ADMISSION"));
            assert!(!body.contains("CHANGED_TEMPLATE_AFTER_ADMISSION"));
        }
    }
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn real_floor_is_limited_to_selected_participants() {
    let f = Fixture::scenario(4, Scenario::Happy).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 @member1 $grill").await;
    f.wait(|| terminal(&f)).await;
    let outsiders:i64=f.db().query_row("SELECT COUNT(*) FROM history WHERE context_id IN ('member2','member3') AND projection_key LIKE 'grill.%'",[],|r|r.get(0)).unwrap();
    assert_eq!(outsiders, 0);
    let events = room.page_events(None, 500).await.unwrap();
    for e in events {
        if matches!(
            e.content,
            EnvelopePayload::GrillQuestion(_)
                | EnvelopePayload::GrillAnswer(_)
                | EnvelopePayload::GrillPlan(_)
        ) {
            assert_eq!(
                e.base.recipients,
                vec![
                    Recipient::Me,
                    Recipient::Member(nick("member0")),
                    Recipient::Member(nick("member1"))
                ]
            );
        }
    }
    let export = room
        .export(ExportScope::Grill, Some(contest(&f)))
        .await
        .unwrap();
    assert!(export.contains("Plan changes"));
    assert!(export.contains("bounded queues"));
    assert!(!export.contains("PERSONA"));
    let projections:i64=f.db().query_row("SELECT COUNT(*) FROM history WHERE context_id LIKE 'contest:%' AND projection_key='grill.working'",[],|r|r.get(0)).unwrap();
    assert!(projections > 0);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn quoted_plan_source_is_frozen_and_clear_removes_all_floor_records() {
    let f = Fixture::scenario(3, Scenario::Pass).await;
    let room = f.open(OpenMode::Interactive).await;
    let ctx = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        ..Default::default()
    };
    let err = parse("@member0 $grill", &Default::default(), &ctx).unwrap_err();
    assert_eq!(
        err.message,
        "@member0 has no plan to grill yet. Ask it something first."
    );
    seed(&f, &room).await;
    let source = room
        .page_events(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
        .unwrap();
    send(&f, &room, "@member0 A different plan").await;
    f.wait(|| f.count("message.final") == 2).await;
    let result = send_meta(
        &f,
        &room,
        "@member0 $grill Focus on cost",
        parser::SubmissionMeta {
            quote: vec![source.base.id],
            ..Default::default()
        },
    )
    .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| terminal(&f)).await;
    let raw: String = f
        .db()
        .query_row(
            "SELECT map_json FROM contest_private_maps WHERE scope_key='grill.settings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let frozen: GrillPreflight = serde_json::from_str(&raw).unwrap();
    let EnvelopePayload::MessageFinal(reply) = source.content else {
        panic!()
    };
    assert_eq!(
        frozen.setup.source_message_id,
        reply.completed.generation.message_id
    );
    assert_eq!(frozen.setup.focus, "Focus on cost");
    assert_eq!(f.count("verdict"), 0);
    assert_eq!(calls(&f, "judge"), 0);
    let trophies: i64 = f
        .db()
        .query_row(
            "SELECT COALESCE(SUM(wins+losses+trophies),0) FROM scores",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(trophies, 0);
    let result = room
        .submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    for table in [
        "grill_plans",
        "grill_turns",
        "contest_private_maps",
        "contests",
    ] {
        let count: i64 = f
            .db()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn generation_identity_bound_readers_and_durable_deadline_are_enforced() {
    let f = Fixture::scenario(3, Scenario::SlowQuestion).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill").await;
    f.wait(|| calls(&f, "grill.question") == 1).await;
    let db = f.db();
    let (raw,attempt,fence,epoch):(String,u32,i64,i64)=db.query_row("SELECT scheduling_meta,attempt,lease_fence,epoch FROM work WHERE kind='grill.question'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut work: Work = serde_json::from_value(value["work"].clone()).unwrap();
    work.attempt = attempt;
    let owner = ExecutorKey::Member(nick("member1"));
    let record = squad_store::work::record(&db, work.work_id)
        .unwrap()
        .unwrap();
    let deadline = record.deadline.unwrap();
    assert!(
        deadline > chrono::Utc::now()
            && deadline <= chrono::Utc::now() + chrono::Duration::seconds(180)
    );
    db.execute(
        "UPDATE work SET lease_until=? WHERE work_id=?",
        rusqlite::params![
            (deadline + chrono::Duration::seconds(10)).to_rfc3339(),
            work.work_id.to_string()
        ],
    )
    .unwrap();
    assert!(
        squad_store::work::check_fence(
            &db,
            work.work_id,
            &owner,
            attempt,
            fence as u64,
            epoch as u64,
            deadline + chrono::Duration::milliseconds(1)
        )
        .is_err()
    );
    let make = |message_id| {
        EnvelopePayload::ContestArtifact(ContestArtifact {
            contest_id: work.contest_id.unwrap(),
            step_id: work.step_id.unwrap(),
            artifact_kind: Text::new("grill.question").unwrap(),
            work_id: Some(work.work_id),
            source_refs: vec![],
            data: serde_json::json!({"generation":GenerationIdentity{message_id,work_id:work.work_id,trigger_event_id:work.trigger_event_id,attempt},"raw":"PASS","finishReason":"stop"}),
        })
    };
    assert!(squad_core::router::grill::canonical(&db, &work, &make(work.message_id)).is_ok());
    assert!(squad_core::router::grill::canonical(&db, &work, &make(MessageId::new())).is_err());
    let store = squad_store::Store::open(f.config.data_dir.join("squad.db")).unwrap();
    let foreign = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member(nick("member2")),
        epoch as u64,
    );
    assert_eq!(
        foreign.for_work(work.work_id).await.unwrap_err(),
        ReaderError::Forbidden
    );
    assert_eq!(
        foreign
            .prompt_values_for_work(work.work_id)
            .await
            .unwrap_err(),
        ReaderError::Forbidden
    );
    room.submit(Request::Stop {
        request_id: EventId::new(),
        target: None,
    })
    .await;
    assert!(squad_core::router::grill::canonical(&db, &work, &make(work.message_id)).is_err());
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn confirmation_rechecks_daily_cap_after_restart() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    let result = send(&f, &room, "@member0 $grill").await;
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Contest(plan),
        ..
    } = result
    else {
        panic!("{result:?}")
    };
    room.shutdown().await.unwrap();
    drop(room);
    f.config.budget.daily_max_usd = 0.05;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        })
        .await;
    assert!(
        matches!(&result,RequestResult::Rejected{error,..} if error.code==ErrorCode::DAILY_MAX_REACHED&&error.text.as_str().starts_with("Daily budget admission refused: spent $")&&error.text.as_str().contains(" of $0.050000 today; this request needs up to $3.631216 reserved.")),
        "{result:?}"
    );
    assert_eq!(status(&f), "FAILED");
    assert_eq!(calls(&f, "grill.question"), 0);
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn expired_attempts_retry_then_record_timeout_without_late_acceptance() {
    let f = Fixture::scenario(3, Scenario::SlowQuestion).await;
    let room = f.open(OpenMode::Interactive).await;
    seed(&f, &room).await;
    send(&f, &room, "@member0 $grill $GRILL_ROUNDS=1").await;
    for attempt in 1..=2 {
        f.wait(||f.db().query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='grill.question' AND target='member1' AND status='running' AND attempt=? AND provider_may_have_started=1)",[attempt],|r|r.get::<_,bool>(0)).unwrap()).await;
        f.db().execute("UPDATE work SET execution_deadline=? WHERE kind='grill.question' AND target='member1'",[(chrono::Utc::now()-chrono::Duration::milliseconds(1)).to_rfc3339()]).unwrap();
    }
    f.wait(|| terminal(&f)).await;
    assert_eq!(calls(&f, "grill.question"), 3);
    assert_eq!(f.count("grill.question"), 0);
    assert_eq!(f.count("grill.pass"), 2);
    let timed_out:i64=f.db().query_row("SELECT COUNT(*) FROM events WHERE kind='grill.pass' AND json_extract(payload,'$.reason')='timeout'",[],|r|r.get(0)).unwrap();
    assert_eq!(timed_out, 1);
    assert_eq!(status(&f), "COMPLETE");
    assert_eq!(
        f.db()
            .query_row(
                "SELECT json_extract(state_json,'$.terminalReason') FROM contests",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "grillers_exhausted"
    );
    no_reservation(&f);
    room.shutdown().await.unwrap();
}
