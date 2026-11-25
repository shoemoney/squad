use squad_protocol::*;
use squad_store::{Store, accounting as a};
fn grant() -> ExecutionGrant {
    let nick: MemberNick = serde_json::from_value(serde_json::json!("alice")).unwrap();
    let now = chrono::Utc::now();
    ExecutionGrant {
        binding: ExecutorBinding {
            model: Text::new("test/model").unwrap(),
            effort: ReasoningEffort::Low,
        },
        work: Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: Some(MessageId::new()),
            kind: WorkKind::Reply,
            target: Target::Member(nick.clone()),
            context_id: ContextId(nick.to_string()),
            trigger_event_id: EventId::new(),
            history_through: HistoryPosition::new(1).unwrap(),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: None,
            prompt_ref: None,
            max_output_tokens: None,
            contest_id: None,
            step_id: None,
        },
        owner: ExecutorKey::Member(nick),
        fence: 1,
        epoch: 1,
        lease_until: now + chrono::Duration::seconds(30),
        deadline: None,
    }
}
fn usage(
    grant: &ExecutionGrant,
    day: &str,
    cost: Option<f64>,
    outcome: UsageOutcome,
) -> UsageRecord {
    UsageRecord {
        work_id: grant.work.work_id,
        attempt: 1,
        target: grant.work.target.clone(),
        context_id: grant.work.context_id.clone(),
        provider_request_id: None,
        model: grant.binding.model.clone(),
        started_at: chrono::Utc::now(),
        ended_at: Some(chrono::Utc::now()),
        outcome,
        usage: Some(Usage {
            input_tokens: 20,
            output_tokens: 2,
            cost_usd: cost,
        }),
        estimated_cost_usd: None,
        debate_id: None,
        case_id: None,
        contest_id: None,
        response_model: None,
        day: day.into(),
    }
}
#[test]
fn unknown_billed_failure_correction_and_clear_preserve_the_ledger() {
    let store = Store::open(":memory:").unwrap();
    let grant = grant();
    store
        .call(move |db| {
            let tx = db.transaction()?;
            a::reserve(
                &tx,
                "reservation",
                Some(grant.work.work_id),
                "2026-11-23",
                500_000,
                20_000_000,
            )?;
            a::start(&tx, &grant, chrono::Utc::now(), "2026-11-23", 20_000_000)?;
            a::reconcile(
                &tx,
                &usage(&grant, "2026-11-23", None, UsageOutcome::Failed),
            )?;
            assert_eq!(
                a::daily(&tx, "2026-11-23")?,
                a::DailySpend {
                    day: "2026-11-23".into(),
                    spent_micros: 0,
                    reserved_micros: 500_000,
                    unknown_calls: 1
                }
            );
            let billed = usage(&grant, "2026-11-23", Some(0.1234567), UsageOutcome::Failed);
            a::reconcile(&tx, &billed)?;
            a::reconcile(&tx, &billed)?;
            assert_eq!(a::daily(&tx, "2026-11-23")?.spent_micros, 123457);
            assert_eq!(a::daily(&tx, "2026-11-23")?.unknown_calls, 0);
            assert_eq!(a::daily(&tx, "2026-11-23")?.reserved_micros, 0);
            a::reconcile(
                &tx,
                &usage(&grant, "2026-11-23", Some(0.1), UsageOutcome::Cancelled),
            )?;
            assert_eq!(a::daily(&tx, "2026-11-23")?.spent_micros, 100_000);
            squad_store::clear::content(&tx)?;
            assert_eq!(a::daily(&tx, "2026-11-23")?.spent_micros, 100_000);
            a::reconcile(
                &tx,
                &usage(&grant, "2026-11-23", Some(0.2), UsageOutcome::Cancelled),
            )?;
            assert_eq!(a::daily(&tx, "2026-11-23")?.spent_micros, 200_000);
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}
#[test]
fn midnight_moves_only_unstarted_reservations_and_daily_admission_rolls_back() {
    let store = Store::open(":memory:").unwrap();
    let started = grant();
    let waiting = grant();
    store
        .call(move |db| {
            let tx = db.transaction()?;
            a::reserve(
                &tx,
                "started",
                Some(started.work.work_id),
                "2026-11-23",
                500_000,
                1_000_000,
            )?;
            a::start(&tx, &started, chrono::Utc::now(), "2026-11-23", 1_000_000)?;
            a::reserve(
                &tx,
                "waiting",
                Some(waiting.work.work_id),
                "2026-11-23",
                500_000,
                1_000_000,
            )?;
            a::transfer_unstarted(&tx, waiting.work.work_id, "2026-11-24")?;
            assert_eq!(a::daily(&tx, "2026-11-23")?.reserved_micros, 500_000);
            assert_eq!(a::daily(&tx, "2026-11-24")?.reserved_micros, 500_000);
            a::transfer_unstarted(&tx, started.work.work_id, "2026-11-24")?;
            assert_eq!(a::daily(&tx, "2026-11-23")?.reserved_micros, 500_000);
            assert!(a::reserve(&tx, "too-big", None, "2026-11-24", 600_000, 1_000_000).is_err());
            a::reconcile(
                &tx,
                &usage(&started, "2026-11-23", Some(1.), UsageOutcome::Completed),
            )?;
            assert!(a::admit(&tx, "2026-11-23", 0, 1_000_000).is_err());
            assert!(a::admit(&tx, "2026-11-23", 0, 2_000_000).is_ok());
            assert_eq!(a::daily(&tx, "2026-11-24")?.spent_micros, 0);
            tx.rollback()?;
            assert_eq!(a::daily(db, "2026-11-23")?.spent_micros, 0);
            Ok(())
        })
        .unwrap();
}
#[test]
fn cost_conversion_rounds_once_and_reservations_round_up() {
    assert_eq!(a::reported_micros(0.0000004).unwrap(), 0);
    assert_eq!(a::reservation_micros(0.0000004).unwrap(), 1);
    for invalid in [-1., f64::INFINITY, f64::NAN, f64::MAX] {
        assert!(a::reported_micros(invalid).is_err());
    }
}
