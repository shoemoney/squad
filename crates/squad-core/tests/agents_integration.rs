mod support;
use squad_protocol::*;
use std::time::Duration;
use support::{fake_openrouter::*, *};

#[tokio::test]
async fn six_and_twelve_members_stream_independently_without_foreign_history() {
    for count in [6, 12] {
        let f = Fixture::new(count, 20_000_000).await;
        f.fake
            .completions(
                (0..count * 2)
                    .map(|_| streaming("answer", "stop").set_delay(Duration::from_millis(150)))
                    .collect(),
            )
            .await;
        let mut runtime = f.start(OpenMode::Interactive);
        runtime.ready().await.unwrap();
        for i in 0..count {
            f.send(&format!("@member{i} PRIVATE_{i}_CANARY")).await;
        }
        f.until(|| f.counts().0 == count as i64).await;
        f.send(&format!(
            "{} shared question",
            (0..count)
                .map(|i| format!("@member{i}"))
                .collect::<Vec<_>>()
                .join(" ")
        ))
        .await;
        f.until(|| f.counts().0 == count as i64 * 2).await;
        assert_eq!(
            f.counts(),
            (count as i64 * 2, count as i64 * 2, count as i64 * 90)
        );
        let requests = f.fake.captured_bodies().await;
        assert_eq!(requests.len(), count * 2);
        for body in &requests {
            let system = body["messages"][0]["content"].as_str().unwrap();
            let own = (0..count)
                .find(|i| system.contains(&format!("\"PERSONA_{i}\"")))
                .unwrap();
            let serialized = body.to_string();
            for foreign in 0..count {
                if own != foreign {
                    assert!(
                        !serialized.contains(&format!("PRIVATE_{foreign}_CANARY")),
                        "own {own} foreign {foreign}: {body}"
                    );
                    assert!(!system.contains(&format!("\"PERSONA_{foreign}\"")));
                }
            }
            assert_eq!(body["model"], MODEL);
        }
        let stored = f
            .store
            .call(|db| squad_store::events::page(db, None, usize::MAX))
            .unwrap();
        let serialized = serde_json::to_string(&stored).unwrap();
        for canary in [
            "SENTINEL-PRIVATE-KEY",
            "PRIVATE REASONING",
            "PRIVATE DETAILS",
        ] {
            assert!(!serialized.contains(canary));
        }
        runtime.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn actual_inflight_stream_finishes_after_other_call_reaches_daily_max() {
    let f = Fixture::new(2, 45).await;
    f.fake
        .completions(vec![
            streaming("first", "stop").set_delay(Duration::from_millis(150)),
            streaming("second", "stop").set_delay(Duration::from_millis(500)),
        ])
        .await;
    f.send("@all parallel").await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.counts().1 == 2).await;
    f.until(|| f.counts().0 == 1).await;
    let completed_target = f
        .store
        .call(|db| {
            Ok(db.query_row(
                "SELECT target FROM work WHERE status='completed'",
                [],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    let refused = f.send(&format!("@{completed_target} next call")).await[0];
    f.until(|| f.status(refused) == WorkStatus::Failed).await;
    f.until(|| f.counts().0 == 2).await;
    assert_eq!(f.counts(), (2, 2, 90));
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn final_verdict_uses_frozen_case_bundle_and_its_own_provider_charge() {
    let f = Fixture::new(2, 20_000_000).await;
    f.fake
        .completions(vec![structured_judge(valid_verdict())])
        .await;
    let (id, case) = f.final_case();
    let debate = f
        .store
        .call(move |db| {
            let (work, _) = squad_store::work::template(db, id)?.unwrap();
            let debate = work.debate_id.unwrap();
            let state = squad_store::contests::debate(db, debate)?;
            assert_eq!(state.state, DebateStatus::Judging);
            assert!(state.judge_requested);
            assert_eq!(state.case_id, Some(case));
            let history = squad_store::history::read(db, &work.context_id, work.history_through)?;
            assert!(history.iter().any(|entry| {
                serde_json::from_str::<DebateAdjudicationBundle>(&entry.content).is_ok()
            }));
            Ok(debate)
        })
        .unwrap();
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.status(id) == WorkStatus::Completed).await;
    assert_eq!(f.counts(), (0, 1, 45));
    let captured = f.fake.captured_bodies().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0]["messages"].as_array().unwrap().len(), 2);
    assert_eq!(captured[0]["reasoning"]["max_tokens"], 8192);
    assert!(!captured[0].to_string().contains("PERSONA_"));
    let verdicts = f
        .store
        .call(|db| {
            Ok(db.query_row(
                "SELECT COUNT(*) FROM events WHERE kind='verdict'",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(verdicts, 1);
    f.store
        .call(move |db| {
            assert_eq!(
                squad_store::contests::debate(db, debate)?.state,
                DebateStatus::Completed
            );
            let sealed: bool = db.query_row(
                "SELECT decision_event_seq IS NOT NULL FROM cases WHERE case_id=?",
                [case.to_string()],
                |r| r.get(0),
            )?;
            assert!(sealed);
            Ok(())
        })
        .unwrap();
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn credentials_and_reasoning_are_absent_from_feed_database_and_enabled_logs() {
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }
    let log = Capture(Arc::new(Mutex::new(Vec::new())));
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .without_time()
            .with_writer(log.clone())
            .finish(),
    )
    .unwrap();
    let f = Fixture::new(2, 20_000_000).await;
    f.fake
        .completions(vec![streaming("visible only", "stop")])
        .await;
    let mut feed = f.bus.subscribe();
    let runtime = f.start(OpenMode::Interactive);
    f.send("@member0 prove logging").await;
    f.until(|| f.counts().0 == 1).await;
    runtime.shutdown().await.unwrap();
    let mut serialized_feed = String::new();
    while let Ok(item) = feed.try_recv() {
        serialized_feed.push_str(&serde_json::to_string(&item).unwrap());
    }
    assert!(serialized_feed.contains("visible only"));
    let logs = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("bus.commit"),
        "logger must actually capture router-thread events"
    );
    let mut artifacts = vec![serialized_feed.into_bytes(), logs.into_bytes()];
    for file in std::fs::read_dir(f.dir.path()).unwrap() {
        let file = file.unwrap();
        if file.file_type().unwrap().is_file() {
            artifacts.push(std::fs::read(file.path()).unwrap());
        }
    }
    for bytes in artifacts {
        for sentinel in [
            "SENTINEL-PRIVATE-KEY",
            "PRIVATE REASONING",
            "PRIVATE DETAILS",
        ] {
            assert!(
                !bytes
                    .windows(sentinel.len())
                    .any(|window| window == sentinel.as_bytes())
            );
        }
    }
}
