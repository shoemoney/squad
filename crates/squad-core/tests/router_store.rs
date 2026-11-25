use squad_core::{bus::InProcessBus, router::Router};
use squad_protocol::*;
use squad_store::Store;
use std::collections::BTreeMap;
fn nick(name: &str) -> MemberNick {
    serde_json::from_value(serde_json::json!(name)).unwrap()
}
fn fixture() -> (Store, Router) {
    let store = Store::open(":memory:").unwrap();
    store.call(|db| {
        for slug in ["test/model", "test/judge"] {
            db.execute("INSERT INTO catalog VALUES (?,131072,65536,'{\"prompt\":\"0\",\"completion\":\"0\"}','[]','2026-11-25T00:00:00Z')", [slug])?;
        }
        Ok(())
    }).unwrap();
    let roster = vec![nick("alice"), nick("bob")];
    let mut bindings = roster
        .iter()
        .map(|nick| {
            (
                nick.to_string(),
                ExecutorBinding {
                    model: Text::new("test/model").unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    bindings.insert(
        "judge".into(),
        ExecutorBinding {
            model: Text::new("test/judge").unwrap(),
            effort: ReasoningEffort::Low,
        },
    );
    let router = Router::new(store.clone(), roster, bindings, 20_000_000).unwrap();
    (store, router)
}
fn message(text: &str, id: EventId) -> Request {
    let meta = parser::SubmissionMeta::default();
    let ctx = parser::ParseContext {
        roster: vec![nick("alice"), nick("bob")],
        ..Default::default()
    };
    let parsed = parse(text, &meta, &ctx).unwrap();
    Request::from_parsed(id, parsed, &meta, false).unwrap()
}
fn accepted(result: &RequestResult) {
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
}
async fn first_work(store: Store, name: &str) -> WorkRecord {
    let reader =
        squad_store::readers::ScopedReaders::new(store, ExecutorKey::Member(nick(name)), 1);
    reader.unfinished().await.unwrap().remove(0)
}
#[test]
fn commit_groups_and_receipts_precede_resolution() {
    let (store, mut router) = fixture();
    let id = EventId::new();
    let request = Request::SetVars {
        request_id: id,
        updates: vec![
            VariableUpdate {
                name: VariableName::Rounds,
                value: 4,
            },
            VariableUpdate {
                name: VariableName::CensusPasses,
                value: 3,
            },
        ],
    };
    let result = router.process(request.clone(), None).unwrap();
    assert_eq!(result.events.len(), 2);
    for (index, event) in result.events.iter().enumerate() {
        assert_eq!(event.commit.commit_index, index as u32);
        assert_eq!(event.commit.commit_size, 2);
        assert_eq!(
            event.commit.accepted_at,
            result.events[0].commit.accepted_at
        );
    }
    assert!(router.process(request, None).unwrap().duplicate);
    let conflict = Request::SetVars {
        request_id: id,
        updates: vec![],
    };
    assert!(matches!(
        router.process(conflict, None),
        Err(squad_store::StoreError::Domain(
            ErrorCode::IDEMPOTENCY_CONFLICT,
            _
        ))
    ));
    let events = store
        .call(|db| squad_store::events::page(db, None, 1))
        .unwrap();
    assert_eq!(events.len(), 2);
}
#[test]
fn rollback_restores_events_variables_and_counters() {
    let (store, mut router) = fixture();
    let request = Request::SetVars {
        request_id: EventId::new(),
        updates: vec![
            VariableUpdate {
                name: VariableName::Rounds,
                value: 5,
            },
            VariableUpdate {
                name: VariableName::CensusPasses,
                value: 99,
            },
        ],
    };
    assert!(router.process(request, None).is_err());
    let state = store
        .call(|db| {
            Ok((
                db.query_row("SELECT value FROM vars WHERE name='ROUNDS'", [], |r| {
                    r.get::<_, u8>(0)
                })?,
                db.query_row("SELECT next_event_seq FROM room_meta", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!(state, (10, 1));
}
#[tokio::test]
async fn ownership_and_frozen_contexts_use_real_store() {
    let (store, mut router) = fixture();
    accepted(
        &router
            .process(message("@alice private one", EventId::new()), None)
            .unwrap()
            .result,
    );
    let a = first_work(store.clone(), "alice").await;
    let b = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member(nick("bob")),
        1,
    );
    assert_eq!(b.get(a.work_id).await.unwrap_err(), ReaderError::Forbidden);
    assert_eq!(
        b.for_work(a.work_id).await.unwrap_err(),
        ReaderError::Forbidden
    );
    let owner = ExecutorKey::Member(nick("alice"));
    let grant = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: a.work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    assert!(matches!(
        grant.result,
        RequestResult::Internal(InternalResult::Claimed(_))
    ));
    let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner, 1);
    let before = reader.for_work(a.work_id).await.unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].content, "private one");
    router
        .process(message("@alice private two", EventId::new()), None)
        .unwrap();
    assert_eq!(reader.for_work(a.work_id).await.unwrap(), before);
    assert!(b.unfinished().await.unwrap().is_empty());
}
#[tokio::test]
async fn result_work_history_and_actual_cost_commit_once() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice hi", EventId::new()), None)
        .unwrap();
    let record = first_work(store.clone(), "alice").await;
    let owner = ExecutorKey::Member(nick("alice"));
    let claimed = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: record.work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = claimed.result else {
        panic!("claim")
    };
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                work_id: record.work_id,
                attempt: 1,
                fence: grant.fence,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let reply = Completion {
        completed: CompletedGeneration {
            generation: GenerationIdentity {
                message_id: grant.work.message_id,
                work_id: grant.work.work_id,
                trigger_event_id: grant.work.trigger_event_id,
                attempt: 1,
            },
            model: grant.binding.model,
            finish_reason: FinishReason::Stop,
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 2,
                cost_usd: Some(0.25),
            }),
        },
        in_reply_to_message_id: grant.work.in_reply_to_message_id.unwrap(),
        content: "hello".into(),
    };
    let request = Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
        work_id: record.work_id,
        attempt: 1,
        fence: grant.fence,
        epoch: 1,
        output: EnvelopePayload::AgentReply(reply),
    }));
    let outcome = router
        .process(request.clone(), Some(owner.clone()))
        .unwrap();
    accepted(&outcome.result);
    assert_eq!(outcome.events.len(), 4);
    assert!(router.process(request, Some(owner)).unwrap().duplicate);
    let state = store
        .call(move |db| {
            Ok((
                squad_store::work::record(db, record.work_id)?
                    .unwrap()
                    .status,
                db.query_row("SELECT SUM(spent_micros) FROM spend_daily", [], |r| {
                    r.get::<_, i64>(0)
                })?,
                db.query_row(
                    "SELECT COUNT(*) FROM history WHERE role='assistant'",
                    [],
                    |r| r.get::<_, i64>(0),
                )?,
            ))
        })
        .unwrap();
    assert_eq!(state, (WorkStatus::Completed, 250_000, 1));
}
#[tokio::test]
async fn clear_fences_readers_and_preserves_monotonic_allocations() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice hi", EventId::new()), None)
        .unwrap();
    let record = first_work(store.clone(), "alice").await;
    let old = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member(nick("alice")),
        1,
    );
    let id = EventId::new();
    let clear = Request::Clear { request_id: id };
    let out = router.process(clear.clone(), None).unwrap();
    assert_eq!(out.events[0].commit.seq.get(), 2);
    assert!(router.process(clear, None).unwrap().duplicate);
    assert_eq!(
        old.get(record.work_id).await.unwrap_err(),
        ReaderError::StaleWork
    );
    assert_eq!(
        store
            .call(|db| Ok(
                db.query_row("SELECT next_history_pos FROM room_meta", [], |r| r
                    .get::<_, i64>(0))?
            ))
            .unwrap(),
        2
    );
}
#[tokio::test]
async fn bus_publishes_only_after_commit_and_wakeups_coalesce() {
    let (store, router) = fixture();
    let bus = InProcessBus::start(router);
    let mut feed = bus.subscribe();
    let key = ExecutorKey::Member(nick("alice"));
    let mut mailbox = bus.register(key.clone());
    for _ in 0..300 {
        bus.wake_mailbox(&key);
    }
    assert!(mailbox.try_recv().is_ok());
    assert!(mailbox.try_recv().is_err());
    let id = EventId::new();
    accepted(&bus.send(message("@alice hi", id)).await);
    let FeedItem::Committed(event) = feed.recv().await.unwrap() else {
        panic!("durable")
    };
    assert_eq!(event.base.id, id);
    assert!(
        store
            .call(move |db| squad_store::events::by_id(db, id))
            .unwrap()
            .is_some()
    );
    let rejected = bus
        .send(Request::Internal(InternalRequest::Executor(
            ExecutorRequest::Claim {
                work_id: WorkId::new(),
            },
        )))
        .await;
    assert!(matches!(
        rejected,
        RequestResult::Rejected {
            error: ErrorPayload {
                code: ErrorCode::PRODUCER_FORBIDDEN,
                ..
            },
            ..
        }
    ));
}

#[tokio::test]
async fn repairs_are_case_bound_and_step_contexts_exclude_ordinary_history() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice ordinary secret", EventId::new()), None)
        .unwrap();
    let contest = ContestId::new();
    let step = StepId::new();
    let step_work = WorkId::new();
    let case = CaseId::new();
    let judge_work = WorkId::new();
    let other_case = CaseId::new();
    let id = EventId::new();
    store
        .commit(id, "system".into(), "fixture".into(), move |tx, group| {
            let base = squad_core::router::envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::SystemNotice(SystemNotice {
                    code: Text::new("FIXTURE").unwrap(),
                    text: Text::new("context boundary").unwrap(),
                    work_id: None,
                    debate_id: None,
                    case_id: None,
                    contest_id: None,
                }),
            );
            let trigger = base.base.id;
            let seq = group.push(tx, base)?;
            let event = squad_store::events::at(tx, seq.get())?;
            let state = DebateState {
                debate_id: contest,
                state: DebateStatus::RoundRunning,
                debaters: [nick("alice"), nick("bob")],
                round: 1,
                total_rounds: 2,
                judge_model: "test/model".into(),
                judge_requested: true,
                state_version: 1,
                terminal_reason: None,
                case_id: Some(case),
                case_ids: vec![case],
            };
            squad_store::contests::save_debate(tx, &state, None, seq)?;
            let context = ContextId(format!("contest:{contest}:{step}:alice"));
            let script = ContestStep {
                contest_id: contest,
                step_id: step,
                ordinal: 1,
                script_version: Text::new("test.v1").unwrap(),
                move_name: ContestMove::Fanout,
                inputs: vec![trigger],
                assignments: vec![ContestAssignment {
                    target: Target::Member(nick("alice")),
                    context_id: context.clone(),
                    payload_refs: vec![trigger],
                    work_id: Some(step_work),
                }],
            };
            squad_store::contest_steps::insert(tx, &script, seq)?;
            let projection = HistoryProjection {
                context_id: context.clone(),
                origin_event_id: trigger,
                projection_key: Text::new("anonymous.bundle").unwrap(),
                role: HistoryRole::User,
                name: Some("squad".into()),
                content: "anonymous bundle".into(),
                meta: HistoryMetadata::Anonymous { anonymous: True },
            };
            let boundary = squad_store::history::project(tx, &projection)?.unwrap();
            let binding = ExecutorBinding {
                model: Text::new("test/model").unwrap(),
                effort: ReasoningEffort::Low,
            };
            let work = Work {
                work_id: step_work,
                message_id: MessageId::new(),
                in_reply_to_message_id: Some(MessageId::new()),
                kind: WorkKind::Reply,
                target: Target::Member(nick("alice")),
                context_id: context,
                trigger_event_id: trigger,
                history_through: boundary,
                attempt: 1,
                debate_id: None,
                round: None,
                replay: None,
                case_id: None,
                prompt_ref: None,
                max_output_tokens: None,
                contest_id: Some(contest),
                step_id: Some(step),
            };
            squad_store::work::insert(tx, &work, &binding, None)?;
            squad_store::contest_steps::attach_work(tx, step, &work)?;
            let opened = CaseOpened {
                case_id: case,
                judge_invocation_id: EventId::new(),
                work_id: Some(judge_work),
                judge_model: Text::new("test/model").unwrap(),
                material: CaseMaterial::Standalone {
                    messages: vec![],
                    question: Text::new("case secret").unwrap(),
                },
                debate_id: None,
                contest_id: None,
            };
            squad_store::cases::open(tx, &opened, seq)?;
            let case_context = ContextId(format!("judge:{case}"));
            let boundary = squad_store::history::project(
                tx,
                &squad_store::history::attributed(
                    &event,
                    case_context.clone(),
                    "case.input",
                    HistoryRole::User,
                    None,
                    "case secret".into(),
                ),
            )?
            .unwrap();
            let work = Work {
                work_id: judge_work,
                message_id: MessageId::new(),
                in_reply_to_message_id: None,
                kind: WorkKind::Judge,
                target: Target::Judge,
                context_id: case_context,
                trigger_event_id: trigger,
                history_through: boundary,
                attempt: 1,
                debate_id: None,
                round: None,
                replay: None,
                case_id: Some(case),
                prompt_ref: None,
                max_output_tokens: None,
                contest_id: None,
                step_id: None,
            };
            squad_store::work::insert(tx, &work, &binding, None)?;
            Ok(group.accepted(id))
        })
        .unwrap();
    let alice = ExecutorKey::Member(nick("alice"));
    let case_owner = ExecutorKey::Case(case);
    store
        .call(move |db| {
            let tx = db.transaction()?;
            squad_store::leases::claim(&tx, step_work, &alice, chrono::Utc::now(), None)?.unwrap();
            squad_store::leases::claim(&tx, judge_work, &case_owner, chrono::Utc::now(), None)?
                .unwrap();
            squad_store::judge_repairs::save(
                &tx,
                judge_work,
                case,
                &JudgeRepairData {
                    attempt: 1,
                    rejected_output: "private rejected output".into(),
                    validation_errors: vec!["invalid JSON".into()],
                },
            )?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    let member = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Member(nick("alice")),
        1,
    );
    let entries = member.for_work(step_work).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].content, "anonymous bundle");
    assert_eq!(
        serde_json::to_value(&entries[0].meta).unwrap(),
        serde_json::json!({"anonymous":true})
    );
    assert_eq!(
        member.get(judge_work).await.unwrap_err(),
        ReaderError::Forbidden
    );
    assert_eq!(
        member.repair_for_attempt(judge_work, 1).await.unwrap_err(),
        ReaderError::Forbidden
    );
    let foreign =
        squad_store::readers::ScopedReaders::new(store.clone(), ExecutorKey::Case(other_case), 1);
    assert_eq!(
        foreign.repair_for_attempt(judge_work, 1).await.unwrap_err(),
        ReaderError::Forbidden
    );
    let case_reader = squad_store::readers::ScopedReaders::new(store, ExecutorKey::Case(case), 1);
    assert_eq!(
        case_reader.for_work(judge_work).await.unwrap()[0].content,
        "case secret"
    );
    assert_eq!(
        case_reader
            .repair_for_attempt(judge_work, 1)
            .await
            .unwrap()
            .unwrap()
            .rejected_output,
        "private rejected output"
    );
}

#[tokio::test]
async fn ingress_saturation_cannot_lose_durable_work() {
    let (store, router) = fixture();
    let bus = InProcessBus::start(router);
    let mut tasks = Vec::new();
    // A capacity-one full mailbox stays full throughout this capacity-256 ingress burst.
    let _mailbox = bus.register(ExecutorKey::Member(nick("alice")));
    for _ in 0..300 {
        let bus = bus.clone();
        tasks.push(tokio::spawn(async move {
            bus.send(message("@alice hello", EventId::new())).await
        }));
    }
    for task in tasks {
        accepted(&task.await.unwrap());
    }
    let counts = store
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?,
                db.query_row("SELECT COUNT(*) FROM work WHERE status='queued'", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!(counts, (580, 20));
}
#[tokio::test]
async fn held_delivery_preserves_source_order_identity_and_deduplicates() {
    let (store, mut router) = fixture();
    let first = EventId::new();
    let second = EventId::new();
    router
        .process(message("@alice first", first), None)
        .unwrap();
    router
        .process(message("@alice second", second), None)
        .unwrap();
    let contest = ContestId::new();
    store
        .call(move |db| {
            let tx = db.transaction()?;
            let state = DebateState {
                debate_id: contest,
                state: DebateStatus::RoundRunning,
                debaters: [nick("alice"), nick("bob")],
                round: 1,
                total_rounds: 2,
                judge_model: "test/model".into(),
                judge_requested: false,
                state_version: 1,
                terminal_reason: None,
                case_id: None,
                case_ids: vec![],
            };
            squad_store::contests::save_debate(&tx, &state, None, Cursor::new(2).unwrap())?;
            let dest = ContextId("bob".into());
            for id in [second, first] {
                let source = squad_store::events::by_id(&tx, id)?.unwrap();
                let projection = squad_store::history::attributed(
                    &source,
                    dest.clone(),
                    "delayed",
                    HistoryRole::User,
                    None,
                    id.to_string(),
                );
                squad_store::history::defer(&tx, contest, &projection)?;
            }
            assert!(
                squad_store::history::read(&tx, &dest, HistoryPosition::new(99).unwrap())?
                    .is_empty()
            );
            squad_store::history::release(&tx, contest)?;
            squad_store::history::release(&tx, contest)?;
            let rows = squad_store::history::read(&tx, &dest, HistoryPosition::new(99).unwrap())?;
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].content, first.to_string());
            assert_eq!(rows[1].content, second.to_string());
            let HistoryMetadata::Attributed {
                origin_event_id,
                origin_seq,
                ..
            } = rows[0].meta
            else {
                panic!("attributed")
            };
            assert_eq!(origin_event_id, first);
            assert_eq!(origin_seq.get(), 1);
            assert!(rows[0].position.get() > origin_seq.get());
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}

#[tokio::test]
async fn recipient_admission_is_independent_and_judging_allocates_only_case_work() {
    let (store, mut router) = fixture();
    for _ in 0..20 {
        router
            .process(message("@alice queued", EventId::new()), None)
            .unwrap();
    }
    let outcome = router
        .process(message("@all hello", EventId::new()), None)
        .unwrap();
    assert_eq!(outcome.events.len(), 2);
    assert!(
        matches!(&outcome.events[1].content,EnvelopePayload::Error(e) if e.code==ErrorCode::AGENT_QUEUE_FULL)
    );
    let bob = first_work(store.clone(), "bob").await;
    assert_eq!(bob.kind, WorkKind::Reply);
    let source = outcome.events[0].base.id;
    let meta = parser::SubmissionMeta {
        quote: vec![source],
        reply_to: vec![],
    };
    let parsed = parse("@judge decide", &meta, &router.parse_context().unwrap()).unwrap();
    let id = EventId::new();
    let request = Request::from_parsed(id, parsed, &meta, false).unwrap();
    let outcome = router.process(request, None).unwrap();
    assert!(
        outcome
            .events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::CaseOpened(_)))
    );
    let counts = store
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM work WHERE kind='reply'", [], |r| {
                    r.get::<_, i64>(0)
                })?,
                db.query_row("SELECT COUNT(*) FROM work WHERE kind='judge'", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!(counts, (21, 1));
}
#[test]
fn all_exclusions_are_frozen_for_every_supported_roster_size() {
    for size in 1..=12 {
        let store = Store::open(":memory:").unwrap();
        let roster = (0..size)
            .map(|n| nick(&format!("m{n}")))
            .collect::<Vec<_>>();
        let bindings = roster
            .iter()
            .map(|n| {
                (
                    n.to_string(),
                    ExecutorBinding {
                        model: Text::new("test/model").unwrap(),
                        effort: ReasoningEffort::Low,
                    },
                )
            })
            .collect();
        let mut router = Router::new(store, roster.clone(), bindings, 20_000_000).unwrap();
        let raw = if size > 1 {
            "@all -m0 hello"
        } else {
            "@all hello"
        };
        let meta = parser::SubmissionMeta::default();
        let parsed = parse(raw, &meta, &router.parse_context().unwrap()).unwrap();
        let request = Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap();
        let out = router.process(request, None).unwrap();
        let expected = roster
            .into_iter()
            .filter(|nick| size == 1 || nick.as_str() != "m0")
            .map(Recipient::Member)
            .collect::<Vec<_>>();
        assert_eq!(out.events[0].base.recipients, expected);
    }
}

#[tokio::test]
async fn deliberate_quote_sharing_exposes_only_frozen_selected_content() {
    let (store, mut router) = fixture();
    let source = EventId::new();
    router
        .process(message("@alice private source", source), None)
        .unwrap();
    let meta = parser::SubmissionMeta {
        quote: vec![source],
        reply_to: vec![],
    };
    let parsed = parse("@bob review", &meta, &router.parse_context().unwrap()).unwrap();
    let id = EventId::new();
    let request = Request::from_parsed(id, parsed, &meta, false).unwrap();
    router.process(request.clone(), None).unwrap();
    // Retrying accepted content does not repeat mutable quote resolution.
    store
        .call(move |db| {
            db.execute(
                "UPDATE events SET payload=json_set(payload,'$.body','changed later') WHERE id=?",
                [source.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    assert!(router.process(request, None).unwrap().duplicate);
    let bob = first_work(store.clone(), "bob").await;
    let owner = ExecutorKey::Member(nick("bob"));
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: bob.work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let reader = squad_store::readers::ScopedReaders::new(store, owner, 1);
    let entries = reader.for_work(bob.work_id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].content.contains("me: private source"));
    assert!(!entries[0].content.contains("changed later"));
}
#[tokio::test]
async fn failed_sql_commit_has_no_receipt_feed_or_durable_acceptance() {
    let (store, router) = fixture();
    store.call(|db| {db.execute_batch("CREATE TRIGGER reject_work BEFORE INSERT ON work BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;")?;Ok(())}).unwrap();
    let bus = InProcessBus::start(router);
    let mut feed = bus.subscribe();
    let result = bus.send(message("@alice rollback", EventId::new())).await;
    assert!(matches!(result, RequestResult::Rejected { .. }));
    assert!(matches!(
        feed.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    let counts = store
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?,
                db.query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))?,
                db.query_row("SELECT COUNT(*) FROM receipts", [], |r| r.get::<_, i64>(0))?,
            ))
        })
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}
struct NoRender;
impl PromptRenderer for NoRender {
    fn render(
        &self,
        _: &PromptRef,
        _: &BTreeMap<String, String>,
    ) -> std::result::Result<String, ReaderError> {
        Err(ReaderError::NotFound)
    }
}
#[tokio::test]
async fn runtime_constructs_only_the_requested_executor_capabilities() {
    let (store, router) = fixture();
    let bus = InProcessBus::start(router);
    accepted(&bus.send(message("@alice hi", EventId::new())).await);
    for executor in [
        ExecutorKey::Member(nick("alice")),
        ExecutorKey::Member(nick("bob")),
        ExecutorKey::Case(CaseId::new()),
    ] {
        let context = squad_core::runtime::contexts::agent_context(
            store.clone(),
            &bus,
            executor.clone(),
            1,
            std::sync::Arc::new(NoRender),
            tokio_util::sync::CancellationToken::new(),
        );
        assert_eq!(context.executor, executor);
        let records = context.work.unfinished().await.unwrap();
        assert_eq!(
            records.len(),
            usize::from(executor == ExecutorKey::Member(nick("alice")))
        );
    }
}

#[tokio::test]
async fn admission_fifo_does_not_depend_on_uuid_random_tails() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice fifo", EventId::new()), None)
        .unwrap();
    let original = first_work(store.clone(), "alice").await;
    let first: WorkId = "01900000-0000-7000-8000-0000000000ff".parse().unwrap();
    let second: WorkId = "01900000-0000-7000-8000-000000000001".parse().unwrap();
    store
        .call(move |db| {
            let tx = db.transaction()?;
            let (mut work, binding) = squad_store::work::template(&tx, original.work_id)?.unwrap();
            tx.execute(
                "UPDATE work SET status='cancelled',runnable=0 WHERE work_id=?",
                [original.work_id.to_string()],
            )?;
            for id in [first, second] {
                work.work_id = id;
                work.message_id = MessageId::new();
                squad_store::work::insert(&tx, &work, &binding, None)?;
            }
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    let owner = ExecutorKey::Member(nick("alice"));
    let reader = squad_store::readers::ScopedReaders::new(store, owner.clone(), 1);
    assert_eq!(
        reader
            .unfinished()
            .await
            .unwrap()
            .iter()
            .map(|r| r.work_id)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    let claim = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: second,
            })),
            Some(owner),
        )
        .unwrap();
    assert!(matches!(
        claim.result,
        RequestResult::Internal(InternalResult::NoWork)
    ));
}

#[test]
fn storage_ceiling_refuses_new_generation_but_keeps_cleanup_and_history() {
    let (store, router) = fixture();
    let mut router = router.with_storage_capacity(1);
    let id = EventId::new();
    let outcome = router.process(message("@alice hello", id), None).unwrap();
    match &outcome.result {
        RequestResult::Rejected { error, .. } => {
            assert_eq!(error.code, ErrorCode::STORAGE_LIMIT);
            assert!(
                error.text.as_str().contains("/clear") && error.text.as_str().contains("export")
            );
        }
        other => panic!("{other:?}"),
    }
    let work: i64 = store
        .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(work, 0);
    let vars = Request::SetVars {
        request_id: EventId::new(),
        updates: vec![VariableUpdate {
            name: VariableName::Rounds,
            value: 4,
        }],
    };
    accepted(&router.process(vars, None).unwrap().result);
    let clear = Request::Clear {
        request_id: EventId::new(),
    };
    accepted(&router.process(clear, None).unwrap().result);
    let mut roomy = fixture().1;
    accepted(
        &roomy
            .process(message("@alice hello", EventId::new()), None)
            .unwrap()
            .result,
    );
}
