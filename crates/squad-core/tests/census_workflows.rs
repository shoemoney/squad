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
        let data = text.rsplit_once("\n{\"").map(|_| ()).is_some();
        let _ = data;
        let material = extract_data(text);
        let output = if text.contains("Hunt independently") {
            serde_json::json!({"gaps":[{"localId":"one","omission":"No backup process","consequence":"Data may be lost"}]})
        } else if text.contains("Merge the independent findings") {
            serde_json::json!({"ledger":[{"id":"gap","omission":"No backup process","consequence":"Data may be lost","lenses":["failure modes"],"sourceFindIds":material["sourceIds"]}]})
        } else if text.contains("Independently rate") {
            serde_json::json!({"ratings":material["ledger"].as_array().unwrap().iter().map(|g|serde_json::json!({"gapId":g["id"],"rating":"real","reason":"Missing","citation":null})).collect::<Vec<_>>(),"additions":[]})
        } else if text.contains("Finalize the ledger") {
            serde_json::json!({"confirmed":material["outcomes"].as_array().unwrap().iter().map(|g|serde_json::json!({"gapId":g["gap"]["id"],"priority":"Prevent permanent loss"})).collect::<Vec<_>>(),"covered":[],"trivial":[],"additions":[]})
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
