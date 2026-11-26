use super::*;
#[path = "../../squad-agents/tests/support/fake_openrouter.rs"]
mod fake;
use fake::*;
use squad_agents::{
    openrouter::*,
    registry::{Registry, TaskKind},
    resolver::*,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use wiremock::ResponseTemplate;

fn config(dir: &std::path::Path, provider: &str) -> SquadConfig {
    SquadConfig::load_with_env(
        &config::LoadOptions {
            env_file: None,
            data_dir: None,
            working_dir: dir.into(),
            config_dir: Some(dir.join("config")),
            default_data_dir: dir.join("data"),
        },
        &BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "fake-key".into()),
            ("OPENROUTER_BASE_URL".into(), provider.into()),
            ("SQUAD_MODEL_A".into(), "~fixture/member-latest".into()),
            ("SQUAD_JUDGE_MODEL".into(), "~fixture/judge-latest".into()),
        ]),
    )
    .unwrap()
}

#[tokio::test]
async fn durable_aliases_survive_reopen_without_renewing_ttl_and_expire_exactly() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![alias_probe(MODEL); 4]).await;
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path(), &fake.base_url());
    let client = OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap();
    let path = dir.path().join("cache.db");
    {
        let mut persistence =
            ResolutionPersistence::new(Store::open(&path).unwrap(), 1_000_000).unwrap();
        let first = resolve_persisted(&client, &cfg, 100, &mut persistence)
            .await
            .unwrap();
        assert_eq!(first.members[0].probes.len(), 1);
        assert_eq!(first.judge.probes.len(), 1);
    }
    // New SQLite connection and adapter, like a second process startup.
    let mut persistence =
        ResolutionPersistence::new(Store::open(&path).unwrap(), 1_000_000).unwrap();
    let second = resolve_persisted(&client, &cfg, 100 + 86399, &mut persistence)
        .await
        .unwrap();
    assert!(second.members[0].probes.is_empty());
    assert!(second.judge.probes.is_empty());
    assert_eq!(fake.captured_bodies().await.len(), 2);
    assert_eq!(
        persistence
            .cached_alias(&cfg.openrouter_base_url, &cfg.judge.model)
            .unwrap()
            .unwrap()
            .resolved_at,
        100
    );
    let expired = resolve_persisted(&client, &cfg, 100 + 86400, &mut persistence)
        .await
        .unwrap();
    assert_eq!(expired.members[0].probes.len(), 1);
    assert_eq!(expired.judge.probes.len(), 1);
    assert_eq!(fake.captured_bodies().await.len(), 4);
    let attempts = persistence
        .store
        .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))?))
        .unwrap();
    assert_eq!(attempts, 4);
}

#[tokio::test]
async fn configured_ttl_zero_refreshes_and_provider_endpoints_are_isolated() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![alias_probe(MODEL); 8]).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), &fake.base_url());
    let client = OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap();
    let mut persistence =
        ResolutionPersistence::new(Store::open(dir.path().join("cache.db")).unwrap(), 1_000_000)
            .unwrap();
    resolve_persisted(&client, &cfg, 100, &mut persistence)
        .await
        .unwrap();
    cfg.alias_cache_ttl_seconds = 0;
    resolve_persisted(&client, &cfg, 101, &mut persistence)
        .await
        .unwrap();
    assert_eq!(fake.captured_bodies().await.len(), 4);
    cfg.alias_cache_ttl_seconds = 10;
    resolve_persisted(&client, &cfg, 110, &mut persistence)
        .await
        .unwrap();
    assert_eq!(fake.captured_bodies().await.len(), 4);
    resolve_persisted(&client, &cfg, 111, &mut persistence)
        .await
        .unwrap();
    assert_eq!(fake.captured_bodies().await.len(), 6);
    cfg.openrouter_base_url.push_str("other-provider/");
    resolve_persisted(&client, &cfg, 112, &mut persistence)
        .await
        .unwrap();
    assert_eq!(fake.captured_bodies().await.len(), 8);
}

#[tokio::test]
async fn missing_cached_slug_repairs_once_and_preserves_frozen_work() {
    for failure in [
        ResponseTemplate::new(404),
        provider_error(404),
        ResponseTemplate::new(400).set_body_json(
            serde_json::json!({"error":{"message":"model fixture/model not found"}}),
        ),
    ] {
        let fake = FakeOpenRouter::start().await;
        let mut data = catalog();
        let mut replacement = data["data"][0].clone();
        replacement["id"] = serde_json::json!("fixture/replacement");
        data["data"].as_array_mut().unwrap().push(replacement);
        fake.catalog(data).await;
        fake.completions(vec![
            alias_probe(MODEL),
            failure.clone(),
            alias_probe("fixture/replacement"),
            failure,
        ])
        .await;
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config(dir.path(), &fake.base_url());
        cfg.judge.model = MODEL.into();
        let store = Store::open(dir.path().join("cache.db")).unwrap();
        let mut persistence = ResolutionPersistence::new(store.clone(), 1_000_000).unwrap();
        let client = OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap();
        let initial = resolve_persisted(&client, &cfg, 100, &mut persistence)
            .await
            .unwrap();
        let request = Registry::new(&cfg)
            .unwrap()
            .request(
                Some(&cfg.agents[0]),
                initial.members[0].model.as_ref().unwrap(),
                TaskKind::Reply,
            )
            .unwrap();
        let client = client.with_model_recovery(Arc::new(AliasRecovery {
            store: store.clone(),
            config: cfg.clone(),
            max: 1_000_000,
            refreshed: tokio::sync::Mutex::new(Default::default()),
        }));
        for _ in 0..2 {
            let mut stream = client.stream(request.clone(), CancellationToken::new());
            let mut failed = false;
            while let Some(event) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
                failed |= matches!(event, StreamEvent::AttemptFailed { .. });
            }
            assert!(failed);
        }
        let next = resolve_persisted(
            &client,
            &cfg,
            chrono::Utc::now().timestamp() as u64,
            &mut persistence,
        )
        .await
        .unwrap();
        assert_eq!(
            next.members[0].model.as_ref().unwrap().id,
            "fixture/replacement"
        );
        assert!(next.members[0].probes.is_empty());
        let bodies = fake.captured_bodies().await;
        assert_eq!(bodies.len(), 4);
        assert_eq!(bodies[1]["model"], MODEL);
        assert_eq!(bodies[2]["model"], "~fixture/member-latest");
        assert_eq!(bodies[3]["model"], MODEL);
        assert_eq!(request.model, MODEL);
    }
}

#[test]
fn ttl_uses_normal_configuration_loader_and_rejects_invalid_values() {
    let dir = tempfile::tempdir().unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: None,
        working_dir: dir.path().into(),
        config_dir: Some(dir.path().join("config")),
        default_data_dir: dir.path().join("data"),
    };
    for (value, expected) in [
        ("0", Some(0)),
        ("60", Some(60)),
        ("-1", None),
        ("invalid", None),
    ] {
        let result = SquadConfig::load_with_env(
            &options,
            &BTreeMap::from([
                ("OPENROUTER_API_KEY".into(), "fixture".into()),
                ("SQUAD_ALIAS_CACHE_TTL_SECONDS".into(), value.into()),
            ]),
        );
        if let Some(expected) = expected {
            assert_eq!(result.unwrap().alias_cache_ttl_seconds, expected);
        } else {
            assert_eq!(result.unwrap_err(), config::ConfigError::AliasCacheTtl);
        }
    }
}

#[tokio::test]
async fn concurrent_member_and_judge_share_a_successful_alias_probe() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![alias_probe(MODEL)]).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = config(dir.path(), &fake.base_url());
    cfg.judge.model = cfg.agents[0].model.clone();
    let mut persistence =
        ResolutionPersistence::new(Store::open(dir.path().join("cache.db")).unwrap(), 1_000_000)
            .unwrap();
    let client = OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap();
    let result = resolve_persisted(&client, &cfg, 100, &mut persistence)
        .await
        .unwrap();
    assert_eq!(fake.captured_bodies().await.len(), 1);
    assert_eq!(result.members[0].model, result.judge.model);
    assert_eq!(result.members[0].probes.len(), 1);
    assert!(result.judge.probes.is_empty());
    let (attempts, cost) = persistence
        .store
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))?,
                db.query_row("SELECT SUM(spent_micros) FROM spend_daily", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!((attempts, cost), (1, 45));
}
