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
        if body["model"] == "fixture/judge" {
            let user = body["messages"][1]["content"].as_str().unwrap();
            let data = user
                .split("Tournament prompt and anonymized answers, supplied as evidence:\n")
                .nth(1)
                .unwrap()
                .trim_start();
            let mut de = serde_json::Deserializer::from_str(data);
            let data: serde_json::Value = serde::Deserialize::deserialize(&mut de).unwrap();
            structured_judge(
                serde_json::json!({"ranking":data["entries"].as_array().unwrap().iter().map(|e|serde_json::json!({"label":e["label"],"score":8,"critique":"Supported proposal"})).collect::<Vec<_>>()}),
            )
        } else {
            streaming(
                &format!(
                    "ANSWER_{}: I am MEMBER0 from fixture/member0. PERSONA_0",
                    body["model"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("fixture/member")
                ),
                "stop",
            )
        }
    }
}
#[tokio::test]
async fn tournament_happy_path_is_blind_private_and_exactly_once() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_test_writer()
        .try_init();
    let f = Fixture::new().await;
    let room = f.open(OpenMode::Interactive).await;
    let result = room
        .submit(request(
            &f.config,
            "@member0 @member1 @member2 $tournament Design a bridge",
        ))
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.count("tournament.champion") == 1).await;
    assert_eq!(f.count("tournament.entry"), 5);
    assert_eq!(f.count("tournament.ranking"), 2);
    assert_eq!(f.count("tournament.result"), 5);
    assert_eq!(f.count("contest.picked"), 0);
    let db = f.db();
    let trophies: i64 = db
        .query_row("SELECT SUM(trophies) FROM scores", [], |r| r.get(0))
        .unwrap();
    assert_eq!(trophies, 1);
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM tournament_results WHERE eliminated=1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    for body in f.fake.captured_bodies().await {
        if body["model"] == "fixture/judge" {
            let messages = body["messages"].to_string();
            for token in ["member0", "MEMBER0", "fixture/", "PERSONA_", "labelMap"] {
                assert!(!messages.contains(token), "{messages}");
            }
            assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        }
    }
    for i in 0..3 {
        let own = format!("member{i}");
        let mut q=db.prepare("SELECT content FROM history WHERE context_id=? AND projection_key IN ('tournament.entry','tournament.result')").unwrap();
        let rows = q
            .query_map([&own], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        for other in 0..3 {
            if other != i {
                assert!(
                    rows.iter()
                        .all(|row| !row.contains(&format!("ANSWER_{other}")))
                );
            }
        }
    }
    room.shutdown().await.unwrap();
}
#[derive(Clone)]
struct Scripted {
    invalid: usize,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    fail_member: bool,
    judge_delay: Duration,
}
impl Respond for Scripted {
    fn respond(&self, r: &HttpRequest) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
        if body["model"] == "fixture/judge" {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < self.invalid {
                return streaming("{\"ranking\":[]}", "stop").set_delay(self.judge_delay);
            }
            Answers.respond(r).set_delay(self.judge_delay)
        } else if self.fail_member && body["model"] == "fixture/member0" {
            ResponseTemplate::new(500)
        } else {
            Answers.respond(r)
        }
    }
}
impl Fixture {
    async fn script(
        &self,
        invalid: usize,
        fail_member: bool,
        judge_delay: Duration,
    ) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(Scripted {
                invalid,
                calls: calls.clone(),
                fail_member,
                judge_delay,
            })
            .with_priority(1)
            .mount(&self.fake.server)
            .await;
        calls
    }
    fn terminal(&self, status: &str) -> bool {
        self.db().query_row("SELECT EXISTS(SELECT 1 FROM contests WHERE kind='tournament' AND json_extract(state_json,'$.state')=?)",[status],|r|r.get(0)).unwrap()
    }
}
const COMMAND: &str = "@member0 @member1 @member2 $tournament Design a bridge";
#[tokio::test]
async fn tournament_repairs_twice_then_accepts_without_replay() {
    let f = Fixture::new().await;
    let calls = f.script(2, false, Duration::ZERO).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, COMMAND)).await;
    f.wait(|| f.terminal("COMPLETE")).await;
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 4);
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM judge_repairs", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(f.count("tournament.entry"), 5);
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn tournament_judge_exhaustion_replays_once_and_preserves_failed_evidence() {
    let f = Fixture::new().await;
    let calls = f.script(usize::MAX, false, Duration::ZERO).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, COMMAND)).await;
    f.wait(|| f.terminal("FAILED")).await;
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 6);
    assert_eq!(f.count("tournament.entry"), 6);
    assert_eq!(f.count("tournament.result"), 0);
    assert_eq!(f.count("tournament.champion"), 0);
    assert_eq!(
        f.db()
            .query_row("SELECT COUNT(*) FROM tournament_rounds", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn tournament_single_forfeit_is_last_with_nullable_score_and_critique() {
    let f = Fixture::new().await;
    f.script(0, true, Duration::ZERO).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, COMMAND)).await;
    f.wait(|| f.terminal("COMPLETE")).await;
    let (rank, score, critique, eliminated): (i64, Option<i64>, Option<String>, bool) = f
        .db()
        .query_row(
            "SELECT rank,score,critique,eliminated FROM tournament_results WHERE nick='member0'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!((rank, score, critique, eliminated), (3, None, None, true));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn tournament_stop_fences_judge_releases_holds_and_clear_removes_trophies() {
    let f = Fixture::new().await;
    f.script(0, false, Duration::from_secs(2)).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, COMMAND)).await;
    f.wait(|| f.count("tournament.entry") == 3).await;
    room.submit(request(&f.config, "@member0 ordinary held canary"))
        .await;
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM history WHERE content LIKE '%ordinary held canary%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let result = room
        .submit(Request::Stop {
            request_id: EventId::new(),
            target: Some(nick("member1")),
        })
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    assert!(f.terminal("STOPPED"));
    assert_eq!(f.count("tournament.champion"), 0);
    f.wait(|| f.count("message.final") == 1).await;
    room.submit(Request::Clear {
        request_id: EventId::new(),
    })
    .await;
    for t in [
        "tournament_labels",
        "tournament_rounds",
        "tournament_results",
        "scores",
    ] {
        assert_eq!(
            f.db()
                .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn tournament_restart_mid_judging_retains_entries_and_only_one_champion() {
    let f = Fixture::new().await;
    f.script(0, false, Duration::from_millis(800)).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(request(&f.config, COMMAND)).await;
    f.wait(|| f.count("tournament.entry") == 3 && f.terminal("JUDGING"))
        .await;
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    f.wait(|| f.terminal("COMPLETE")).await;
    assert_eq!(f.count("tournament.entry"), 5);
    assert_eq!(f.count("tournament.champion"), 1);
    assert_eq!(
        f.db()
            .query_row("SELECT SUM(trophies) FROM scores", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn tournament_daily_max_refuses_without_dispatch() {
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.0001;
    let room = f.open(OpenMode::Interactive).await;
    let result = room.submit(request(&f.config, COMMAND)).await;
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
    assert_eq!(f.count("tournament.entry"), 0);
    assert_eq!(
        f.db()
            .query_row(
                "SELECT COUNT(*) FROM work WHERE contest_id IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    room.shutdown().await.unwrap();
}
#[test]
fn tournament_call_counts_and_unicode_redaction() {
    use squad_core::orchestrator::tournament::*;
    assert_eq!(call_counts(3), (7, 32));
    assert_eq!(call_counts(6), (25, 110));
    let tokens = vec![
        "fable".into(),
        "anthropic/claude-sonnet".into(),
        "anthropic".into(),
        "claude".into(),
        "Friendly expert".into(),
        "éclair".into(),
    ];
    assert_eq!(
        redact(
            "FABLE, anthropic/claude-sonnet; Friendly Expert! ÉCLAIR fableish",
            &tokens
        ),
        "[redacted], [redacted]; [redacted]! [redacted] fableish"
    );
}
