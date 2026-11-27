#[path = "support/m6_room.rs"]
mod support;
use squad_protocol::*;
use std::time::Duration;
use support::{fake_openrouter::*, *};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

#[tokio::test]
async fn provider_retries_replace_partial_output_and_reconcile_each_paid_attempt() {
    for failure in [
        rate_limited("0"),
        ResponseTemplate::new(503),
        disconnected(),
    ] {
        let f = Fixture::new().await;
        f.fake
            .completions(vec![failure, streaming("replacement", "stop")])
            .await;
        f.fake.generation(GENERATION, serde_json::json!({"tokens_prompt":11,"tokens_completion":17,"total_cost":0.000025})).await;
        let room = f.open(OpenMode::Interactive).await;
        room.submit(f.request("@member0 retry safely")).await;
        f.finals(1).await;
        let events = room.events_after(None, 500).await.unwrap();
        let finals: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let EnvelopePayload::MessageFinal(m) = &e.content {
                    Some(m)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(finals[0].content, "replacement");
        assert_eq!(finals[0].completed.generation.attempt, 2);
        assert_eq!(f.fake.captured_bodies().await.len(), 2);
        assert_eq!(f.scalar("SELECT COUNT(*) FROM attempts"), 2);
        assert_eq!(
            f.scalar("SELECT SUM(reported_cost_micros) FROM attempts"),
            f.scalar("SELECT SUM(spent_micros) FROM spend_daily")
        );
        room.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn exhausted_member_does_not_block_other_member_or_persist_provisional_text() {
    let f = Fixture::new().await;
    f.fake
        .completions(vec![streaming("healthy reply", "stop")])
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_partial_json(
            serde_json::json!({"model":"fixture/member0"}),
        ))
        .respond_with(ResponseTemplate::new(503))
        .with_priority(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@all independent progress")).await;
    f.finals(1).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM work WHERE status='failed'") == 1)
        .await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM attempts WHERE target='member0'"),
        2
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM attempts WHERE target='member1'"),
        1
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn real_room_daily_max_exact_message_restart_and_raised_max() {
    let mut f = Fixture::new().await;
    f.config.budget.daily_max_usd = 0.000045;
    f.fake.server.reset().await;
    f.fake.catalog(serde_json::json!({"data":[
        {"id":"fixture/member0","context_length":1000000,"pricing":{"prompt":"0.000000000001","completion":"0.000000000002"}},
        {"id":"fixture/member1","context_length":1000000,"pricing":{"prompt":"0.000000000001","completion":"0.000000000002"}},
        {"id":"fixture/judge","context_length":1000000,"pricing":{"prompt":"0.000000000001","completion":"0.000000000002"}}
    ]})).await;
    f.fake
        .completions(vec![
            streaming("first", "stop"),
            streaming("raised", "stop"),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 paid")).await;
    f.finals(1).await;
    room.submit(f.request("@member0 refused")).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM work WHERE status='failed'") == 1)
        .await;
    let events = room.events_after(None, 500).await.unwrap();
    assert!(events.iter().any(|e|matches!(&e.content,EnvelopePayload::WorkState(WorkStateEvent {error:Some(error),..}) if error.code==ErrorCode::DAILY_MAX_REACHED && error.text.as_str().starts_with("Daily max of $0.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow."))));
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    room.shutdown().await.unwrap();
    f.config.budget.daily_max_usd = 1.;
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(room.state().await.daily_spend.spent_usd, 0.000045);
    room.submit(f.request("@member0 raised cap")).await;
    f.finals(2).await;
    assert_eq!(room.state().await.daily_spend.spent_usd, 0.000090);
    let before = f.scalar("SELECT SUM(spent_micros) FROM spend_daily");
    room.submit(Request::Clear {
        request_id: EventId::new(),
    })
    .await;
    assert_eq!(
        f.scalar("SELECT SUM(spent_micros) FROM spend_daily"),
        before
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn missing_cost_unknown_count_and_current_reservations_survive_restart() {
    let f = Fixture::new().await;
    f.fake.completions(vec![missing_cost()]).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 cost unknown")).await;
    f.finals(1).await;
    let before = room.state().await.daily_spend;
    assert_eq!(before.unknown_calls, 1);
    assert_eq!(before.spent_usd, 0.);
    room.shutdown().await.unwrap();
    let room = f.open(OpenMode::Interactive).await;
    let after = room.state().await.daily_spend;
    assert_eq!(after.unknown_calls, before.unknown_calls);
    assert_eq!(after.reserved_usd, before.reserved_usd);
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn no_wait_and_receipt_replay_resume_once_on_interactive_restart() {
    let f = Fixture::new().await;
    f.fake.completions(vec![streaming("resumed", "stop")]).await;
    let request = f.request("@member0 dormant");
    let room = f.open(OpenMode::OneShot).await;
    let receipt = room.submit_commit_only(request.clone()).await;
    assert!(matches!(
        receipt,
        RequestResult::Accepted {
            cursor: Some(_),
            ..
        }
    ));
    room.shutdown().await.unwrap();
    assert!(f.fake.captured_bodies().await.is_empty());
    let room = f.open(OpenMode::OneShot).await;
    assert_eq!(room.submit_commit_only(request.clone()).await, receipt);
    room.shutdown().await.unwrap();
    assert!(f.fake.captured_bodies().await.is_empty());
    let room = f.open(OpenMode::Interactive).await;
    f.finals(1).await;
    assert_eq!(room.submit(request).await, receipt);
    room.shutdown().await.unwrap();
    assert_eq!(f.scalar("SELECT COUNT(*) FROM attempts"), 1);
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
}

#[tokio::test]
async fn targeted_stop_preserves_other_inflight_work_and_blocks_late_result() {
    let f = Fixture::new().await;
    f.fake
        .completions(vec![
            delayed(Duration::from_millis(500)),
            delayed(Duration::from_millis(500)),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@all stop only one")).await;
    f.wait(|| f.scalar("SELECT COUNT(*) FROM attempts") == 2)
        .await;
    assert!(matches!(
        room.submit(Request::Stop {
            request_id: EventId::new(),
            target: Some("member0".parse().unwrap())
        })
        .await,
        RequestResult::Accepted { .. }
    ));
    f.finals(1).await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE target='member0' AND status='cancelled'"),
        1
    );
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM work WHERE target='member1' AND status='completed'"),
        1
    );
    room.shutdown().await.unwrap();
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM events WHERE kind='message.final' AND sender='member0'"),
        0
    );
}

#[tokio::test]
async fn credentials_and_provider_reasoning_never_reach_feed_export_debug_or_sqlite() {
    let f = Fixture::new().await;
    f.fake
        .completions(vec![streaming("public answer", "stop")])
        .await;
    assert!(!format!("{:?}", f.config).contains(KEY));
    let client = squad_agents::openrouter::OpenRouterClient::new(&f.fake.base_url(), KEY).unwrap();
    assert!(!format!("{client:?}").contains(KEY));
    let room = f.open(OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    room.submit(f.request("@member0 privacy check")).await;
    f.finals(1).await;
    let mut observed = format!("{:?}", room.state().await);
    while let Ok(item) = feed.try_recv() {
        observed.push_str(&format!("{item:?}"));
    }
    observed.push_str(&room.export(ExportScope::Room, None).await.unwrap());
    for forbidden in [KEY, "PRIVATE REASONING", "PRIVATE DETAILS"] {
        assert!(!observed.contains(forbidden));
    }
    for request in f.fake.server.received_requests().await.unwrap() {
        assert!(
            request
                .url
                .host_str()
                .is_some_and(|h| h == "127.0.0.1" || h == "localhost")
        );
        assert!(!String::from_utf8_lossy(&request.body).contains(KEY));
    }
    // Pin SQLite's sidecar lifetime while scanning every persisted file. The
    // writer closes asynchronously, so directory enumeration alone races WAL cleanup.
    let scan_guard = f.db();
    scan_guard
        .execute_batch("BEGIN; SELECT COUNT(*) FROM events;")
        .unwrap();
    room.shutdown().await.unwrap();
    for entry in std::fs::read_dir(&f.config.data_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(path).unwrap();
            for forbidden in [KEY, "PRIVATE REASONING", "PRIVATE DETAILS"] {
                assert!(
                    !bytes
                        .windows(forbidden.len())
                        .any(|w| w == forbidden.as_bytes())
                );
            }
        }
    }
}

#[tokio::test]
async fn ordinary_missing_cost_must_retain_a_positive_priced_reservation() {
    let f = Fixture::new().await;
    f.fake.completions(vec![missing_cost()]).await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 retain uncertain charge"))
        .await;
    f.finals(1).await;
    let state = room.state().await;
    room.shutdown().await.unwrap();
    assert!(state.daily_spend.reserved_usd > 0.);
}
#[tokio::test]
async fn delayed_failed_receipt_releases_its_hold_without_another_completion() {
    let f = Fixture::new().await;
    let delayed_id = "gen-late-failed-charge";
    f.fake
        .completions(vec![
            raw_sse(event(serde_json::json!({
                "id": delayed_id, "model": "fixture/member0",
                "choices": [{"delta": {"content": "partial"}, "finish_reason": null}]
            }))),
            streaming("replacement", "stop"),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 recover a disconnected call"))
        .await;
    f.finals(1).await;
    assert_eq!(f.scalar("SELECT SUM(unknown_calls) FROM spend_daily"), 1);
    assert!(f.scalar("SELECT SUM(reserved_micros) FROM spend_daily") > 0);
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    f.fake
        .generation(
            delayed_id,
            serde_json::json!({
                "tokens_prompt": 11, "tokens_completion": 17, "total_cost": 0.014
            }),
        )
        .await;
    f.wait(|| f.scalar("SELECT SUM(unknown_calls) FROM spend_daily") == 0)
        .await;
    assert_eq!(f.scalar("SELECT SUM(reserved_micros) FROM spend_daily"), 0);
    assert_eq!(
        f.scalar("SELECT SUM(spent_micros) FROM spend_daily"),
        14_045
    );
    assert_eq!(
        f.scalar(
            "SELECT COUNT(*) FROM attempts WHERE outcome='failed' AND reported_cost_micros=14000"
        ),
        1
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    room.shutdown().await.unwrap();
    assert_eq!(
        f.scalar("SELECT SUM(spent_micros) FROM spend_daily"),
        14_045
    );
}

#[tokio::test]
async fn delayed_receipt_survives_clear_and_reopen_with_its_original_attempt() {
    let f = Fixture::new().await;
    let delayed_id = "gen-late-cleared-charge";
    f.fake
        .completions(vec![
            raw_sse(event(serde_json::json!({
                "id": delayed_id, "model": "fixture/member0",
                "choices": [{"delta": {"content": "partial"}, "finish_reason": null}]
            }))),
            streaming("replacement", "stop"),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 keep the original billing identity"))
        .await;
    f.finals(1).await;
    assert_eq!(f.scalar("SELECT SUM(unknown_calls) FROM spend_daily"), 1);
    let cleared = room
        .submit(Request::Clear {
            request_id: EventId::new(),
        })
        .await;
    assert!(matches!(cleared, RequestResult::Accepted { .. }));
    room.shutdown().await.unwrap();
    drop(room);
    assert_eq!(f.scalar("SELECT COUNT(*) FROM work"), 0);
    assert_eq!(f.scalar("SELECT SUM(unknown_calls) FROM spend_daily"), 1);
    f.fake
        .generation(
            delayed_id,
            serde_json::json!({
                "tokens_prompt": 11, "tokens_completion": 17, "total_cost": 0.014
            }),
        )
        .await;
    let reopened = f.open(OpenMode::OneShot).await;
    f.wait(|| f.scalar("SELECT SUM(unknown_calls) FROM spend_daily") == 0)
        .await;
    assert_eq!(f.scalar("SELECT SUM(reserved_micros) FROM spend_daily"), 0);
    assert_eq!(
        f.scalar("SELECT SUM(spent_micros) FROM spend_daily"),
        14_045
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    reopened.shutdown().await.unwrap();
    assert_eq!(
        f.scalar("SELECT SUM(spent_micros) FROM spend_daily"),
        14_045
    );
}

#[tokio::test]
async fn work_receipt_sweeper_preserves_startup_probe_ownership() {
    let mut f = Fixture::new().await;
    f.config.agents[0].model = "~fixture/member0-alias".into();
    f.fake.completions(vec![streaming("healthy", "stop")]).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_partial_json(
            serde_json::json!({"model":"~fixture/member0-alias"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"gen-unpriced-startup", "model":"fixture/member0",
            "choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":11,"completion_tokens":17}
        })))
        .with_priority(1)
        .expect(1)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM attempts WHERE context_id='probe:member0' AND unresolved=1"),
        1
    );
    let result = room
        .submit(f.request("@member1 continue ordinary work"))
        .await;
    assert!(matches!(result, RequestResult::Accepted { .. }));
    f.finals(1).await;
    room.shutdown().await.unwrap();
    assert_eq!(
        f.scalar("SELECT COUNT(*) FROM attempts WHERE context_id='probe:member0' AND unresolved=1"),
        1
    );
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
}
