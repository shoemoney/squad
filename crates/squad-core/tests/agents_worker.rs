mod support;
use squad_protocol::*;
use std::time::Duration;
use support::{fake_openrouter::*, *};

#[tokio::test]
async fn one_shot_is_explicit_and_ignores_dormant_fifo() {
    let f = Fixture::new(2, 20_000_000).await;
    let old = f.send("@member0 dormant").await[0];
    f.fake
        .completions(vec![streaming("new answer", "stop")])
        .await;
    let mut runtime = f.start(OpenMode::OneShot);
    runtime.ready().await.unwrap();
    let new = f.send("@member0 new request").await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(f.fake.captured_bodies().await.is_empty());
    runtime.wake(&new);
    f.until(|| f.counts().0 == 1).await;
    let status = f
        .store
        .call(move |db| Ok(squad_store::work::record(db, old)?.unwrap().status))
        .unwrap();
    assert_eq!(status, WorkStatus::Queued);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancellation_and_acceptance_are_serialized_and_old_attempts_rejected() {
    let f = Fixture::new(2, 20_000_000).await;
    let ids = f.send("@member0 first").await;
    let owner = ExecutorKey::Member(nick("member0"));
    let grant = f.claim(ids[0], owner.clone()).await;
    assert!(matches!(
        f.bus
            .send(Request::Stop {
                request_id: EventId::new(),
                target: None
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    let submit = f.bus.executor_submit(owner.clone());
    assert!(
        matches!(submit.submit(reply(&grant)).await, RequestResult::Rejected { error, .. } if error.code == ErrorCode::STALE_WORK_ATTEMPT)
    );
    let next = f.send("@member0 second").await;
    let grant = f.claim(next[0], owner).await;
    assert!(matches!(
        submit.submit(reply(&grant)).await,
        RequestResult::Accepted { .. }
    ));
    assert!(matches!(
        submit.submit(reply(&grant)).await,
        RequestResult::Accepted { .. }
    ));
    f.bus
        .send(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    assert_eq!(f.counts().0, 1);
}

#[tokio::test]
async fn daily_limit_refuses_queued_calls_and_dispatched_calls_finish() {
    let f = Fixture::new(2, 45).await;
    f.fake
        .completions(vec![
            streaming("first", "stop"),
            delayed(Duration::from_millis(300)),
        ])
        .await;
    let ids = f.send("@member0 first").await;
    let owner = ExecutorKey::Member(nick("member0"));
    let first = f.claim(ids[0], owner.clone()).await;
    let second = f.send("@member1 dispatched").await;
    let second = f
        .claim(second[0], ExecutorKey::Member(nick("member1")))
        .await;
    let submit = f.bus.executor_submit(owner.clone());
    let record = account(&first, Some(0.000045));
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Account {
                fence: first.fence,
                epoch: first.epoch,
                record
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    assert!(matches!(
        submit.submit(reply(&first)).await,
        RequestResult::Accepted { .. }
    ));
    assert!(matches!(
        f.bus
            .executor_submit(second.owner.clone())
            .submit(reply(&second))
            .await,
        RequestResult::Accepted { .. }
    ));
    f.send("@member0 queued refusal").await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| {
        f.store
            .call(|db| {
                Ok(
                    db.query_row("SELECT COUNT(*) FROM work WHERE status='failed'", [], |r| {
                        r.get::<_, i64>(0)
                    })?,
                )
            })
            .unwrap()
            == 1
    })
    .await;
    assert_eq!(f.counts().1, 2);
    assert!(f.fake.captured_bodies().await.is_empty());
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn restart_consumes_interrupted_attempt_rejects_stale_grant_and_never_duplicates_replies() {
    let f = Fixture::new(2, 20_000_000).await;
    let id = f.send("@member0 interrupted").await[0];
    let owner = ExecutorKey::Member(nick("member0"));
    let interrupted = f.claim(id, owner.clone()).await;
    // Controlled recovery fixture: recovery is engine-owned, outside the runtime.
    // The original provider start is retained as unknown; its attempt is consumed.
    f.store.call(move |db| {
        let tx = db.transaction()?;
        tx.execute("UPDATE room_meta SET epoch=epoch+1",[])?;
        tx.execute("UPDATE work SET epoch=(SELECT epoch FROM room_meta),status='queued',lease_fence=lease_fence+1,lease_owner=NULL,lease_until=NULL",[])?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    let stale = f.bus.executor_submit(owner);
    assert!(
        matches!(stale.submit(reply(&interrupted)).await,RequestResult::Rejected { error,.. } if error.code == ErrorCode::STALE_WORK_ATTEMPT)
    );
    f.fake
        .completions(vec![streaming("recovered", "stop")])
        .await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.counts().0 == 1).await;
    runtime.shutdown().await.unwrap();
    assert_eq!(f.counts(), (1, 2, 45));
    let record = f
        .store
        .call(move |db| Ok(squad_store::work::record(db, id)?.unwrap()))
        .unwrap();
    assert_eq!(record.attempt, 2);
    let runtime = f.start(OpenMode::Interactive);
    tokio::time::sleep(Duration::from_millis(150)).await;
    runtime.shutdown().await.unwrap();
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    let record = account(&interrupted, Some(0.000025));
    assert!(matches!(
        stale
            .submit(ExecutorRequest::Account {
                fence: interrupted.fence,
                epoch: interrupted.epoch,
                record
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(f.counts().2, 70);
}

#[tokio::test]
async fn ordinary_retry_uses_a_new_started_attempt_and_discards_partial_text() {
    let f = Fixture::new(2, 20_000_000).await;
    f.fake
        .completions(vec![disconnected(), streaming("replacement", "stop")])
        .await;
    f.fake
        .generation(
            GENERATION,
            serde_json::json!({"tokens_prompt":11,"tokens_completion":17,"total_cost":0.000025}),
        )
        .await;
    f.send("@member0 retry").await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.counts().0 == 1).await;
    let events = f
        .store
        .call(|db| squad_store::events::page(db, None, usize::MAX))
        .unwrap();
    let reply = events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::MessageFinal(r) = &e.content {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(reply.content, "replacement");
    assert_eq!(reply.completed.generation.attempt, 2);
    assert_eq!(f.counts(), (1, 2, 70));
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn full_capacity_one_mailbox_never_blocks_bounded_ingress_or_loses_durable_work() {
    let f = Fixture::new(2, 20_000_000).await;
    let mut mailbox = f.bus.register(ExecutorKey::Member(nick("member0")));
    f.send("@member0 missed wake").await;
    // The durable acknowledgement may precede its best-effort wake delivery.
    tokio::time::timeout(Duration::from_secs(5), async {
        while mailbox.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(mailbox.len(), 1);
    let mut pending = Vec::new();
    for _ in 0..constants::INGRESS_CAPACITY + 10 {
        let bus = f.bus.clone();
        pending.push(tokio::spawn(async move {
            bus.send(Request::SetVars {
                request_id: EventId::new(),
                updates: vec![VariableUpdate {
                    name: VariableName::Rounds,
                    value: 3,
                }],
            })
            .await
        }));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        for task in pending {
            assert!(matches!(
                task.await.unwrap(),
                RequestResult::Accepted { .. }
            ));
        }
    })
    .await
    .unwrap();
    assert_eq!(mailbox.len(), 1);
    mailbox.recv().await.unwrap();
    f.fake
        .completions(vec![streaming("durable recovery", "stop")])
        .await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.counts().0 == 1).await;
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn dispatch_rechecks_outstanding_reservations_against_current_limit() {
    let f = Fixture::new(2, 50).await;
    let id = f.send("@member0 reserved before the limit changed").await[0];
    f.store
        .call(move |db| {
            squad_store::accounting::reserve(
                db,
                "over-current-limit",
                Some(id),
                &chrono::Local::now().format("%Y-%m-%d").to_string(),
                100,
                20_000_000,
            )
        })
        .unwrap();
    let submit = f.bus.executor_submit(ExecutorKey::Member(nick("member0")));
    let RequestResult::Internal(InternalResult::Claimed(grant)) =
        submit.submit(ExecutorRequest::Claim { work_id: id }).await
    else {
        panic!("claim")
    };
    assert!(
        matches!(submit.submit(ExecutorRequest::StartAttempt{ input_tokens: None,work_id:id,attempt:grant.work.attempt,fence:grant.fence}).await,RequestResult::Rejected{error,..} if error.code==ErrorCode::DAILY_MAX_REACHED)
    );
    assert_eq!(f.counts().1, 0);
    assert_eq!(f.status(id), WorkStatus::Failed);
    let reserved = f
        .store
        .call(|db| {
            Ok(db.query_row(
                "SELECT COALESCE(SUM(reserved_micros),0) FROM spend_daily",
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(reserved, 0);
}

#[tokio::test]
async fn recovered_work_keeps_its_concrete_binding_after_alias_resolution_changes() {
    let mut f = Fixture::new(2, 20_000_000).await;
    f.send("@member0 frozen model").await;
    let mut changed = f.snapshot.members[0].model.clone().unwrap();
    changed.id = "fixture/new-alias-target".into();
    f.snapshot.members[0].model = Some(changed);
    f.fake
        .completions(vec![streaming("original model", "stop")])
        .await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.counts().0 == 1).await;
    let requests = f.fake.captured_bodies().await;
    assert_eq!(requests[0]["model"], MODEL);
    assert!(
        requests[0]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains(MODEL)
    );
    runtime.shutdown().await.unwrap();
}
