//! Real start commands for the eleven new modes and a router wired to admit them.
#![allow(dead_code)]
use squad_core::router::Router;
use squad_protocol::*;
use squad_store::Store;

pub const STARTS: [(&str, &str); 11] = [
    ("forge", "@all $forge Write a short launch post"),
    (
        "forecast",
        "@all $forecast Will the project ship this year?",
    ),
    (
        "decide",
        "@all $decide Choose storage\n--\nSQLite\nPostgreSQL",
    ),
    ("negotiate", "@all $negotiate Agree a support contract"),
    (
        "blow_it_up",
        "@all $blow_it_up Deploy without a rollback plan",
    ),
    ("readback", "@all $readback Keep records for thirty days"),
    ("debug", "@all $debug Requests stall\n--\nInspect logs only"),
    (
        "trace",
        "@all $trace When does it run?\n--\nIt runs on Mondays.",
    ),
    (
        "rank",
        "@all $rank Prioritize features\n--\nExport\nRecovery\nSearch",
    ),
    (
        "tune",
        "@all $tune Be concise\n--\nReturn one correct label",
    ),
    (
        "bass_ackwards_prd",
        "@all $bass_ackwards_prd A reliable status page",
    ),
];

pub const MEMBERS: [&str; 4] = ["alice", "bob", "carol", "dave"];

/// A store and router whose four members and judge all use one large-context fixture model.
pub fn room(dir: &std::path::Path) -> (Store, Router) {
    let agents: Vec<_> = MEMBERS
        .iter()
        .map(|n| {
            (
                n.to_string(),
                "test/model".to_string(),
                format!("persona of {n}"),
            )
        })
        .collect();
    room_with(dir, &agents, "test/model")
}

/// The same room over an explicit roster of `(nick, model slug, persona)` and a judge slug.
pub fn room_with(
    dir: &std::path::Path,
    agents: &[(String, String, String)],
    judge: &str,
) -> (Store, Router) {
    let store = Store::open(":memory:").unwrap();
    let slugs: Vec<String> = agents
        .iter()
        .map(|a| a.1.clone())
        .chain(std::iter::once(judge.to_string()))
        .collect();
    store.call(move |db| {
        for slug in &slugs {
            db.execute("INSERT OR IGNORE INTO catalog VALUES (?,1000000,65536,'{\"prompt\":\"0\",\"completion\":\"0\"}','[]','2026-11-25T00:00:00Z')", [slug])?;
        }
        Ok(())
    }).unwrap();
    let roster = agents
        .iter()
        .map(|a| MemberNick::new(&a.0).unwrap())
        .collect();
    let bindings = agents
        .iter()
        .map(|a| (a.0.clone(), a.1.clone()))
        .chain(std::iter::once(("judge".to_string(), judge.to_string())))
        .map(|(n, model)| {
            (
                n,
                ExecutorBinding {
                    model: Text::new(&model).unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )
        })
        .collect();
    let mut router = Router::new(store.clone(), roster, bindings, 20_000_000).unwrap();
    let mut config = SquadConfig::load_with_env(
        &config::LoadOptions {
            env_file: None,
            data_dir: Some(dir.join("data")),
            working_dir: dir.into(),
            config_dir: None,
            default_data_dir: dir.join("data"),
        },
        &std::collections::BTreeMap::from([(
            "OPENROUTER_API_KEY".to_string(),
            "fake-key".to_string(),
        )]),
    )
    .unwrap();
    config.agents = agents
        .iter()
        .map(|(nick, model, persona)| config::AgentConfig {
            nick: MemberNick::new(nick).unwrap(),
            model: model.clone(),
            persona: persona.clone(),
            reasoning_effort: None,
        })
        .collect();
    config.judge.model = judge.into();
    config.budget.daily_max_usd = 200.;
    config.budget.debate_confirm_usd = 100.;
    router.engine_config = Some(config);
    (store, router)
}

/// Parse `text` against the router's current context and process it.
pub fn start(router: &mut Router, text: &str) -> squad_store::Result<RequestResult> {
    let ctx = router.parse_context().unwrap();
    let meta = parser::SubmissionMeta::default();
    let parsed = parse(text, &meta, &ctx).unwrap();
    router
        .process(
            Request::from_parsed_with_context(EventId::new(), parsed, &meta, &ctx, false).unwrap(),
            None,
        )
        .map(|o| o.result)
}
