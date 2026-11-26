//! Real room/executor streaming soak with an offline provider, never real credentials.
#![cfg(target_os = "linux")]
#[path = "../../squad-core/tests/support/m6_room.rs"]
mod support;
use squad_protocol::*;
use std::time::{Duration, Instant};
use support::{fake_openrouter::*, *};
use wiremock::{
    Mock,
    matchers::{method, path},
};

// Kernel high-water RSS covers allocations between wave samples as well.
fn high_water_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn soak_response() -> wiremock::ResponseTemplate {
    // Eight SSE deltas carry a 3,712-byte answer. The ordinary character-at-a-time
    // fixture is deliberately adversarial; the initial debug soak timed out
    // during concurrent compilation. Retain that failed run without attributing it.
    let mut body = String::new();
    for _ in 0..8 {
        body.push_str(&event(serde_json::json!({"choices":[{"index":0,
            "delta":{"content":"Streaming fixture paragraph. ".repeat(16)},
            "finish_reason":null}]})));
    }
    body.push_str(&event(
        serde_json::json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
    ));
    body.push_str(&event(serde_json::json!({"choices":[],"usage":usage()})));
    body.push_str("data: [DONE]\r\n\r\n");
    raw_sse(body)
}

#[tokio::test]
#[ignore = "manual 30-minute streaming RSS soak: run alone with --ignored --nocapture"]
async fn six_and_twelve_agent_streaming_for_thirty_minutes() {
    let mut rooms = Vec::new();
    for workers in [6, 12] {
        let mut fixture = Fixture::with_roster(workers).await;
        // The provider fixture must not retain every expanding prompt during a
        // memory measurement; durable application history remains enabled.
        fixture.fake.server = wiremock::MockServer::builder()
            .disable_request_recording()
            .start()
            .await;
        let models = fixture
            .config
            .agents
            .iter()
            .map(|agent| agent.model.clone())
            .chain(std::iter::once(fixture.config.judge.model.clone()))
            .map(|id| {
                serde_json::json!({"id":id,"context_length":1000000,
                "top_provider":{"max_completion_tokens":65536},
                "supported_parameters":["reasoning"],
                "pricing":{"prompt":"0.000001","completion":"0.000002"}})
            })
            .collect::<Vec<_>>();
        fixture
            .fake
            .catalog(serde_json::json!({"data":models}))
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(soak_response().set_delay(Duration::from_millis(100)))
            .mount(&fixture.fake.server)
            .await;
        let room = fixture.open(OpenMode::Interactive).await;
        rooms.push((workers, fixture, room));
    }
    let start = Instant::now();
    let mut waves = 0;
    let mut peak = high_water_kib();
    while start.elapsed() < Duration::from_secs(1800) {
        for (_, fixture, room) in &rooms {
            // wiremock also retains requests per mounted mock even with global
            // recording disabled. Reset between waves to measure the runtime,
            // while retaining its full durable conversation history.
            fixture.fake.server.reset().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat/completions"))
                .respond_with(soak_response().set_delay(Duration::from_millis(100)))
                .mount(&fixture.fake.server)
                .await;
            assert!(matches!(
                room.submit(fixture.request("@all exercise streaming memory"))
                    .await,
                RequestResult::Accepted { .. }
            ));
        }
        waves += 1;
        for (workers, fixture, _) in &rooms {
            tokio::time::timeout(Duration::from_secs(60), async {
                while fixture.scalar("SELECT COUNT(*) FROM events WHERE kind='message.final'")
                    != (waves * workers) as i64
                {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .expect("streaming wave failed to complete");
        }
        peak = peak.max(high_water_kib());
        assert!(
            peak <= 512 * 1024,
            "combined six- and twelve-agent process RSS exceeds 512 MiB"
        );
        if waves % 6 == 0 {
            println!(
                "M6 soak elapsed_s={} waves={waves} combined_process_peak_rss_kib={peak}",
                start.elapsed().as_secs()
            );
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    for (_, _, room) in rooms {
        room.shutdown().await.unwrap();
    }
    println!(
        "M6 soak PASS elapsed_s={} rosters=6,12 waves={waves} combined_process_peak_rss_kib={peak}",
        start.elapsed().as_secs()
    );
}
