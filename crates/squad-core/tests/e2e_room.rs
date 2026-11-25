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
    dir: tempfile::TempDir,
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
        Self { dir, config, fake }
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
    async fn wait(&self, check: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !check() {
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
            if messages.contains("round_score") || messages.contains("round_judge") {
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
    assert!(matches!(
        room.submit(squad_protocol::Request::Clear {
            request_id: EventId::new()
        })
        .await,
        RequestResult::Accepted { .. }
    ));
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
            "/debate @member0 @member1 ROUNDS=2 JUDGE=on Compare designs",
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
    room.shutdown().await.unwrap();
}
