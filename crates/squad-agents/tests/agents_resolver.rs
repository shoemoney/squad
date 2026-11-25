#[allow(dead_code)]
mod support;
use futures::StreamExt;
use serde_json::json;
use squad_agents::{
    openrouter::*,
    registry::{Registry, TaskKind},
    resolver::*,
};
use squad_protocol::config::{LoadOptions, SquadConfig};
use std::{collections::BTreeMap, path::PathBuf};
use support::fake_openrouter::*;
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};
fn config(selection: &str) -> SquadConfig {
    let root = PathBuf::from("/nonexistent/squad-resolver-fixture");
    SquadConfig::load_with_env(
        &LoadOptions {
            env_file: None,
            data_dir: None,
            working_dir: root.clone(),
            config_dir: None,
            default_data_dir: root.join("data"),
        },
        &BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "fake-key".into()),
            ("SQUAD_MODEL_A".into(), selection.into()),
            ("SQUAD_JUDGE_MODEL".into(), MODEL.into()),
        ]),
    )
    .unwrap()
}
fn client(fake: &FakeOpenRouter) -> OpenRouterClient {
    OpenRouterClient::new(&fake.base_url(), "fake-key").unwrap()
}
#[tokio::test]
async fn concrete_identity_and_catalog_are_exact_and_shared() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    let result = resolve(&client(&fake), &config(MODEL), None, 123)
        .await
        .unwrap();
    assert_eq!(result.members[0].model.as_ref().unwrap().id, MODEL);
    assert_eq!(result.judge.model, result.members[0].model);
    assert!(fake.captured_bodies().await.is_empty());
    assert_eq!(fake.server.received_requests().await.unwrap().len(), 1);
    assert!(result.diagnostics[0].contains("catalog_at=123"));
}
#[tokio::test]
async fn unknown_concrete_names_its_environment_variable() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    let error = resolve(&client(&fake), &config("unknown/model"), None, 0)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("SQUAD_MODEL_A"));
    assert!(fake.captured_bodies().await.is_empty());
}
#[tokio::test]
async fn alias_absent_from_catalog_probes_and_retries_only_http_400() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![ResponseTemplate::new(400), alias_probe(MODEL)])
        .await;
    let result = resolve(&client(&fake), &config("~fixture/latest"), None, 0)
        .await
        .unwrap();
    assert_eq!(result.members[0].model.as_ref().unwrap().id, MODEL);
    let bodies = fake.captured_bodies().await;
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0]["max_tokens"], 16);
    assert_eq!(bodies[1]["max_tokens"], 2048);
    assert_eq!(bodies[0]["stream"], false);
    assert!(bodies[0].get("reasoning").is_none());
}
#[tokio::test]
async fn catalog_supported_probe_uses_lowest_effort() {
    let fake = FakeOpenRouter::start().await;
    let mut data = catalog();
    let mut alias = data["data"][0].clone();
    alias["id"] = json!("~fixture/latest");
    alias["supported_reasoning_efforts"] = json!(["high", "minimal", "low"]);
    data["data"].as_array_mut().unwrap().push(alias);
    fake.catalog(data).await;
    fake.completions(vec![alias_probe(MODEL)]).await;
    resolve(&client(&fake), &config("~fixture/latest"), None, 0)
        .await
        .unwrap();
    assert_eq!(
        fake.captured_bodies().await[0]["reasoning"]["effort"],
        "minimal"
    );
}
#[tokio::test]
async fn failed_and_invalid_alias_targets_are_offline_without_guessing() {
    for response in [
        ResponseTemplate::new(503),
        alias_probe("missing/identity"),
        alias_probe("~fixture/latest"),
        ResponseTemplate::new(200).set_body_json(json!({})),
    ] {
        let fake = FakeOpenRouter::start().await;
        fake.catalog(catalog()).await;
        fake.completions(vec![response]).await;
        let result = resolve(&client(&fake), &config("~fixture/latest"), None, 0)
            .await
            .unwrap();
        assert!(result.members[0].model.is_none());
        assert!(
            result.members[0]
                .error
                .as_ref()
                .unwrap()
                .contains("SQUAD_MODEL_A")
        );
        assert!(result.judge.model.is_some());
        assert_eq!(fake.captured_bodies().await.len(), 1);
    }
}
#[tokio::test]
async fn cache_requires_three_transient_failures_and_no_more_than_24_hours() {
    for (age, expected) in [(86400, true), (86401, false)] {
        let fake = FakeOpenRouter::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&fake.server)
            .await;
        let cache = CatalogSnapshot {
            fetched_at: 100,
            models: normalize_catalog(catalog()).unwrap(),
        };
        let result = acquire_catalog(&client(&fake), Some(&cache), 100 + age).await;
        assert_eq!(result.is_ok(), expected);
        assert_eq!(fake.server.received_requests().await.unwrap().len(), 3);
    }
    let fake = FakeOpenRouter::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&fake.server)
        .await;
    let cache = CatalogSnapshot {
        fetched_at: 100,
        models: normalize_catalog(catalog()).unwrap(),
    };
    assert!(
        acquire_catalog(&client(&fake), Some(&cache), 101)
            .await
            .is_err()
    );
    assert_eq!(fake.server.received_requests().await.unwrap().len(), 1);
}
#[tokio::test]
async fn fresh_catalog_is_authoritative_and_cache_never_replaces_probe() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(json!({"data": []})).await;
    let cache = CatalogSnapshot {
        fetched_at: 0,
        models: normalize_catalog(catalog()).unwrap(),
    };
    assert!(
        resolve(&client(&fake), &config(MODEL), Some(&cache), 1)
            .await
            .is_err()
    );
    let fake = FakeOpenRouter::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&fake.server)
        .await;
    fake.completions(vec![alias_probe(MODEL)]).await;
    let result = resolve(&client(&fake), &config("~fixture/latest"), Some(&cache), 1)
        .await
        .unwrap();
    assert!(result.cached);
    assert!(result.diagnostics[0].contains("WARNING"));
    assert_eq!(fake.captured_bodies().await.len(), 1);
}
#[tokio::test]
async fn completions_remain_pinned_and_metadata_exposes_mismatch() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![
        alias_probe(MODEL),
        response_model_mismatch("other/model"),
    ])
    .await;
    let cfg = config("~fixture/latest");
    let client = client(&fake);
    let result = resolve(&client, &cfg, None, 0).await.unwrap();
    let pinned = &result.members[0];
    let request = Registry::new(&cfg)
        .unwrap()
        .request(
            Some(&cfg.agents[0]),
            pinned.model.as_ref().unwrap(),
            TaskKind::Reply,
        )
        .unwrap();
    let events: Vec<_> = client
        .stream(request, CancellationToken::new())
        .collect()
        .await;
    assert!(events.iter().any(|event| {
        pinned
            .observe(event)
            .is_some_and(|s| s.contains("MODEL_IDENTITY_MISMATCH"))
    }));
    assert_eq!(pinned.model.as_ref().unwrap().id, MODEL);
    assert_eq!(fake.captured_bodies().await[1]["model"], MODEL);
}
#[tokio::test]
async fn public_nonstream_probe_and_catalog_methods_use_existing_transport() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![alias_probe(MODEL)]).await;
    let c = client(&fake);
    assert!(c.fetch_catalog().await.unwrap()["data"].is_array());
    assert_eq!(
        c.probe_alias("~fixture/latest", 16).await.unwrap()["model"],
        MODEL
    );
    assert_eq!(
        c.probe_alias(MODEL, 16).await,
        Err(TransportError::InvalidModel)
    );
}

#[tokio::test]
async fn probe_attempts_retain_metadata_and_usage_without_reasoning_or_output() {
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![ResponseTemplate::new(400), alias_probe(MODEL)])
        .await;
    let result = resolve(&client(&fake), &config("~fixture/latest"), None, 0)
        .await
        .unwrap();
    let probes = &result.members[0].probes;
    assert_eq!(probes.len(), 2);
    assert!(probes[0].error.as_ref().unwrap().contains("400"));
    assert_eq!(probes[1].generation_id.as_deref(), Some(GENERATION));
    assert_eq!(probes[1].response_model.as_deref(), Some(MODEL));
    assert_eq!(probes[1].usage.as_ref().unwrap().cost, Some(0.000045));
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("PRIVATE REASONING")
    );
}

#[tokio::test]
async fn mode_fixtures_preserve_structured_outputs_and_exact_pass() {
    let output = json!({"fixture": ["anonymous A", "anonymous B"]});
    let mut responses = vec![
        round_score(output.clone()),
        tied_round_score("a", "b"),
        malformed_round_score(),
        delayed_round_score(output.clone(), std::time::Duration::ZERO),
        tournament_ranking(output.clone()),
        repair_output(output.clone()),
        grill_questions("Why?"),
        grill_pass(),
        grill_answer("Answer. Plan changes: none."),
        grill_rebuild("Rebuilt plan"),
    ];
    for fixture in [
        census_member,
        census_judge,
        crosscheck_member,
        crosscheck_judge,
        survey_member,
        survey_judge,
        probe_member,
        probe_judge,
        goal_extraction,
    ] {
        responses.push(fixture(output.clone()));
    }
    responses.push(delayed_mode(output, std::time::Duration::ZERO));
    responses.extend(repair_exhaustion(2));
    let count = responses.len();
    let fake = FakeOpenRouter::start().await;
    fake.completions(responses).await;
    let cfg = config(MODEL);
    let m = normalize_catalog(catalog()).unwrap().remove(MODEL).unwrap();
    let request = Registry::new(&cfg)
        .unwrap()
        .request(Some(&cfg.agents[0]), &m, TaskKind::Reply)
        .unwrap();
    for index in 0..count {
        let events: Vec<_> = client(&fake)
            .stream(request.clone(), CancellationToken::new())
            .collect()
            .await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::Complete { .. }))
        );
        let text: String = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::Delta { text, .. } = e {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        if index == 7 {
            assert_eq!(text, "PASS");
        }
    }
    assert_eq!(fake.captured_bodies().await.len(), count);
}
#[tokio::test]
async fn billing_fixtures_keep_unknown_cost_and_usage_before_failure() {
    let fake = FakeOpenRouter::start().await;
    fake.completions(vec![
        missing_cost(),
        billed_failure(),
        failed_round_score(),
        failed_member(),
    ])
    .await;
    let cfg = config(MODEL);
    let m = normalize_catalog(catalog()).unwrap().remove(MODEL).unwrap();
    let mut request = Registry::new(&cfg)
        .unwrap()
        .request(Some(&cfg.agents[0]), &m, TaskKind::Reply)
        .unwrap();
    request.policy = ExecutionPolicy::SingleAttempt(DeadlineClass::Ordinary);
    let events: Vec<_> = client(&fake)
        .stream(request.clone(), CancellationToken::new())
        .collect()
        .await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage {usage, ..} if usage.cost.is_none()))
    );
    let events: Vec<_> = client(&fake)
        .stream(request.clone(), CancellationToken::new())
        .collect()
        .await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::Usage {usage, ..} if usage.cost == Some(0.000045)))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::AttemptFailed { .. }))
    );
    for _ in 0..2 {
        let events: Vec<_> = client(&fake)
            .stream(request.clone(), CancellationToken::new())
            .collect()
            .await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::AttemptFailed { .. }))
        );
    }
}

#[tokio::test]
async fn cancelled_billed_call_and_delayed_cost_remain_reconcilable() {
    let fake = FakeOpenRouter::start().await;
    fake.completions(vec![billed_cancellation(std::time::Duration::from_secs(
        10,
    ))])
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/generation"))
        .respond_with(delayed_cost(std::time::Duration::from_millis(1)))
        .mount(&fake.server)
        .await;
    let cfg = config(MODEL);
    let model = normalize_catalog(catalog()).unwrap().remove(MODEL).unwrap();
    let request = Registry::new(&cfg)
        .unwrap()
        .request(Some(&cfg.agents[0]), &model, TaskKind::Reply)
        .unwrap();
    let cancel = CancellationToken::new();
    let c = client(&fake);
    let events = c.stream(request, cancel.clone()).collect::<Vec<_>>();
    let cancellation = async {
        // Wait until this specific fake has accepted the request before cancelling.
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if !fake.captured_bodies().await.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
    };
    let (events, ()) = tokio::join!(events, cancellation);
    assert!(events.iter().any(|e| matches!(
        e,
        StreamEvent::AttemptFailed {
            error: TransportError::Cancelled,
            ..
        }
    )));
    // Lookup is an accounting concern: exercise the delayed fixture directly.
    let value: serde_json::Value =
        reqwest::get(format!("{}generation?id={GENERATION}", fake.base_url()))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(value["data"]["id"], GENERATION);
    assert_eq!(value["data"]["total_cost"], 0.000045);
}

struct AliasStore(Option<AliasCacheEntry>);
impl ResolutionStore for AliasStore {
    type Error = ();
    fn cached_alias(&self, _: &str, _: &str) -> Result<Option<AliasCacheEntry>, ()> {
        Ok(self.0.clone())
    }
    fn save_alias(&mut self, _: &str, _: &str, entry: &AliasCacheEntry) -> Result<(), ()> {
        self.0 = Some(entry.clone());
        Ok(())
    }
    fn remove_alias(&mut self, _: &str, _: &str) -> Result<(), ()> {
        self.0 = None;
        Ok(())
    }
    fn cached_catalog(&self) -> Result<Option<CatalogSnapshot>, ()> {
        Ok(None)
    }
    fn save_resolution(&mut self, _: &ResolutionSnapshot) -> Result<(), ()> {
        Ok(())
    }
}
#[tokio::test]
async fn cached_alias_target_must_still_be_in_the_fresh_catalog() {
    let listed = normalize_catalog(catalog()).unwrap()[MODEL].clone();
    // A listed cached target is reused without probing.
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    let mut store = AliasStore(Some(AliasCacheEntry {
        model: listed.clone(),
        resolved_at: 0,
    }));
    let result = resolve_persisted(&client(&fake), &config("~fixture/latest"), 10, &mut store)
        .await
        .unwrap();
    assert_eq!(result.members[0].model.as_ref().unwrap().id, MODEL);
    assert!(fake.captured_bodies().await.is_empty());
    // A delisted cached target is re-resolved once and the cache is repaired.
    let fake = FakeOpenRouter::start().await;
    fake.catalog(catalog()).await;
    fake.completions(vec![alias_probe(MODEL)]).await;
    let mut gone = listed.clone();
    gone.id = "fixture/delisted".into();
    let mut store = AliasStore(Some(AliasCacheEntry {
        model: gone,
        resolved_at: 0,
    }));
    let result = resolve_persisted(&client(&fake), &config("~fixture/latest"), 10, &mut store)
        .await
        .unwrap();
    assert_eq!(result.members[0].model.as_ref().unwrap().id, MODEL);
    assert_eq!(fake.captured_bodies().await.len(), 1);
    assert_eq!(store.0.unwrap().model.id, MODEL);
}
