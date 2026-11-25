mod support;
use squad_protocol::*;
use support::*;

async fn priced(max: i64) -> Fixture {
    let f = Fixture::new(2, max).await;
    f.store.call(|db| {
        db.execute(r#"UPDATE catalog SET pricing_json='{"prompt":"0.000001","completion":"0.000002"}'"#, [])?;
        Ok(())
    }).unwrap();
    f
}
fn totals(f: &Fixture) -> (i64, i64, u32) {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let d = f
        .store
        .call(move |db| squad_store::accounting::daily(db, &day))
        .unwrap();
    (d.spent_micros, d.reserved_micros, d.unknown_calls)
}
async fn try_start(f: &Fixture, id: WorkId) -> RequestResult {
    let submit = f.bus.executor_submit(ExecutorKey::Member(nick("member0")));
    let RequestResult::Internal(InternalResult::Claimed(g)) =
        submit.submit(ExecutorRequest::Claim { work_id: id }).await
    else {
        panic!("claim")
    };
    submit
        .submit(ExecutorRequest::StartAttempt {
            work_id: id,
            attempt: g.work.attempt,
            fence: g.fence,
        })
        .await
}
#[tokio::test]
async fn cap_refuses_all_permitted_attempts_atomically_before_provider_start() {
    // One call fits, but both permitted attempts do not.
    let f = priced(200_000).await;
    let id = f.send("@member0 refused").await[0];
    assert!(
        matches!(try_start(&f,id).await, RequestResult::Rejected {error,..} if error.code==ErrorCode::DAILY_MAX_REACHED)
    );
    assert_eq!(totals(&f), (0, 0, 0));
    assert_eq!(f.counts().1, 0);
    assert!(f.fake.captured_bodies().await.is_empty());
}
#[tokio::test]
async fn unavailable_prices_refuse_dispatch_and_known_free_prices_are_allowed() {
    let f = priced(20_000_000).await;
    f.store
        .call(|db| {
            db.execute("UPDATE catalog SET pricing_json='{}'", [])?;
            Ok(())
        })
        .unwrap();
    let id = f.send("@member0 unavailable").await[0];
    assert!(
        matches!(try_start(&f,id).await, RequestResult::Rejected {error,..} if error.code==ErrorCode::COST_UNKNOWN)
    );
    assert_eq!(totals(&f), (0, 0, 0));
    assert_eq!(f.counts().1, 0);
    f.store
        .call(|db| {
            db.execute(
                r#"UPDATE catalog SET pricing_json='{"prompt":0,"completion":0}'"#,
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let id = f.send("@member0 free").await[0];
    assert!(matches!(
        try_start(&f, id).await,
        RequestResult::Internal(InternalResult::Started(_))
    ));
}
#[tokio::test]
async fn success_settles_once_and_releases_unused_retry() {
    let f = priced(20_000_000).await;
    let id = f.send("@member0 paid").await[0];
    let g = f.claim(id, ExecutorKey::Member(nick("member0"))).await;
    let reserved = totals(&f).1;
    assert!(reserved > 0);
    let submit = f.bus.executor_submit(g.owner.clone());
    // Duplicate starts must not create more reservations or unknown calls.
    for _ in 0..2 {
        assert!(matches!(
            submit
                .submit(ExecutorRequest::StartAttempt {
                    work_id: id,
                    attempt: g.work.attempt,
                    fence: g.fence
                })
                .await,
            RequestResult::Internal(InternalResult::Started(_))
        ));
        assert_eq!(totals(&f), (0, reserved, 1));
    }
    for _ in 0..3 {
        assert!(matches!(
            submit
                .submit(ExecutorRequest::Account {
                    fence: g.fence,
                    epoch: g.epoch,
                    record: account(&g, Some(0.000045))
                })
                .await,
            RequestResult::Accepted { .. }
        ));
        assert_eq!(totals(&f), (45, reserved / 2, 0));
    }
    assert!(matches!(
        submit.submit(reply(&g)).await,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(totals(&f), (45, 0, 0));
}
#[tokio::test]
async fn failure_stop_and_restart_release_only_unused_calls() {
    for action in ["failure", "stop", "restart"] {
        let f = priced(20_000_000).await;
        let id = f.send("@member0 pending").await[0];
        let g = f.claim(id, ExecutorKey::Member(nick("member0"))).await;
        let reserved = totals(&f).1;
        let submit = f.bus.executor_submit(g.owner.clone());
        match action {
            "failure" => {
                let failure = ErrorPayload {
                    code: ErrorCode::PROVIDER_ERROR,
                    text: Text::new("fixture failure").unwrap(),
                    work_id: Some(id),
                    debate_id: None,
                    case_id: None,
                    contest_id: None,
                    related_event_id: None,
                    earliest_cursor: None,
                    retry_action: None,
                    retryable: false,
                };
                assert!(matches!(
                    submit
                        .submit(ExecutorRequest::Failed {
                            work_id: id,
                            attempt: g.work.attempt,
                            fence: g.fence,
                            epoch: g.epoch,
                            error: failure,
                            repair: None
                        })
                        .await,
                    RequestResult::Accepted { .. }
                ));
            }
            "stop" => {
                f.bus
                    .send(Request::Stop {
                        request_id: EventId::new(),
                        target: None,
                    })
                    .await;
            }
            _ => {
                squad_core::router::recovery::recover_on_open(&f.store).unwrap();
            }
        }
        assert_eq!(totals(&f), (0, reserved / 2, 1), "{action}");
        // The started unknown share remains payable even after fencing/recovery.
        let mut billed = account(&g, Some(0.000045));
        billed.outcome = if action == "stop" {
            UsageOutcome::Cancelled
        } else {
            UsageOutcome::Failed
        };
        for _ in 0..2 {
            assert!(matches!(
                submit
                    .submit(ExecutorRequest::Account {
                        fence: g.fence,
                        epoch: g.epoch,
                        record: billed.clone()
                    })
                    .await,
                RequestResult::Accepted { .. }
            ));
        }
        assert_eq!(totals(&f), (45, 0, 0), "{action}");
    }
}

#[tokio::test]
async fn competing_dispatches_cannot_spend_the_same_daily_capacity() {
    let f = priced(400_000).await;
    let ids = f.send("@all competing calls").await;
    let mut grants = Vec::new();
    for (id, member) in ids.into_iter().zip(["member0", "member1"]) {
        let submit = f.bus.executor_submit(ExecutorKey::Member(nick(member)));
        let RequestResult::Internal(InternalResult::Claimed(g)) =
            submit.submit(ExecutorRequest::Claim { work_id: id }).await
        else {
            panic!("claim")
        };
        grants.push((submit, g));
    }
    let start = |(submit, g): &(std::sync::Arc<dyn ExecutorSubmit>, ExecutionGrant)| {
        let submit = submit.clone();
        let g = g.clone();
        async move {
            submit
                .submit(ExecutorRequest::StartAttempt {
                    work_id: g.work.work_id,
                    attempt: g.work.attempt,
                    fence: g.fence,
                })
                .await
        }
    };
    let (a, b) = tokio::join!(start(&grants[0]), start(&grants[1]));
    let results = [a, b];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, RequestResult::Internal(InternalResult::Started(_))))
            .count(),
        1
    );
    assert_eq!(results.iter().filter(|r| matches!(r,RequestResult::Rejected {error,..} if error.code==ErrorCode::DAILY_MAX_REACHED)).count(),1);
    assert_eq!(f.counts().1, 1);
    assert!(totals(&f).1 <= 400_000);
}

#[tokio::test]
async fn rollover_moves_only_unused_capacity_and_preserves_unknown_started_share() {
    let f = priced(20_000_000).await;
    let id = f.send("@member0 rollover").await[0];
    let mut g = f.claim(id, ExecutorKey::Member(nick("member0"))).await;
    let unit = totals(&f).1 / 2;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let tomorrow = (chrono::Local::now().date_naive() + chrono::Duration::days(1)).to_string();
    g.work.attempt = 2;
    let next = tomorrow.clone();
    f.store
        .call(move |db| {
            squad_store::accounting::start_priced(db, &g, chrono::Utc::now(), &next, 20_000_000)
        })
        .unwrap();
    let a = f
        .store
        .call(move |db| squad_store::accounting::daily(db, &today))
        .unwrap();
    let b = f
        .store
        .call(move |db| squad_store::accounting::daily(db, &tomorrow))
        .unwrap();
    assert_eq!((a.reserved_micros, a.unknown_calls), (unit, 1));
    assert_eq!((b.reserved_micros, b.unknown_calls), (unit, 1));
}
