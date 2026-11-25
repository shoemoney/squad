mod support;
use squad_protocol::*;
use std::time::Duration;
use support::{fake_openrouter::*, *};

#[tokio::test]
async fn fresh_cases_have_two_messages_no_personas_or_foreign_material_and_high_effort() {
    let f = Fixture::new(2, 20_000_000).await;
    f.fake
        .completions(vec![free_form_case("ruling A"), free_form_case("ruling B")])
        .await;
    let (a, _) = f.standalone("A_CANARY");
    let (b, _) = f.standalone("B_CANARY");
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.status(a) == WorkStatus::Completed && f.status(b) == WorkStatus::Completed)
        .await;
    assert_eq!(f.counts().2, 90);
    for body in f.fake.captured_bodies().await {
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][1]["name"], "squad");
        assert_eq!(body["reasoning"]["effort"], "high");
        let text = body.to_string();
        assert_ne!(text.contains("A_CANARY"), text.contains("B_CANARY"));
        assert!(!text.contains("PERSONA_"));
    }
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn invalid_round_repair_is_private_and_next_attempt_is_a_distinct_charge() {
    let f = Fixture::new(2, 20_000_000).await;
    f.fake
        .completions(vec![
            streaming("PRIVATE_REJECTED_OUTPUT", "stop"),
            tied_round_score("member0", "member1"),
        ])
        .await;
    let (id, case) = f.round_case();
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.status(id) == WorkStatus::Completed).await;
    assert_eq!(f.counts(), (0, 2, 90));
    let requests = f.fake.captured_bodies().await;
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].to_string().contains("PRIVATE_REJECTED_OUTPUT"));
    assert!(requests[1].to_string().contains("PRIVATE_REJECTED_OUTPUT"));
    for body in requests {
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }
    let repair = f
        .store
        .call(move |db| squad_store::judge_repairs::get(db, id, 2, case))
        .unwrap()
        .unwrap();
    assert_eq!(repair.rejected_output, "PRIVATE_REJECTED_OUTPUT");
    assert_eq!(
        f.store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM history WHERE instr(content,'PRIVATE_REJECTED_OUTPUT')>0",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0
    );

    let events = f
        .store
        .call(|db| squad_store::events::page(db, None, usize::MAX))
        .unwrap();
    assert!(
        !serde_json::to_string(&events)
            .unwrap()
            .contains("PRIVATE_REJECTED_OUTPUT")
    );
    let error = f
        .store
        .call(move |db| Ok(squad_store::work::record(db, id)?.unwrap().error))
        .unwrap();
    assert!(error.is_none());
    assert_eq!(
        f.store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM history WHERE content LIKE '%PRIVATE_REJECTED_OUTPUT%'",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0
    );
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn judges_never_retry_transport_retry_after_and_cancel_queued_and_active_calls() {
    let f = Fixture::new(2, 20_000_000).await;
    f.fake.completions(vec![rate_limited("0")]).await;
    let (id, _) = f.standalone("rate");
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.status(id) == WorkStatus::Failed).await;
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    runtime.shutdown().await.unwrap();

    let f = Fixture::new(2, 20_000_000).await;
    f.fake
        .completions(vec![delayed(Duration::from_secs(3))])
        .await;
    let (active, _) = f.round_case();
    let (queued, _) = f.standalone("queued");
    let mut runtime = f.start(OpenMode::OneShot);
    runtime.ready().await.unwrap();
    runtime.wake(&[active]);
    f.until(|| f.counts().1 == 1).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.fake.captured_bodies().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    f.bus
        .send(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    f.until(|| {
        f.status(active) == WorkStatus::Cancelled && f.status(queued) == WorkStatus::Cancelled
    })
    .await;
    runtime.shutdown().await.unwrap();
    assert_eq!(f.fake.captured_bodies().await.len(), 1);
    assert_eq!(f.counts().1, 1);
}

#[tokio::test]
async fn round_exhaustion_is_unscored_and_daily_limit_can_refuse_only_the_repair_call() {
    for (limit, expected_attempts) in [(20_000_000, 2), (45, 1)] {
        let f = Fixture::new(2, limit).await;
        f.fake
            .completions(vec![malformed_round_score(), malformed_round_score()])
            .await;
        let (id, _) = f.round_case();
        let runtime = f.start(OpenMode::Interactive);
        f.until(|| f.status(id) == WorkStatus::Failed).await;
        assert_eq!(f.counts().1, expected_attempts);
        assert_eq!(
            f.fake.captured_bodies().await.len(),
            expected_attempts as usize
        );
        let scores = f
            .store
            .call(|db| {
                Ok(db.query_row(
                    "SELECT COUNT(*) FROM round_scores WHERE scores_json IS NULL",
                    [],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap();
        assert_eq!(scores, i64::from(expected_attempts == 2));
        runtime.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn stale_round_results_are_rejected_and_late_accounting_remains_payable() {
    let f = Fixture::new(2, 20_000_000).await;
    let (id, case) = f.round_case();
    let owner = ExecutorKey::Case(case);
    let grant = f.claim(id, owner.clone()).await;
    f.bus
        .send(Request::Stop {
            request_id: EventId::new(),
            target: None,
        })
        .await;
    let submit = f.bus.executor_submit(owner);
    let work = &grant.work;
    let result = ExecutorRequest::Result {
        work_id: id,
        attempt: work.attempt,
        fence: grant.fence,
        epoch: grant.epoch,
        output: EnvelopePayload::DebateRoundScore(DebateRoundScore {
            debate_id: work.debate_id.unwrap(),
            case_id: case,
            round: 1,
            scores: Some(std::collections::BTreeMap::from([
                (
                    nick("member0"),
                    RoundScore {
                        score: 7,
                        reason: "good".into(),
                    },
                ),
                (
                    nick("member1"),
                    RoundScore {
                        score: 7,
                        reason: "good".into(),
                    },
                ),
            ])),
            tally: std::collections::BTreeMap::new(),
            reason: None,
        }),
    };
    assert!(
        matches!(submit.submit(result).await,RequestResult::Rejected{error,..} if error.code == ErrorCode::STALE_WORK_ATTEMPT)
    );
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Account {
                fence: grant.fence,
                epoch: grant.epoch,
                record: account(&grant, Some(0.000045))
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(f.counts().2, 45);
}

#[tokio::test]
async fn repairs_survive_recovery_and_round_boundaries_remain_frozen() {
    let f = Fixture::new(2, 20_000_000).await;
    let (id, case) = f.round_case();
    let grant = f.claim(id, ExecutorKey::Case(case)).await;
    let mut error = squad_core::router::error(
        ErrorCode::JUDGE_INVALID_VERDICT,
        "Judge output failed validation",
    );
    error.work_id = Some(id);
    error.case_id = Some(case);
    error.debate_id = grant.work.debate_id;
    error.contest_id = grant.work.contest_id;
    let repair = JudgeRepairData {
        attempt: 2,
        rejected_output: "RECOVERED_PRIVATE_REPAIR".into(),
        validation_errors: vec!["invalid JSON".into()],
    };
    assert!(matches!(
        f.bus
            .executor_submit(grant.owner.clone())
            .submit(ExecutorRequest::Failed {
                work_id: id,
                attempt: 1,
                fence: grant.fence,
                epoch: grant.epoch,
                error,
                repair: Some(repair)
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    f.store
        .call(move |db| {
            let tx = db.transaction()?;
            let source = squad_store::events::by_id(&tx, grant.work.trigger_event_id)?.unwrap();
            squad_store::history::project(
                &tx,
                &squad_store::history::attributed(
                    &source,
                    grant.work.context_id.clone(),
                    "late.case.material",
                    HistoryRole::User,
                    None,
                    "LATE_FUTURE_CANARY".into(),
                ),
            )?;
            tx.execute("UPDATE room_meta SET epoch=epoch+1", [])?;
            tx.execute("UPDATE cases SET epoch=(SELECT epoch FROM room_meta)", [])?;
            tx.execute(
                "UPDATE contests SET epoch=(SELECT epoch FROM room_meta)",
                [],
            )?;
            tx.execute(
                "UPDATE work SET epoch=(SELECT epoch FROM room_meta),lease_fence=lease_fence+1",
                [],
            )?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    let foreign = squad_store::readers::ScopedReaders::new(
        f.store.clone(),
        ExecutorKey::Case(CaseId::new()),
        2,
    );
    assert_eq!(
        foreign.repair_for_attempt(id, 1).await.unwrap_err(),
        ReaderError::Forbidden
    );
    f.fake
        .completions(vec![tied_round_score("member0", "member1")])
        .await;
    let runtime = f.start(OpenMode::Interactive);
    f.until(|| f.status(id) == WorkStatus::Completed).await;
    let requests = f.fake.captured_bodies().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].to_string().contains("RECOVERED_PRIVATE_REPAIR"));
    assert!(!requests[0].to_string().contains("LATE_FUTURE_CANARY"));
    assert_eq!(f.counts().1, 2);
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn final_and_round_deadlines_are_terminal_without_hidden_retries() {
    for round in [true, false] {
        let f = Fixture::new(2, 20_000_000).await;
        f.fake
            .completions(vec![delayed(Duration::from_secs(300))])
            .await;
        let (id, _) = if round {
            f.round_case()
        } else {
            f.standalone("deadline")
        };
        let runtime = f.start(OpenMode::Interactive);
        tokio::time::timeout(Duration::from_secs(3), async {
            while f.fake.captured_bodies().await.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::pause();
        let millis = if round {
            constants::ROUND_JUDGE_DEADLINE_MS
        } else {
            constants::FINAL_JUDGE_DEADLINE_MS
        };
        tokio::time::advance(Duration::from_millis(millis + 1)).await;
        tokio::time::resume();
        f.until(|| f.status(id) == WorkStatus::Failed).await;
        assert_eq!(f.fake.captured_bodies().await.len(), 1);
        let error = f
            .store
            .call(move |db| Ok(squad_store::work::record(db, id)?.unwrap().error.unwrap()))
            .unwrap();
        assert_eq!(error.code, ErrorCode::PROVIDER_TIMEOUT);
        runtime.shutdown().await.unwrap();
    }
}
