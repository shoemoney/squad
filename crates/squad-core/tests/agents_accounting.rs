mod support;
use squad_protocol::*;
use support::*;

#[tokio::test]
async fn atomic_pending_to_actual_and_corrections_are_idempotent_after_cancel_and_clear() {
    let f = Fixture::new(2, 20_000_000).await;
    let id = f.send("@member0 paid call").await[0];
    f.store
        .call(move |db| {
            squad_store::accounting::reserve(
                db,
                "reservation",
                Some(id),
                &chrono::Local::now().format("%Y-%m-%d").to_string(),
                100,
                20_000_000,
            )
        })
        .unwrap();
    let grant = f.claim(id, ExecutorKey::Member(nick("member0"))).await;
    let submit = f.bus.executor_submit(grant.owner.clone());
    let send = |record| ExecutorRequest::Account {
        fence: grant.fence,
        epoch: grant.epoch,
        record,
    };
    let unknown = account(&grant, None);
    assert!(matches!(
        submit.submit(send(unknown)).await,
        RequestResult::Accepted { .. }
    ));
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let read_day = |day: String| {
        f.store
            .call(move |db| squad_store::accounting::daily(db, &day))
            .unwrap()
    };
    let pending = read_day(day.clone());
    assert_eq!(
        (
            pending.spent_micros,
            pending.reserved_micros,
            pending.unknown_calls
        ),
        (0, 100, 1)
    );
    f.bus
        .send(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    let known = account(&grant, Some(0.000045));
    for _ in 0..3 {
        assert!(matches!(
            submit.submit(send(known.clone())).await,
            RequestResult::Accepted { .. }
        ));
    }
    let actual = read_day(day.clone());
    assert_eq!(
        (
            actual.spent_micros,
            actual.reserved_micros,
            actual.unknown_calls
        ),
        (45, 0, 0)
    );
    let events = f
        .store
        .call(|db| squad_store::events::page(db, None, usize::MAX))
        .unwrap();
    for event in events.iter().filter(|e| matches!(&e.content, EnvelopePayload::UsageRecord(r) if r.usage.as_ref().is_some_and(|u|u.cost_usd.is_some()))) {
        assert_eq!(event.commit.commit_size,2);
        let budget = events.iter().find(|e| e.commit.seq.get() == event.commit.seq.get()+1).unwrap();
        assert!(matches!(&budget.content,EnvelopePayload::BudgetState(b) if b.spent_usd == 0.000045));
    }
    f.bus
        .send(Request::Clear {
            request_id: EventId::new(),
        })
        .await;
    let correction = account(&grant, Some(0.000060));
    for _ in 0..2 {
        assert!(matches!(
            submit.submit(send(correction.clone())).await,
            RequestResult::Accepted { .. }
        ));
    }
    assert_eq!(read_day(day).spent_micros, 60);
    let wrong_owner = f.bus.executor_submit(ExecutorKey::Member(nick("member1")));
    assert!(matches!(
        wrong_owner.submit(send(correction.clone())).await,
        RequestResult::Rejected { .. }
    ));
    assert_eq!(f.counts().2, 60);
}

#[tokio::test]
async fn wrong_epoch_invalid_cost_and_missing_start_cannot_mutate_ledger() {
    let f = Fixture::new(2, 20_000_000).await;
    let id = f.send("@member0 paid call").await[0];
    let grant = f.claim(id, ExecutorKey::Member(nick("member0"))).await;
    let submit = f.bus.executor_submit(grant.owner.clone());
    let mut record = account(&grant, Some(0.000045));
    for epoch in [0, 99] {
        assert!(matches!(
            submit
                .submit(ExecutorRequest::Account {
                    fence: grant.fence,
                    epoch,
                    record: record.clone()
                })
                .await,
            RequestResult::Rejected { .. }
        ));
    }
    record.usage.as_mut().unwrap().cost_usd = Some(-1.0);
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Account {
                fence: grant.fence,
                epoch: grant.epoch,
                record
            })
            .await,
        RequestResult::Rejected { .. }
    ));
    assert_eq!(f.counts(), (0, 1, 0));
    assert_eq!(
        f.store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM events WHERE kind='usage.record'",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn local_day_rollover_shared_totals_and_raised_limit_use_the_configured_message() {
    let mut f = Fixture::new(2, 20_000_000).await;
    let id = f.send("@member0 rollover").await[0];
    let old_day = (chrono::Local::now().date_naive() - chrono::Duration::days(1)).to_string();
    let old = old_day.clone();
    f.store
        .call(move |db| {
            squad_store::accounting::reserve(db, "rollover", Some(id), &old, 100, 20_000_000)
        })
        .unwrap();
    let grant = f.claim(id, ExecutorKey::Member(nick("member0"))).await;
    let submit = f.bus.executor_submit(grant.owner.clone());
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Account {
                fence: grant.fence,
                epoch: grant.epoch,
                record: account(&grant, Some(20.0))
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    assert!(matches!(
        submit.submit(reply(&grant)).await,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(
        f.store
            .call(move |db| squad_store::accounting::daily(db, &old_day))
            .unwrap()
            .reserved_micros,
        0
    );
    let refused = f.send("@member1 limit").await[0];
    let submit = f.bus.executor_submit(ExecutorKey::Member(nick("member1")));
    let RequestResult::Internal(InternalResult::Claimed(claim)) = submit
        .submit(ExecutorRequest::Claim { work_id: refused })
        .await
    else {
        panic!("claim")
    };
    let RequestResult::Rejected { error, .. } = submit
        .submit(ExecutorRequest::StartAttempt {
            input_tokens: None,
            work_id: refused,
            attempt: claim.work.attempt,
            fence: claim.fence,
        })
        .await
    else {
        panic!("refused")
    };
    assert_eq!(error.code, ErrorCode::DAILY_MAX_REACHED);
    assert_eq!(
        error.text.as_str(),
        "Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow."
    );
    let bindings = f
        .config
        .agents
        .iter()
        .map(|a| {
            (
                a.nick.to_string(),
                ExecutorBinding {
                    model: Text::new(fake_openrouter::MODEL).unwrap(),
                    effort: ReasoningEffort::Medium,
                },
            )
        })
        .chain(std::iter::once((
            "judge".into(),
            ExecutorBinding {
                model: Text::new(fake_openrouter::MODEL).unwrap(),
                effort: ReasoningEffort::High,
            },
        )))
        .collect();
    f.bus = squad_core::bus::InProcessBus::start(
        squad_core::router::Router::new(
            f.store.clone(),
            f.config.agents.iter().map(|a| a.nick.clone()).collect(),
            bindings,
            30_000_000,
        )
        .unwrap(),
    );
    let id = f.send("@member1 raised").await[0];
    let grant = f.claim(id, ExecutorKey::Member(nick("member1"))).await;
    let result = f
        .bus
        .executor_submit(grant.owner.clone())
        .submit(ExecutorRequest::Account {
            fence: grant.fence,
            epoch: grant.epoch,
            record: account(&grant, Some(1.0)),
        })
        .await;
    assert!(matches!(result, RequestResult::Accepted { .. }));
    assert_eq!(f.counts().2, 21_000_000);
}

#[tokio::test]
async fn canonical_plan_reservation_splits_per_attempt_and_unknown_calls_survive_plan_release() {
    let f = Fixture::new(2, 20_000_000).await;
    let (id, case) = f.round_case();
    let plan = PlanId::new();
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let reserve_day = day.clone();
    f.store.call(move |db| {
        let tx=db.transaction()?;
        let (work,_)=squad_store::work::template(&tx,id)?.unwrap();
        squad_store::accounting::reserve(&tx,&format!("plan:{plan}"),None,&reserve_day,200,20_000_000)?;
        tx.execute("UPDATE reservations SET contest_id=?,plan_id=? WHERE reservation_id=?",rusqlite::params![work.contest_id.unwrap().to_string(),plan.to_string(),format!("plan:{plan}")])?;
        tx.execute("UPDATE work SET scheduling_meta=json_set(scheduling_meta,'$.planId',?,'$.attemptReservationMicros',100) WHERE work_id=?",rusqlite::params![plan.to_string(),id.to_string()])?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    let grant = f.claim(id, ExecutorKey::Case(case)).await;
    let submit = f.bus.executor_submit(grant.owner.clone());
    let read_day = day.clone();
    let spend = f
        .store
        .call(move |db| squad_store::accounting::daily(db, &read_day))
        .unwrap();
    assert_eq!((spend.reserved_micros, spend.unknown_calls), (200, 1));
    // Terminal engine release removes only the canonical unstarted remainder.
    f.store.call(move |db| {
        let tx=db.transaction()?;
        tx.execute("UPDATE spend_daily SET reserved_micros=reserved_micros-(SELECT amount_micros FROM reservations WHERE reservation_id=?)",[format!("plan:{plan}")])?;
        tx.execute("DELETE FROM reservations WHERE reservation_id=?",[format!("plan:{plan}")])?;
        tx.commit()?;
        Ok(())
    }).unwrap();
    let unresolved = f
        .store
        .call(move |db| squad_store::accounting::daily(db, &day))
        .unwrap();
    assert_eq!(
        (unresolved.reserved_micros, unresolved.unknown_calls),
        (100, 1)
    );
    let record = account(&grant, Some(0.000045));
    for _ in 0..2 {
        assert!(matches!(
            submit
                .submit(ExecutorRequest::Account {
                    fence: grant.fence,
                    epoch: grant.epoch,
                    record: record.clone()
                })
                .await,
            RequestResult::Accepted { .. }
        ));
    }
    let day = record.day;
    let actual = f
        .store
        .call(move |db| squad_store::accounting::daily(db, &day))
        .unwrap();
    assert_eq!(
        (
            actual.spent_micros,
            actual.reserved_micros,
            actual.unknown_calls
        ),
        (45, 0, 0)
    );
}

#[tokio::test]
async fn missing_plan_reservation_rejects_dispatch_without_leaving_running_work() {
    let f = Fixture::new(2, 20_000_000).await;
    let (id, case) = f.round_case();
    f.store.call(move |db|{
        db.execute("UPDATE work SET scheduling_meta=json_set(scheduling_meta,'$.planId',?,'$.attemptReservationMicros',100) WHERE work_id=?",rusqlite::params![PlanId::new().to_string(),id.to_string()])?;
        Ok(())
    }).unwrap();
    let submit = f.bus.executor_submit(ExecutorKey::Case(case));
    let RequestResult::Internal(InternalResult::Claimed(grant)) =
        submit.submit(ExecutorRequest::Claim { work_id: id }).await
    else {
        panic!("claim")
    };
    assert!(matches!(
        submit
            .submit(ExecutorRequest::StartAttempt {
                input_tokens: None,
                work_id: id,
                attempt: grant.work.attempt,
                fence: grant.fence
            })
            .await,
        RequestResult::Rejected { .. }
    ));
    assert_eq!(f.status(id), WorkStatus::Failed);
    assert_eq!(f.counts().1, 0);
}
