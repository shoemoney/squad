//! M6.P2.L1.T11: a running attempt keeps its 60 s work lease alive by renewing
//! every 20 s. Time is paused so the 20 s cadence elapses without waiting.
mod support;
use squad_protocol::*;
use std::time::{Duration, Instant};
use support::{fake_openrouter::*, *};

fn lease(f: &Fixture, work: WorkId) -> (String, Option<String>) {
    f.store
        .call(move |db| {
            Ok(db.query_row(
                "SELECT status, lease_until FROM work WHERE work_id=?",
                [work.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap()
}

#[test]
fn renewal_is_a_third_of_the_lease_contract() {
    assert_eq!(constants::WORK_LEASE_MS, 60_000);
    assert_eq!(constants::WORK_LEASE_RENEWAL_MS, 20_000);
    assert!(constants::WORK_LEASE_RENEWAL_MS * 3 <= constants::WORK_LEASE_MS);
}

#[tokio::test(start_paused = true)]
async fn long_running_attempt_renews_its_lease_without_losing_the_work() {
    let f = Fixture::new(2, 20_000_000).await;
    // The provider answers slowly in real time; paused virtual time races past
    // many 20 s renewal ticks while the worker waits on it.
    f.fake
        .completions(vec![delayed(Duration::from_millis(1500))])
        .await;
    let work = f.send("@member0 take your time").await[0];
    let mut runtime = f.start(OpenMode::Interactive);
    runtime.ready().await.unwrap();

    let real_deadline = Instant::now() + Duration::from_secs(20);
    // First lease observed while running.
    let first = loop {
        let (status, lease) = lease(&f, work);
        if status == "running"
            && let Some(lease) = lease
        {
            break lease;
        }
        assert!(Instant::now() < real_deadline, "work never started running");
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    // A renewal moves lease_until forward while the same attempt is running.
    let renewed = loop {
        let (status, lease) = lease(&f, work);
        if status == "running"
            && let Some(lease) = lease
            && lease != first
        {
            break lease;
        }
        assert!(
            status == "running",
            "attempt finished ({status}) before any renewal was observed"
        );
        assert!(Instant::now() < real_deadline, "lease was never renewed");
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let parse = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap();
    assert!(parse(&renewed) > parse(&first));
    assert!(
        parse(&renewed).with_timezone(&chrono::Utc) - chrono::Utc::now()
            > chrono::Duration::seconds(55)
    );
    // Renewal extended the same running attempt; it was not recovered into a new one.
    let attempt: i64 = f
        .store
        .call(move |db| {
            Ok(db.query_row(
                "SELECT attempt FROM work WHERE work_id=?",
                [work.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(attempt, 1);
    // (Virtual time also fires the provider's own timeout once the real delay
    // outlasts it, so completion is deliberately not asserted here.)
    runtime.shutdown().await.unwrap();
}
