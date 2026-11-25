//! Short deterministic measurements; these do not replace the 30-minute TUI gate.
#[path = "../../squad-core/tests/support/m6_room.rs"]
mod support;
use squad_protocol::*;
use std::time::{Duration, Instant};
use support::{fake_openrouter::*, *};
use wiremock::{
    Mock,
    matchers::{method, path},
};

#[cfg(target_os = "linux")]
fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|s| s.split_whitespace().next())
                .map(|s| s.parse().unwrap())
        })
        .unwrap()
}
#[cfg(not(target_os = "linux"))]
fn rss_kib() -> u64 {
    0
}

#[tokio::test]
#[ignore = "manual performance gate: six-worker first-delta p95 currently exceeds 500 ms"]
async fn deterministic_first_delta_and_replay_measurements_six_and_twelve_workers() {
    let mut within_latency_target = true;
    for workers in [6, 12] {
        let f = Fixture::with_roster(workers).await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat/completions"))
            .respond_with(
                streaming("A deterministic reply", "stop").set_delay(Duration::from_millis(100)),
            )
            .mount(&f.fake.server)
            .await;
        let room = f.open(OpenMode::Interactive).await;
        let mut feed = room.subscribe();
        let mut first = Vec::new();
        let mut acceptance = Vec::new();
        let mut peak = rss_kib();
        for n in 0..105 {
            while feed.try_recv().is_ok() {}
            let started = Instant::now();
            assert!(matches!(
                room.submit(f.request("@all measure application overhead"))
                    .await,
                RequestResult::Accepted { .. }
            ));
            let accepted = started.elapsed();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if matches!(feed.recv().await.unwrap(), FeedItem::Delta(_)) {
                        break;
                    }
                }
            })
            .await
            .unwrap();
            let elapsed = started.elapsed();
            f.finals(((n + 1) * workers) as i64).await;
            if n >= 5 {
                first.push(elapsed);
                acceptance.push(accepted);
            }
            peak = peak.max(rss_kib());
        }
        first.sort();
        acceptance.sort();
        let replay = Instant::now();
        let page = room.page_events(None, 100).await.unwrap();
        let replay = replay.elapsed();
        assert!(page.len() >= 100);
        println!(
            "M6 workers={workers} samples=100 fake_provider_delay_ms=100 accept_p95_us={} first_delta_p95_us={} replay_100_us={} peak_rss_kib={peak}",
            acceptance[94].as_micros(),
            first[94].as_micros(),
            replay.as_micros()
        );
        within_latency_target &= first[94] <= Duration::from_millis(500);
        assert!(replay <= Duration::from_millis(500));
        assert!(peak <= 512 * 1024);
        room.shutdown().await.unwrap();
    }
    assert!(
        within_latency_target,
        "deterministic first-delta p95 exceeds 500 ms; see measurements above"
    );
}
