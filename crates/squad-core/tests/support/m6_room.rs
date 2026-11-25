#![allow(dead_code)]
#[path = "../../../squad-agents/tests/support/fake_openrouter.rs"]
pub mod fake_openrouter;
use fake_openrouter::*;
use squad_agents::openrouter::OpenRouterClient;
use squad_core::room::RoomHandle;
use squad_protocol::*;
use std::{collections::BTreeMap, time::Duration};
pub const KEY: &str = "M6-SENTINEL-CREDENTIAL-NEVER-PERSIST";
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub config: SquadConfig,
    pub fake: FakeOpenRouter,
}
impl Fixture {
    pub async fn new() -> Self {
        Self::with_roster(2).await
    }
    pub async fn with_roster(count: usize) -> Self {
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
            &BTreeMap::from([("OPENROUTER_API_KEY".into(), KEY.into())]),
        )
        .unwrap();
        config.agents = (0..count)
            .map(|i| config::AgentConfig {
                nick: format!("member{i}").parse().unwrap(),
                model: format!("fixture/member{i}"),
                persona: format!("PERSONA_{i}"),
                reasoning_effort: None,
            })
            .collect();
        config.judge.model = "fixture/judge".into();
        config.budget.debate_confirm_usd = 10.;
        let fake = FakeOpenRouter::start().await;
        let models = config.agents.iter().map(|a| a.model.clone()).chain(std::iter::once(config.judge.model.clone())).map(|id| serde_json::json!({"id":id,"context_length":1000000,"top_provider":{"max_completion_tokens":65536},"supported_parameters":["reasoning"],"pricing":{"prompt":"0.000001","completion":"0.000002"}})).collect::<Vec<_>>();
        fake.catalog(serde_json::json!({"data":models})).await;
        Self { dir, config, fake }
    }
    pub async fn open(&self, mode: OpenMode) -> RoomHandle {
        RoomHandle::open_with_client(
            self.config.clone(),
            "main",
            mode,
            OpenRouterClient::new(&self.fake.base_url(), KEY).unwrap(),
        )
        .await
        .unwrap()
    }
    pub fn request(&self, text: &str) -> Request {
        let context = parser::ParseContext {
            roster: self.config.agents.iter().map(|a| a.nick.clone()).collect(),
            ..Default::default()
        };
        let meta = parser::SubmissionMeta::default();
        Request::from_parsed(
            EventId::new(),
            parse(text, &meta, &context).unwrap(),
            &meta,
            false,
        )
        .unwrap()
    }
    pub fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.config.data_dir.join("squad.db")).unwrap()
    }
    pub fn scalar(&self, sql: &str) -> i64 {
        self.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }
    pub async fn wait(&self, condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("M6 condition timed out");
    }
    pub async fn finals(&self, count: i64) {
        self.wait(|| {
            self.scalar("SELECT COUNT(*) FROM events WHERE kind='message.final'") == count
        })
        .await;
    }
}
