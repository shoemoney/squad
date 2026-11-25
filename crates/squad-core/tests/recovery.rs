use squad_core::router::Router;
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

use squad_core::router::recovery::{CaseExecutors, RuntimeHarness};
#[derive(Default)]
struct Hooks {
    spawned: Vec<CaseId>,
    retired: Vec<CaseId>,
}
impl CaseExecutors for Hooks {
    fn spawn(&mut self, case: CaseId) -> squad_store::Result<()> {
        self.spawned.push(case);
        Ok(())
    }
    fn retire(&mut self, case: CaseId) {
        self.retired.push(case);
    }
}
fn pending(store: &Store) -> WorkId {
    store
        .call(|db| {
            Ok(db
                .query_row("SELECT work_id FROM work ORDER BY rowid LIMIT 1", [], |r| {
                    r.get::<_, String>(0)
                })?
                .parse()
                .unwrap())
        })
        .unwrap()
}
fn claim(runtime: &mut RuntimeHarness<Hooks>, id: WorkId) -> ExecutionGrant {
    let result = runtime
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: id,
            })),
            Some(ExecutorKey::Member(nick("alice"))),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = result.result else {
        panic!("{result:?}")
    };
    grant
}
#[test]
fn oneshot_keeps_prior_work_dormant_and_interactive_finds_a_lost_wakeup() {
    let (store, mut router) = fixture();
    let id = EventId::new();
    let request = message("@alice committed before crash", id);
    let original = router.process(request.clone(), None).unwrap().result;
    let work = pending(&store);
    let mut runtime = RuntimeHarness::new(router, OpenMode::OneShot, Hooks::default()).unwrap();
    assert!(runtime.scan().unwrap().is_empty());
    assert!(
        runtime
            .process(
                Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                    work_id: work
                })),
                Some(ExecutorKey::Member(nick("alice")))
            )
            .is_err()
    );
    assert_eq!(runtime.process(request, None).unwrap().result, original);
    assert!(runtime.scan().unwrap().is_empty());
    drop(runtime);
    let (_, template) = fixture();
    let router = Router::new(
        store.clone(),
        template.roster().to_vec(),
        BTreeMap::from([
            (
                "alice".into(),
                ExecutorBinding {
                    model: Text::new("test/model").unwrap(),
                    effort: ReasoningEffort::Low,
                },
            ),
            (
                "bob".into(),
                ExecutorBinding {
                    model: Text::new("test/model").unwrap(),
                    effort: ReasoningEffort::Low,
                },
            ),
        ]),
        20_000_000,
    )
    .unwrap();
    let mut resumed = RuntimeHarness::new(router, OpenMode::Interactive, Hooks::default()).unwrap();
    assert_eq!(resumed.scan().unwrap()[0].work_id, work);
    assert_eq!(claim(&mut resumed, work).work.attempt, 1);
}
#[test]
fn recovery_reuses_only_unstarted_ordinals_and_fences_stopped_workers() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice claim", EventId::new()), None)
        .unwrap();
    let id = pending(&store);
    let mut runtime = RuntimeHarness::new(router, OpenMode::Interactive, Hooks::default()).unwrap();
    let first = claim(&mut runtime, id);
    drop(runtime);
    // A restarted process invalidates even an unexpired lease.
    store
        .call(|db| {
            let tx = db.transaction()?;
            squad_store::leases::recover(&tx, true, chrono::Utc::now())?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    let second = store
        .call(move |db| {
            let tx = db.transaction()?;
            let grant = squad_store::leases::claim(
                &tx,
                id,
                &ExecutorKey::Member(nick("alice")),
                chrono::Utc::now(),
                None,
            )?
            .unwrap();
            tx.commit()?;
            Ok(grant)
        })
        .unwrap();
    assert_eq!(second.work.attempt, 1);
    assert!(second.fence > first.fence);
    let second_copy = second.clone();
    store
        .call(move |db| {
            assert!(
                squad_store::work::check_fence(
                    db,
                    id,
                    &first.owner,
                    1,
                    first.fence,
                    first.epoch,
                    chrono::Utc::now()
                )
                .is_err()
            );
            let tx = db.transaction()?;
            squad_store::accounting::start(
                &tx,
                &second_copy,
                chrono::Utc::now(),
                "2026-11-24",
                20_000_000,
            )?;
            tx.commit()?;
            let tx = db.transaction()?;
            squad_store::leases::recover(&tx, true, chrono::Utc::now())?;
            tx.commit()?;
            let tx = db.transaction()?;
            let next =
                squad_store::leases::claim(&tx, id, &second_copy.owner, chrono::Utc::now(), None)?
                    .unwrap();
            assert_eq!(next.work.attempt, 2);
            assert_eq!(
                squad_store::accounting::daily(&tx, "2026-11-24")?.unknown_calls,
                1
            );
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM attempts", [], |r| r.get::<_, i64>(0))?,
                1
            );
            tx.commit()?;
            Ok(())
        })
        .unwrap();
}
#[test]
fn newly_committed_oneshot_work_is_enabled_but_does_not_drain_other_contexts() {
    let (store, mut router) = fixture();
    router
        .process(message("@bob old no-wait", EventId::new()), None)
        .unwrap();
    let mut runtime = RuntimeHarness::new(router, OpenMode::OneShot, Hooks::default()).unwrap();
    runtime
        .process(message("@alice this invocation", EventId::new()), None)
        .unwrap();
    let work = runtime.scan().unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].target, Target::Member(nick("alice")));
    claim(&mut runtime, work[0].work_id);
    assert_eq!(
        store
            .call(|db| Ok(
                db.query_row("SELECT status FROM work WHERE target='bob'", [], |r| r
                    .get::<_, String>(
                    0
                ))?
            ))
            .unwrap(),
        "queued"
    );
}

#[test]
fn oneshot_can_claim_its_own_work_behind_dormant_work_for_the_same_member() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice prior invocation", EventId::new()), None)
        .unwrap();
    let old = pending(&store);
    let mut runtime = RuntimeHarness::new(router, OpenMode::OneShot, Hooks::default()).unwrap();
    runtime
        .process(message("@alice this invocation", EventId::new()), None)
        .unwrap();
    let own = runtime.scan().unwrap();
    assert_eq!(own.len(), 1);
    assert_ne!(own[0].work_id, old);
    claim(&mut runtime, own[0].work_id);
    store
        .call(move |db| {
            assert_eq!(squad_store::work::record(db, old)?.unwrap().attempt, 0);
            Ok(())
        })
        .unwrap();
}
fn result_request(grant: &ExecutionGrant) -> Request {
    Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
        work_id: grant.work.work_id,
        attempt: grant.work.attempt,
        fence: grant.fence,
        epoch: grant.epoch,
        output: EnvelopePayload::AgentReply(Completion {
            completed: CompletedGeneration {
                generation: GenerationIdentity {
                    message_id: grant.work.message_id,
                    work_id: grant.work.work_id,
                    trigger_event_id: grant.work.trigger_event_id,
                    attempt: grant.work.attempt,
                },
                model: grant.binding.model.clone(),
                finish_reason: FinishReason::Stop,
                usage: Some(Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cost_usd: Some(0.01),
                }),
            },
            in_reply_to_message_id: grant.work.in_reply_to_message_id.unwrap(),
            content: "accepted final".into(),
        }),
    }))
}
#[test]
fn lost_result_reply_reuses_receipt_and_never_recounts_cost_or_history() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice result", EventId::new()), None)
        .unwrap();
    let id = pending(&store);
    let mut runtime = RuntimeHarness::new(router, OpenMode::Interactive, Hooks::default()).unwrap();
    let grant = claim(&mut runtime, id);
    runtime
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                input_tokens: None,
                work_id: id,
                attempt: 1,
                fence: grant.fence,
            })),
            Some(grant.owner.clone()),
        )
        .unwrap();
    let request = result_request(&grant);
    let accepted = runtime
        .process(request.clone(), Some(grant.owner.clone()))
        .unwrap();
    assert!(!accepted.duplicate);
    let repeated = runtime.process(request, Some(grant.owner)).unwrap();
    assert!(repeated.duplicate);
    assert_eq!(accepted.result, repeated.result);
    assert!(repeated.events.is_empty());
    store
        .call(move |db| {
            let tx = db.transaction()?;
            squad_store::leases::recover(&tx, true, chrono::Utc::now())?;
            tx.commit()?;
            assert_eq!(
                squad_store::work::record(db, id)?.unwrap().status,
                WorkStatus::Completed
            );
            assert_eq!(
                db.query_row(
                    "SELECT COUNT(*) FROM history WHERE role='assistant'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                1
            );
            assert_eq!(
                db.query_row("SELECT SUM(spent_micros) FROM spend_daily", [], |r| r
                    .get::<_, i64>(0))?,
                10_000
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn stop_and_clear_reject_old_claimed_content() {
    for clear in [false, true] {
        let (store, mut router) = fixture();
        router
            .process(message("@alice delayed", EventId::new()), None)
            .unwrap();
        let id = pending(&store);
        let mut runtime =
            RuntimeHarness::new(router, OpenMode::Interactive, Hooks::default()).unwrap();
        let grant = claim(&mut runtime, id);
        let request = if clear {
            Request::Clear {
                request_id: EventId::new(),
            }
        } else {
            Request::Stop {
                request_id: EventId::new(),
                target: Some(nick("alice")),
            }
        };
        runtime.process(request, None).unwrap();
        assert!(matches!(
            runtime.process(result_request(&grant), Some(grant.owner)),
            Err(squad_store::StoreError::Domain(
                ErrorCode::STALE_WORK_ATTEMPT,
                _
            ))
        ));
        store
            .call(|db| {
                assert_eq!(
                    db.query_row(
                        "SELECT COUNT(*) FROM history WHERE role='assistant'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )?,
                    0
                );
                Ok(())
            })
            .unwrap();
    }
}
#[test]
fn case_lifecycle_double_recovers_repairs_retires_and_spawns_for_a_later_summon() {
    let (store, mut router) = fixture();
    let source = EventId::new();
    router
        .process(message("@alice source", source), None)
        .unwrap();
    let meta = parser::SubmissionMeta {
        quote: vec![source],
        reply_to: vec![],
    };
    let parsed = parse("@judge evaluate", &meta, &router.parse_context().unwrap()).unwrap();
    router
        .process(
            Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap(),
            None,
        )
        .unwrap();
    let mut runtime = RuntimeHarness::new(router, OpenMode::Interactive, Hooks::default()).unwrap();
    let case = runtime.hooks().spawned[0];
    assert_eq!(runtime.hooks().spawned.len(), 1);
    let work = runtime
        .scan()
        .unwrap()
        .into_iter()
        .find(|w| w.target == Target::Judge)
        .unwrap();
    let claim = runtime
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: work.work_id,
            })),
            Some(ExecutorKey::Case(case)),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = claim.result else {
        panic!("case claim")
    };
    let id = work.work_id;
    store.call(move |db| {
        let tx=db.transaction()?;
        squad_store::accounting::start(&tx,&grant,chrono::Utc::now(),"2026-11-24",20_000_000)?;
        tx.execute("INSERT INTO judge_repairs VALUES (?1,1,?2,'bad output','[\"schema failure\"]')",rusqlite::params![id.to_string(),case.to_string()])?;
        tx.commit()?;
        let tx=db.transaction()?;squad_store::leases::recover(&tx,true,chrono::Utc::now())?;tx.commit()?;
        let tx=db.transaction()?;
        let resumed=squad_store::leases::claim(&tx,id,&ExecutorKey::Case(case),chrono::Utc::now(), None)?.unwrap();
        assert_eq!(resumed.work.attempt,2);
        assert_eq!(tx.query_row("SELECT rejected_output FROM judge_repairs WHERE work_id=?",[id.to_string()],|r|r.get::<_,String>(0))?,"bad output");
        tx.execute("UPDATE work SET status='completed',runnable=0,lease_fence=lease_fence+1 WHERE work_id=?",[id.to_string()])?;
        tx.commit()?;Ok(())
    }).unwrap();
    runtime.scan().unwrap();
    assert_eq!(runtime.hooks().retired, vec![case]);
    // The later case summon is a storage double: the case router owns admission.
    store
        .call(move |db| {
            let tx = db.transaction()?;
            let (mut work, binding) = squad_store::work::template(&tx, id)?.unwrap();
            work.work_id = WorkId::new();
            work.message_id = MessageId::new();
            work.attempt = 1;
            squad_store::work::insert(&tx, &work, &binding, None)?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    runtime.scan().unwrap();
    assert_eq!(runtime.hooks().spawned, vec![case, case]);
    runtime
        .process(
            Request::Clear {
                request_id: EventId::new(),
            },
            None,
        )
        .unwrap();
    assert_eq!(runtime.hooks().retired, vec![case, case]);
}

#[test]
fn single_startup_call_persists_oneshot_dormancy_then_restores_original_scheduling_flags() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice committed", EventId::new()), None)
        .unwrap();
    router
        .process(message("@bob held gate", EventId::new()), None)
        .unwrap();
    store
        .call(|db| {
            db.execute("UPDATE work SET runnable=0 WHERE target='bob'", [])?;
            Ok(())
        })
        .unwrap();
    let report = squad_core::router::recovery::recover_on_open_mode(
        &store,
        OpenMode::OneShot,
        chrono::Utc::now(),
    )
    .unwrap();
    assert!(report.eligible.is_empty());
    assert_eq!(report.epoch, 1);
    store
        .call(|db| {
            assert_eq!(
                db.query_row("SELECT COUNT(*) FROM work WHERE runnable=1", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .unwrap();
    // Another OneShot must not overwrite the original flags with zeroes.
    squad_core::router::recovery::recover_on_open_mode(
        &store,
        OpenMode::OneShot,
        chrono::Utc::now(),
    )
    .unwrap();
    let report = squad_core::router::recovery::recover_on_open(&store).unwrap();
    assert_eq!(report.eligible.len(), 1);
    assert_eq!(report.eligible[0].target, Target::Member(nick("alice")));
    store
        .call(|db| {
            assert_eq!(
                db.query_row("SELECT runnable FROM work WHERE target='bob'", [], |r| r
                    .get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn no_wait_commits_without_waking_even_new_case_work() {
    let (store, router) = fixture();
    let mut runtime = RuntimeHarness::new(router, OpenMode::OneShot, Hooks::default()).unwrap();
    let result = runtime
        .commit_only(message("@alice wait until interactive", EventId::new()))
        .unwrap();
    assert!(matches!(
        result.result,
        RequestResult::Accepted {
            cursor: Some(_),
            ..
        }
    ));
    assert!(runtime.scan().unwrap().is_empty());
    assert!(runtime.hooks().spawned.is_empty());
    drop(runtime);
    let report = squad_core::router::recovery::recover_on_open(&store).unwrap();
    assert_eq!(report.eligible.len(), 1);
    assert_eq!(report.eligible[0].attempt, 0);
}

struct HookRenderer;
impl PromptRenderer for HookRenderer {
    fn render(&self, _: &PromptRef, _: &BTreeMap<String, String>) -> Result<String, ReaderError> {
        Ok("fixture".into())
    }
}
struct SharedHook(std::sync::Arc<std::sync::Mutex<Vec<(CaseId, bool)>>>);
impl squad_core::runtime::contexts::CaseExecutorHook for SharedHook {
    fn spawn(&self, case: CaseId, context: AgentContext) -> Result<(), ReaderError> {
        assert_eq!(context.executor, ExecutorKey::Case(case));
        self.0.lock().unwrap().push((case, true));
        Ok(())
    }
    fn retire(&self, case: CaseId) {
        self.0.lock().unwrap().push((case, false));
    }
}
#[test]
fn shared_case_executor_hook_adapter_constructs_only_case_bound_contexts() {
    let (store, router) = fixture();
    let bus = squad_core::bus::InProcessBus::start(router);
    let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut adapter = squad_core::router::recovery::ScopedCaseExecutors {
        hook: SharedHook(observed.clone()),
        context: move |case| {
            squad_core::runtime::contexts::agent_context(
                store.clone(),
                &bus,
                ExecutorKey::Case(case),
                1,
                std::sync::Arc::new(HookRenderer),
                tokio_util::sync::CancellationToken::new(),
            )
        },
    };
    let case = CaseId::new();
    adapter.spawn(case).unwrap();
    adapter.retire(case);
    assert_eq!(*observed.lock().unwrap(), vec![(case, true), (case, false)]);
}

#[derive(Clone)]
struct TraceEvents(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
impl tracing::Subscriber for TraceEvents {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        struct Visitor(String);
        impl tracing::field::Visit for Visitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(&format!("{}={value:?};", field.name()));
            }
        }
        let mut visitor = Visitor(String::new());
        event.record(&mut visitor);
        self.0.lock().unwrap().push(visitor.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}
#[test]
fn startup_and_router_logs_carry_trace_identities_without_conversation_text() {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = TraceEvents(captured.clone());
    let request_id = EventId::new();
    let work_id;
    tracing::subscriber::set_global_default(subscriber).unwrap();
    {
        let (store, mut router) = fixture();
        squad_core::router::recovery::recover_on_open(&store).unwrap();
        router
            .process(
                message("@alice private-conversation-marker", request_id),
                None,
            )
            .unwrap();
        let id = pending(&store);
        work_id = id;
        router
            .process(
                Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                    work_id: id,
                })),
                Some(ExecutorKey::Member(nick("alice"))),
            )
            .unwrap();
    }
    let logs = captured.lock().unwrap().join("\n");
    assert!(logs.contains("Room recovery committed"), "{logs}");
    assert!(logs.contains(&format!("request_id={request_id}")));
    assert!(logs.contains(&format!("correlation_id={request_id}")));
    assert!(logs.contains(&format!("causation_id={request_id}")));
    assert!(logs.contains(&format!("work_id={work_id}")));
    assert!(!logs.contains("private-conversation-marker"));
}

#[test]
fn releasing_a_hold_during_oneshot_defers_its_wakeup_until_interactive_recovery() {
    let (store, mut router) = fixture();
    router
        .process(message("@alice held work", EventId::new()), None)
        .unwrap();
    let contest = ContestId::new();
    store.call(move |db| {
        db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'debate',1,1,'{}',1)",[contest.to_string()])?;
        db.execute("UPDATE work SET held_by_contest_id=?,runnable=0",[contest.to_string()])?;Ok(())
    }).unwrap();
    squad_core::router::recovery::recover_on_open_mode(
        &store,
        OpenMode::OneShot,
        chrono::Utc::now(),
    )
    .unwrap();
    store
        .call(move |db| {
            let tx = db.transaction()?;
            squad_store::history::release(&tx, contest)?;
            tx.commit()?;
            assert!(squad_store::leases::eligible(db)?.is_empty());
            Ok(())
        })
        .unwrap();
    let report = squad_core::router::recovery::recover_on_open(&store).unwrap();
    assert_eq!(report.eligible.len(), 1);
}

#[test]
fn oneshot_periodic_scan_can_reclaim_its_own_unstarted_expired_lease() {
    let (store, router) = fixture();
    let mut runtime = RuntimeHarness::new(router, OpenMode::OneShot, Hooks::default()).unwrap();
    runtime
        .process(message("@alice own invocation", EventId::new()), None)
        .unwrap();
    let id = runtime.scan().unwrap()[0].work_id;
    let first = claim(&mut runtime, id);
    store
        .call(move |db| {
            db.execute(
                "UPDATE work SET lease_until=? WHERE work_id=?",
                rusqlite::params![
                    (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
                    id.to_string()
                ],
            )?;
            Ok(())
        })
        .unwrap();
    runtime.scan().unwrap();
    let next = claim(&mut runtime, id);
    assert_eq!(next.work.attempt, 1);
    assert!(next.fence > first.fence);
}
