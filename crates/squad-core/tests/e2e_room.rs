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
        config.budget.daily_max_usd = 20.;
        config.budget.debate_confirm_usd = 10.;
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
        let messages = body["messages"].to_string();
        if body["model"] == "fixture/judge" {
            if messages.contains("Score round ") {
                tied_round_score("member0", "member1")
            } else {
                structured_judge(verdict())
            }
        } else {
            streaming("A supported proposal with a concrete tradeoff.", "stop")
        }
    }
}
fn verdict() -> serde_json::Value {
    let scores=["member0","member1"].map(|nick|serde_json::json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|id|serde_json::json!({"criterionId":id,"score":5,"reasoning":"Supported by the evidence"})))}));
    serde_json::json!({"winner":"member0","scores":scores,"reasoning":"Member0 wins the tie-breaker.","whatToStealFromLoser":[],"tallyOverrideReason":null})
}
#[tokio::test]
async fn room_six_private_workers_and_clear_restart() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    for i in 0..6 {
        assert!(matches!(
            room.submit(request(
                &f.config,
                &format!("@member{i} PRIVATE_{i}_CANARY")
            ))
            .await,
            RequestResult::Accepted { .. }
        ));
    }
    f.wait(|| f.count("message.final") == 6).await;
    room.submit(request(&f.config, "@all shared question"))
        .await;
    f.wait(|| f.count("message.final") == 12).await;
    for body in f.fake.captured_bodies().await {
        let system = body["messages"][0]["content"].as_str().unwrap();
        let own = (0..6)
            .find(|i| system.contains(&format!("PERSONA_{i}")))
            .unwrap();
        for other in 0..6 {
            if own != other {
                assert!(
                    !body
                        .to_string()
                        .contains(&format!("PRIVATE_{other}_CANARY"))
                );
            }
        }
    }
    let old: i64 = f
        .db()
        .query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))
        .unwrap();
    let cleared = room
        .submit(squad_protocol::Request::Clear {
            request_id: EventId::new(),
        })
        .await;
    assert!(
        matches!(cleared, RequestResult::Accepted { .. }),
        "{cleared:?}"
    );
    assert_eq!(
        f.db()
            .query_row("SELECT epoch FROM room_meta", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        old + 1
    );
    room.submit(request(&f.config, "@all after clear")).await;
    f.wait(|| f.count("message.final") == 6).await;
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn room_full_debate() {
    let _ = tracing_subscriber::fmt().try_init();
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(
            &f.config,
            "@member0 @member1 $debate $ROUNDS=2 @judge Compare designs",
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("verdict") == 1).await;
    assert_eq!(f.count("debate.round_score"), 2);
    assert_eq!(f.count("debate.turn"), 6);
    assert_eq!(f.fake.captured_bodies().await.len(), 9);
    f.assert_ledger();
    let events = room.events_after(None, 500).await.unwrap();
    let verdict = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::Verdict(v) = &e.content {
                Some(v)
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        verdict
            .verdict
            .weighted_totals
            .iter()
            .all(|t| t.total == 5.0)
    );
    assert!(verdict.verdict.round_tally.values().all(|t| *t == 14));
    let state = room.state().await;
    assert_eq!(state.daily_spend.spent_usd, 0.000405);
    assert_eq!(state.daily_spend.reserved_usd, 0.);
    assert_eq!(state.daily_spend.unknown_calls, 0);
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM reservations", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        f.db()
            .query_row("SELECT SUM(reported_cost_micros) FROM attempts", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
        405
    );
    for body in f.fake.captured_bodies().await {
        if body["model"] != "fixture/judge" {
            assert!(!body.to_string().contains("roundTally"));
        }
    }
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn one_shot_debate_wakes_every_dependent_call_and_leaves_unrelated_work_dormant() {
    let f = Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    let unrelated = request(&f.config, "@member5 dormant canary");
    assert!(matches!(
        room.submit_commit_only(unrelated).await,
        RequestResult::Accepted { .. }
    ));
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::OneShot).await;
    assert!(matches!(
        room.submit(request(
            &f.config,
            "@member0 @member1 $debate $ROUNDS=2 @judge Compare designs"
        ))
        .await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| f.count("verdict") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 9);
    assert_eq!(f.db().query_row("SELECT COUNT(*) FROM work WHERE target='member5' AND status='queued' AND attempt=0",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.count("message.final") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 10);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_pending_debate_reuses_committed_sources_without_duplicate_calls() {
    let mut f = Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@member0 @member1 Compare designs"))
        .await;
    f.wait(|| f.count("message.final") == 2).await;
    let command = request(
        &f.config,
        "@member0 @member1 $debate $ROUNDS=2 @judge Compare designs",
    );
    let RequestResult::PendingConfirmation {
        preflight: PendingPreflight::Debate(plan),
        ..
    } = room.submit(command.clone()).await
    else {
        panic!("expected pending plan")
    };
    assert!(!plan.opening_round_required);
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.state().await.pending_preflights.len(), 1);
    let confirm = squad_protocol::Request::DebateConfirm {
        request_id: EventId::new(),
        debate_id: plan.debate_id,
        plan_id: plan.plan_id,
    };
    assert!(matches!(
        room.submit(confirm.clone()).await,
        RequestResult::Accepted { .. }
    ));
    f.wait(|| f.count("verdict") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 9);
    room.submit(confirm).await;
    room.submit(command).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 9);
    assert_eq!(f.count("debate.turn"), 4);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(f.count("verdict"), 1);
    assert_eq!(
        room.state()
            .await
            .scoreboard
            .iter()
            .map(|s| s.wins)
            .sum::<u64>(),
        1
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn daily_max_refuses_new_calls_and_new_contest_reservations() {
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.000045;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, "@member0 first")).await;
    f.wait(|| f.count("message.final") == 1).await;
    room.submit(request(&f.config, "@member0 over cap")).await;
    f.wait(|| {
        f.db()
            .query_row("SELECT COUNT(*) FROM work WHERE status='failed'", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            == 1
    })
    .await;
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    let result = room
        .submit(request(
            &f.config,
            "@member0 @member1 $debate $ROUNDS=2 @judge Over cap",
        ))
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
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    assert_eq!(room.state().await.daily_spend.spent_usd, 0.000045);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn alias_failure_degrades_one_member_and_persists_billed_probes_and_catalog_cache() {
    use wiremock::matchers::body_partial_json;
    let mut f = Fixture::new().await;
    f.config.agents[0].model = "~fixture/offline-latest".into();
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_partial_json(
            serde_json::json!({"model":"~fixture/offline-latest"}),
        ))
        .respond_with(alias_probe("unlisted/model"))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(
        room.state().await.participants[0].status,
        PresenceStatus::Offline
    );
    room.submit(request(
        &f.config,
        "@member0 @member1 valid member still works",
    ))
    .await;
    f.wait(|| f.count("message.final") == 1).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    assert_eq!(f.db().query_row("SELECT COUNT(*) FROM attempts WHERE context_id='probe:member0' AND reported_cost_micros=45",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(room.state().await.daily_spend.spent_usd, 0.000090);
    room.shutdown().await.unwrap();
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .respond_with(ResponseTemplate::new(503))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    let raw: String = f
        .db()
        .query_row("SELECT snapshot_json FROM startup_resolution", [], |r| {
            r.get(0)
        })
        .unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(snapshot["cached"], true);
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM attempts WHERE context_id='probe:member0'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 3);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn admitted_debate_restarts_before_dispatch_and_finishes_without_duplicate_calls() {
    let f=Fixture::new().await;
    let room=f.open(OpenMode::OneShot).await;
    let command=request(&f.config,"@member0 @member1 $debate $ROUNDS=2 @judge Compare designs");
    let receipt=room.submit_commit_only(command.clone()).await;
    assert!(matches!(receipt,RequestResult::Accepted{..}));
    assert!(f.fake.captured_bodies().await.is_empty());
    assert_eq!(f.db().query_row("SELECT COUNT(*) FROM work WHERE kind='debate.turn' AND status='queued'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    room.shutdown().await.unwrap();
    let room=f.open(OpenMode::Interactive).await;
    assert_eq!(room.submit(command).await,receipt);
    f.wait(||f.count("verdict")==1).await;
    assert_eq!(f.fake.captured_bodies().await.len(),9);
    assert_eq!(f.db().query_row("SELECT COUNT(*) FROM attempts",[],|r|r.get::<_,i64>(0)).unwrap(),9);
    assert_eq!(f.db().query_row("SELECT COUNT(*) FROM work",[],|r|r.get::<_,i64>(0)).unwrap(),9);
    f.assert_ledger();
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn explicit_judging_of_unjudged_debate_opens_fresh_case_without_replaying_turns() {
    let _=tracing_subscriber::fmt().try_init();
    let f=Fixture::new().await;
    let room=f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config,"@member0 @member1 $debate $ROUNDS=2 Compare designs")).await;
    f.wait(||f.count("debate.turn")==6 && f.db().query_row("SELECT active FROM contests",[],|r|r.get::<_,i64>(0)).unwrap()==0).await;
    let result=room.submit(squad_protocol::Request::Judge{request_id:EventId::new(),debate_id:None}).await;
    assert!(matches!(result,RequestResult::Accepted{..}),"{result:?}");
    f.wait(||f.count("verdict")==1).await;
    assert_eq!(f.fake.captured_bodies().await.len(),7);
    assert_eq!(f.count("debate.turn"),6);
    assert_eq!(f.count("case.opened"),1);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn question_first_pick_reuses_answers_and_drives_the_selected_debate() {
    let f=Fixture::new().await;
    let room=f.open(OpenMode::OneShot).await;
    room.submit(request(&f.config,"@all $debate $ROUNDS=2 Compare designs?")).await;
    f.wait(||f.count("contest.pick_requested")==1).await;
    let state=room.state().await;
    let Some(ContestState::Selection(pick))=state.active_contest else{panic!("expected pending pick")};
    assert_eq!(f.fake.captured_bodies().await.len(),6);
    let result=room.submit(squad_protocol::Request::ContestPick{request_id:EventId::new(),contest_id:pick.contest_id,nicks:vec![nick("member0"),nick("member1")]}).await;
    assert!(matches!(result,RequestResult::Accepted{..}),"{result:?}");
    f.wait(||f.count("verdict")==1).await;
    assert_eq!(f.count("debate.turn"),4);
    assert_eq!(f.fake.captured_bodies().await.len(),13);
    room.shutdown().await.unwrap();
}
