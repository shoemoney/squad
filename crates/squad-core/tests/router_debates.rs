use squad_core::router::{Router, envelope};
use squad_protocol::*;
use squad_store::{Store, StoreError};
fn nick(s: &str) -> MemberNick {
    MemberNick::new(s).unwrap()
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
    let bindings = ["alice", "bob", "judge"]
        .into_iter()
        .map(|n| {
            (
                n.into(),
                ExecutorBinding {
                    model: Text::new("test/model").unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )
        })
        .collect();
    let router = Router::new(store.clone(), roster, bindings, 20_000_000).unwrap();
    (store, router)
}
fn send(router: &mut Router, text: &str) -> squad_store::commit::CommitOutcome {
    let meta = parser::SubmissionMeta::default();
    let parsed = parse(text, &meta, &router.parse_context().unwrap()).unwrap();
    router
        .process(
            Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap(),
            None,
        )
        .unwrap()
}
fn prepared(router: &mut Router) -> DebateState {
    send(router, "@alice @bob $debate $ROUNDS=2 compare approaches")
        .events
        .into_iter()
        .find_map(|e| {
            if let EnvelopePayload::DebateState(s) = e.content {
                Some(s)
            } else {
                None
            }
        })
        .unwrap()
}
fn plan(state: &DebateState, cost: f64) -> DebatePreflight {
    DebatePreflight {
        judge_only: false,
        debate_id: state.debate_id,
        plan_id: PlanId::new(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        estimate: DebateEstimate {
            estimated_usd: cost,
            reserved_usd: cost,
            remaining_usd: 20.0,
            model_calls: 6,
        },
        judge_model: Some("test/model".into()),
        warning: None,
        confirmation_required: true,
        debaters: state.debaters.clone(),
        rounds: 2,
        judge_requested: false,
        opening_round_required: true,
    }
}
#[test]
fn pending_admission_cancel_and_receipts_are_atomic() {
    let (store, mut router) = fixture();
    let state = prepared(&mut router);
    assert_eq!(
        store
            .call(|db| Ok(db.query_row("SELECT COUNT(*) FROM work", [], |r| r.get::<_, i64>(0))?))
            .unwrap(),
        0
    );
    let preflight = plan(&state, 3.0);
    let proposed = envelope(
        EventId::new(),
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::DebatePreflight(preflight.clone()),
    );
    let out = router.produce(proposed.clone()).unwrap();
    assert!(matches!(
        out.result,
        RequestResult::PendingConfirmation { .. }
    ));
    assert!(router.produce(proposed).unwrap().duplicate);
    let request = Request::DebateCancel {
        request_id: EventId::new(),
        debate_id: state.debate_id,
        plan_id: preflight.plan_id,
    };
    router.process(request.clone(), None).unwrap();
    assert!(router.process(request, None).unwrap().duplicate);
    assert_eq!(
        router.debate_state(state.debate_id).unwrap().state,
        DebateStatus::Cancelled
    );
    assert_eq!(
        store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM events WHERE kind='user.message'",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0
    );
}
#[test]
fn auto_confirm_commits_command_without_ordinary_work_and_stop_releases_claim() {
    let (store, mut router) = fixture();
    let state = prepared(&mut router);
    let plan = plan(&state, 1.0);
    router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan),
        ))
        .unwrap();
    assert_eq!(
        router.debate_state(state.debate_id).unwrap().state,
        DebateStatus::WaitingForDebaters
    );
    assert_eq!(
        store
            .call(|db| Ok((
                db.query_row(
                    "SELECT COUNT(*) FROM events WHERE kind='user.message'",
                    [],
                    |r| r.get::<_, i64>(0)
                )?,
                db.query_row("SELECT COUNT(*) FROM work", [], |r| r.get::<_, i64>(0))?
            )))
            .unwrap(),
        (1, 0)
    );
    router
        .process(
            Request::Stop {
                request_id: EventId::new(),
                target: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(
        router.debate_state(state.debate_id).unwrap().state,
        DebateStatus::Stopped
    );
    prepared(&mut router);
}
#[test]
fn stale_plan_and_expiry_cannot_admit_command() {
    let (store, mut router) = fixture();
    let state = prepared(&mut router);
    let plan = plan(&state, 3.0);
    router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan.clone()),
        ))
        .unwrap();
    assert!(matches!(
        router.process(
            Request::DebateConfirm {
                request_id: EventId::new(),
                debate_id: state.debate_id,
                plan_id: PlanId::new()
            },
            None
        ),
        Err(StoreError::Domain(ErrorCode::DEBATE_PLAN_STALE, _))
    ));
    let contest = state.debate_id;
    store.call(move|db|{db.execute("UPDATE contests SET plan_json=json_set(plan_json,'$.expiresAt','2000-01-01T00:00:00Z') WHERE contest_id=?",[contest.to_string()])?;Ok(())}).unwrap();
    let result = router
        .process(
            Request::DebateConfirm {
                request_id: EventId::new(),
                debate_id: state.debate_id,
                plan_id: plan.plan_id,
            },
            None,
        )
        .unwrap();
    assert!(result.events.iter().any(|e|matches!(&e.content,EnvelopePayload::DebatePreflightResult(p) if p.status==PreflightStatus::Expired)));
    assert_eq!(
        router.debate_state(contest).unwrap().state,
        DebateStatus::Expired
    );
}
#[test]
fn version_and_evidence_gates_reject_forged_progression() {
    let (_, mut router) = fixture();
    let state = prepared(&mut router);
    let sub = ContestTransitionSubmission {
        contest_id: state.debate_id,
        kind: ContestKind::Debate,
        expected_state_version: state.state_version,
        transition: ContestTransition::Debate(DebateTransition::VerdictAccepted {
            verdict_event_id: EventId::new(),
        }),
    };
    assert!(matches!(
        router.transition(EventId::new(), sub),
        Err(StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION, _))
    ));
    let sub = ContestTransitionSubmission {
        contest_id: state.debate_id,
        kind: ContestKind::Debate,
        expected_state_version: 0,
        transition: ContestTransition::Debate(DebateTransition::Prepare),
    };
    assert!(router.transition(EventId::new(), sub).is_err());
    assert_eq!(router.debate_state(state.debate_id).unwrap(), state);
}
#[tokio::test]
async fn question_collection_stays_private_and_stop_cancels_pick_claim() {
    let (store, mut router) = fixture();
    let out = send(&mut router, "@all $debate which approach?");
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::ContestCollecting(_)))
    );
    for n in ["alice", "bob"] {
        let owner = ExecutorKey::Member(nick(n));
        let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
        let work = reader.unfinished().await.unwrap().remove(0);
        router
            .process(
                Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                    work_id: work.work_id,
                })),
                Some(owner),
            )
            .unwrap();
        let history = reader.for_work(work.work_id).await.unwrap();
        assert_eq!(history.len(), 1);
        assert!(history[0].content.contains("which approach?"));
    }
    router
        .process(
            Request::Stop {
                request_id: EventId::new(),
                target: None,
            },
            None,
        )
        .unwrap();
    assert!(router.parse_context().unwrap().active_contest.is_none());
}
#[test]
fn barrier_rejects_metadata_forgery_without_partial_delivery() {
    let (store, mut router) = fixture();
    let state = prepared(&mut router);
    let step = ContestStep {
        contest_id: state.debate_id,
        step_id: StepId::new(),
        ordinal: 1,
        script_version: Text::new("fixture.v1").unwrap(),
        move_name: ContestMove::Barrier,
        inputs: vec![],
        assignments: vec![],
    };
    let out = router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestStep(step.clone()),
        ))
        .unwrap();
    let source = &out.events[0];
    let projection = HistoryProjection {
        context_id: ContextId("alice".into()),
        origin_event_id: source.base.id,
        projection_key: Text::new("leak").unwrap(),
        role: HistoryRole::User,
        name: None,
        content: "hidden material".into(),
        meta: HistoryMetadata::Anonymous { anonymous: True },
    };
    let report = ContestReport {
        contest_id: state.debate_id,
        step_id: step.step_id,
        artifact_event_ids: vec![],
        skipped_work_ids: vec![],
        data: serde_json::json!({}),
    };
    let commit = BarrierCommit {
        contest_id: state.debate_id,
        step_id: step.step_id,
        expected_state_version: state.state_version,
        barrier_event: envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestReport(report),
        ),
        projections: vec![projection],
        next_work: vec![],
    };
    assert!(router.barrier(EventId::new(), commit).is_err());
    assert_eq!(
        store
            .call(|db| Ok((
                db.query_row("SELECT SUM(committed) FROM contest_steps", [], |r| r
                    .get::<_, i64>(0))?,
                db.query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))?
            )))
            .unwrap(),
        (0, 0)
    );
}
#[test]
fn oversized_preflight_cannot_bypass_daily_cap() {
    let (_, mut router) = fixture();
    let state = prepared(&mut router);
    let plan = plan(&state, 21.0);
    let out = router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan),
        ))
        .unwrap();
    assert!(matches!(
        out.result,
        RequestResult::Rejected {
            error: ErrorPayload {
                code: ErrorCode::DAILY_MAX_REACHED,
                ..
            },
            ..
        }
    ));
    assert_eq!(
        router.debate_state(state.debate_id).unwrap().state,
        DebateStatus::Rejected
    );
}

async fn complete_reply(store: &Store, router: &mut Router, name: &str) {
    let owner = ExecutorKey::Member(nick(name));
    let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
    let record = reader.unfinished().await.unwrap().remove(0);
    let out = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: record.work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = out.result else {
        panic!("claim")
    };
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                input_tokens: None,
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
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: Some(0.01),
            }),
        },
        in_reply_to_message_id: grant.work.in_reply_to_message_id.unwrap(),
        content: format!("{name} private solution"),
    };
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                work_id: record.work_id,
                attempt: 1,
                fence: grant.fence,
                epoch: 1,
                output: EnvelopePayload::AgentReply(reply),
            })),
            Some(owner),
        )
        .unwrap();
}
#[tokio::test]
async fn completed_collection_freezes_answers_and_picks_once() {
    let (store, mut router) = fixture();
    send(&mut router, "@all $debate which approach?");
    complete_reply(&store, &mut router, "alice").await;
    assert!(router.parse_context().unwrap().pending_pick.is_none());
    complete_reply(&store, &mut router, "bob").await;
    let pending = router.parse_context().unwrap().pending_pick.unwrap();
    assert_eq!(pending.eligible, vec![nick("alice"), nick("bob")]);
    let invalid = Request::ContestPick {
        request_id: EventId::new(),
        contest_id: pending.contest_id,
        nicks: vec![nick("alice")],
    };
    let result = router.process(invalid, None).unwrap();
    let RequestResult::Rejected { error, .. } = result.result else {
        panic!("invalid pick")
    };
    assert_eq!(error.code, ErrorCode::CONTEST_PICK_INVALID);
    assert_eq!(error.text.as_str(), "Pick exactly two of: @alice, @bob.");
    assert!(router.parse_context().unwrap().pending_pick.is_some());
    let request = Request::ContestPick {
        request_id: EventId::new(),
        contest_id: pending.contest_id,
        nicks: pending.eligible,
    };
    let out = router.process(request.clone(), None).unwrap();
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::ContestPicked(_)))
    );
    assert!(router.process(request, None).unwrap().duplicate);
    assert_eq!(
        router.debate_state(pending.contest_id).unwrap().state,
        DebateStatus::Preparing
    );
    let history = store
        .call(|db| {
            squad_store::history::read(
                db,
                &ContextId("alice".into()),
                HistoryPosition::new(100).unwrap(),
            )
        })
        .unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.content == "alice private solution")
    );
    assert!(
        !history
            .iter()
            .any(|e| e.content.contains("bob private solution"))
    );
}

fn score_fixture(store: &Store) -> (DebateState, EventId) {
    let contest = ContestId::new();
    let case = CaseId::new();
    let trigger = EventId::new();
    let state = DebateState {
        debate_id: contest,
        state: DebateStatus::Judging,
        debaters: [nick("alice"), nick("bob")],
        round: 2,
        total_rounds: 2,
        judge_model: "test/model".into(),
        judge_requested: true,
        state_version: 1,
        terminal_reason: None,
        case_id: Some(case),
        case_ids: vec![case],
    };
    let saved = state.clone();
    store
        .commit(
            trigger,
            "fixture".into(),
            "fixture".into(),
            move |tx, group| {
                let mut event = envelope(
                    trigger,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::DebateState(saved.clone()),
                );
                event.base.id = trigger;
                let seq = group.push(tx, event)?;
                squad_store::contests::save_debate(tx, &saved, None, seq)?;
                let opened = CaseOpened {
                    case_id: case,
                    judge_invocation_id: trigger,
                    work_id: None,
                    judge_model: Text::new("test/model").unwrap(),
                    material: CaseMaterial::Standalone {
                        messages: vec![],
                        question: Text::new("fixture case").unwrap(),
                    },
                    debate_id: Some(contest),
                    contest_id: Some(contest),
                };
                // The runtime case material itself is not under test in this fixture.
                squad_store::cases::open(tx, &opened, seq)?;
                tx.execute(
                    "UPDATE cases SET contest_id=? WHERE case_id=?",
                    rusqlite::params![contest.to_string(), case.to_string()],
                )?;
                let source = squad_store::events::at(tx, seq.get())?;
                squad_store::history::project(
                    tx,
                    &squad_store::history::attributed(
                        &source,
                        ContextId(format!("judge:{case}")),
                        "fixture.case",
                        HistoryRole::User,
                        None,
                        "case input".into(),
                    ),
                )?;
                Ok(group.accepted(trigger))
            },
        )
        .unwrap();
    (state, trigger)
}
fn judge_work(state: &DebateState, trigger: EventId, round: Option<u32>) -> Work {
    Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: if round.is_some() {
            WorkKind::JudgeRound
        } else {
            WorkKind::Judge
        },
        target: Target::Judge,
        context_id: ContextId(format!("judge:{}", state.case_id.unwrap())),
        trigger_event_id: trigger,
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: Some(state.debate_id),
        round,
        replay: None,
        case_id: state.case_id,
        prompt_ref: round.map(|_| PromptRef {
            template_id: Text::new("judge.round.md").unwrap(),
            pack_version: Text::new("fixture.v1").unwrap(),
            template_hash: Text::new("fixture.hash").unwrap(),
        }),
        max_output_tokens: round.map(|_| 512),
        contest_id: Some(state.debate_id),
        step_id: None,
    }
}
fn start(router: &mut Router, work: WorkId, case: CaseId) -> ExecutionGrant {
    let owner = ExecutorKey::Case(case);
    let out = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: work,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = out.result else {
        panic!("claim")
    };
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                input_tokens: None,
                work_id: work,
                attempt: grant.work.attempt,
                fence: grant.fence,
            })),
            Some(owner),
        )
        .unwrap();
    grant
}
#[test]
fn out_of_order_round_scores_stay_private_then_verdict_delivers_tallies_once() {
    let (store, mut router) = fixture();
    let (state, trigger) = score_fixture(&store);
    for (round, a, b) in [(2, 3, 7), (1, 9, 6)] {
        let work = judge_work(&state, trigger, Some(round));
        router
            .allocate_work(EventId::new(), state.debate_id, 1, vec![work.clone()])
            .unwrap();
        let grant = start(&mut router, work.work_id, state.case_id.unwrap());
        let score = DebateRoundScore {
            debate_id: state.debate_id,
            case_id: state.case_id.unwrap(),
            round,
            scores: Some(
                [
                    (
                        nick("alice"),
                        RoundScore {
                            score: a,
                            reason: "Clear reasoning.".into(),
                        },
                    ),
                    (
                        nick("bob"),
                        RoundScore {
                            score: b,
                            reason: "Useful evidence.".into(),
                        },
                    ),
                ]
                .into_iter()
                .collect(),
            ),
            tally: [(nick("alice"), 999), (nick("bob"), 999)]
                .into_iter()
                .collect(),
            reason: None,
        };
        let request = Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
            work_id: work.work_id,
            attempt: 1,
            fence: grant.fence,
            epoch: 1,
            output: EnvelopePayload::DebateRoundScore(score),
        }));
        let out = router
            .process(
                request.clone(),
                Some(ExecutorKey::Case(state.case_id.unwrap())),
            )
            .unwrap();
        assert_eq!(out.events[0].base.recipients, vec![Recipient::Me]);
        assert!(
            router
                .process(request, Some(ExecutorKey::Case(state.case_id.unwrap())))
                .unwrap()
                .duplicate
        );
    }
    let contest = state.debate_id;
    let tallies = store
        .call(move |db| {
            squad_store::round_scores::tally(db, &squad_store::contests::debate(db, contest)?)
        })
        .unwrap();
    assert_eq!(tallies[&nick("alice")], 12);
    assert_eq!(tallies[&nick("bob")], 13);
    assert_eq!(
        store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM history WHERE context_id IN ('alice','bob')",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0
    );
    let work = judge_work(&state, trigger, None);
    router
        .allocate_work(EventId::new(), state.debate_id, 1, vec![work.clone()])
        .unwrap();
    let grant = start(&mut router, work.work_id, state.case_id.unwrap());
    let criteria = |score| {
        [
            CriterionId::Fit,
            CriterionId::Feasibility,
            CriterionId::Originality,
            CriterionId::SurvivedAttacks,
        ]
        .into_iter()
        .map(|criterion_id| CriterionScore {
            criterion_id,
            score,
            reasoning: "Sound.".into(),
        })
        .collect()
    };
    let verdict = DebateVerdict {
        completed: CompletedGeneration {
            generation: GenerationIdentity {
                message_id: work.message_id,
                work_id: work.work_id,
                trigger_event_id: trigger,
                attempt: 1,
            },
            model: grant.binding.model,
            finish_reason: FinishReason::Stop,
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: Some(0.01),
            }),
        },
        debate_id: state.debate_id,
        case_id: state.case_id.unwrap(),
        judge_invocation_id: trigger,
        rubric_version: Text::new("fixture.v1").unwrap(),
        verdict: Verdict {
            verdict: JudgeVerdict {
                winner: nick("bob"),
                scores: vec![
                    JudgeScore {
                        nick: nick("alice"),
                        criteria: criteria(6),
                    },
                    JudgeScore {
                        nick: nick("bob"),
                        criteria: criteria(8),
                    },
                ],
                reasoning: "Bob wins.".into(),
                what_to_steal_from_loser: vec!["Clarity.".into()],
                tally_override_reason: None,
            },
            weighted_totals: vec![],
            round_tally: Default::default(),
        },
    };
    let request = Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
        work_id: work.work_id,
        attempt: 1,
        fence: grant.fence,
        epoch: 1,
        output: EnvelopePayload::Verdict(verdict),
    }));
    let out = router
        .process(
            request.clone(),
            Some(ExecutorKey::Case(state.case_id.unwrap())),
        )
        .unwrap();
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::ScoresChanged(_)))
    );
    assert!(
        router
            .process(request, Some(ExecutorKey::Case(state.case_id.unwrap())))
            .unwrap()
            .duplicate
    );
    assert_eq!(
        router.debate_state(state.debate_id).unwrap().state,
        DebateStatus::Completed
    );
    for name in ["alice", "bob"] {
        let name = name.to_owned();
        let history = store
            .call(move |db| {
                squad_store::history::read(db, &ContextId(name), HistoryPosition::new(100).unwrap())
            })
            .unwrap();
        assert_eq!(history.len(), 3);
    }
    assert_eq!(
        store
            .call(|db| Ok(db
                .query_row("SELECT wins FROM scores WHERE nick='bob'", [], |r| r
                    .get::<_, i64>(0))?))
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn real_step_reader_rejects_foreign_work_and_ordinary_history_substitution() {
    let (store, mut router) = fixture();
    let ordinary = send(&mut router, "@alice ordinary private input").events[0].clone();
    let state = prepared(&mut router);
    router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan(&state, 1.0)),
        ))
        .unwrap();
    let step_id = StepId::new();
    let work_id = WorkId::new();
    let context = ContextId(format!("contest:{}:{step_id}:alice", state.debate_id));
    let step = ContestStep {
        contest_id: state.debate_id,
        step_id,
        ordinal: 1,
        script_version: Text::new("fixture.v1").unwrap(),
        move_name: ContestMove::Fanout,
        inputs: vec![ordinary.base.id],
        assignments: vec![ContestAssignment {
            target: Target::Member(nick("alice")),
            context_id: context.clone(),
            payload_refs: vec![ordinary.base.id],
            work_id: Some(work_id),
        }],
    };
    let work = Work {
        work_id,
        message_id: MessageId::new(),
        in_reply_to_message_id: Some(MessageId::from_event(ordinary.base.id)),
        kind: WorkKind::Reply,
        target: Target::Member(nick("alice")),
        context_id: context.clone(),
        trigger_event_id: ordinary.base.id,
        history_through: HistoryPosition::new(100).unwrap(),
        attempt: 1,
        debate_id: None,
        round: None,
        replay: None,
        case_id: None,
        prompt_ref: None,
        max_output_tokens: None,
        contest_id: Some(state.debate_id),
        step_id: Some(step_id),
    };
    let projection = HistoryProjection {
        context_id: context,
        origin_event_id: ordinary.base.id,
        projection_key: Text::new("blind.input").unwrap(),
        role: HistoryRole::User,
        name: None,
        content: "anonymous assigned bundle".into(),
        meta: HistoryMetadata::Anonymous { anonymous: True },
    };
    let mut forged = work.clone();
    forged.context_id = ContextId("alice".into());
    assert!(
        router
            .allocate_step(
                EventId::new(),
                step.clone(),
                vec![projection.clone()],
                vec![forged],
                Default::default()
            )
            .is_err()
    );
    router
        .allocate_step(
            EventId::new(),
            step,
            vec![projection],
            vec![work],
            Default::default(),
        )
        .unwrap();
    let owner = ExecutorKey::Member(nick("alice"));
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let alice = squad_store::readers::ScopedReaders::new(store.clone(), owner, 1);
    let history = alice.for_work(work_id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, "anonymous assigned bundle");
    assert_eq!(
        serde_json::to_value(&history[0].meta).unwrap(),
        serde_json::json!({"anonymous":true})
    );
    let bob = squad_store::readers::ScopedReaders::new(store, ExecutorKey::Member(nick("bob")), 1);
    assert_eq!(bob.get(work_id).await.unwrap_err(), ReaderError::Forbidden);
    assert_eq!(
        bob.for_work(work_id).await.unwrap_err(),
        ReaderError::Forbidden
    );
}
#[test]
fn judge_only_cancel_restores_terminal_state_and_retains_cases() {
    let (_, mut router) = fixture();
    let state = prepared(&mut router);
    router
        .process(
            Request::Stop {
                request_id: EventId::new(),
                target: None,
            },
            None,
        )
        .unwrap();
    let prior = router.debate_state(state.debate_id).unwrap();
    router
        .process(
            Request::Judge {
                request_id: EventId::new(),
                debate_id: Some(state.debate_id),
            },
            None,
        )
        .unwrap();
    let mut plan = plan(&prior, 3.0);
    plan.judge_only = true;
    plan.judge_requested = true;
    plan.estimate.model_calls = 1;
    router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan.clone()),
        ))
        .unwrap();
    router
        .process(
            Request::DebateCancel {
                request_id: EventId::new(),
                debate_id: state.debate_id,
                plan_id: plan.plan_id,
            },
            None,
        )
        .unwrap();
    let restored = router.debate_state(state.debate_id).unwrap();
    assert_eq!(restored.state, prior.state);
    assert_eq!(restored.case_ids, prior.case_ids);
    assert!(restored.state_version > prior.state_version);
}

#[test]
fn exhausted_round_is_unscored_and_does_not_add_to_tally() {
    let (store, mut router) = fixture();
    let (state, trigger) = score_fixture(&store);
    let work = judge_work(&state, trigger, Some(1));
    router
        .allocate_work(EventId::new(), state.debate_id, 1, vec![work.clone()])
        .unwrap();
    let work_id = work.work_id;
    store
        .call(move |db| {
            db.execute(
                "UPDATE work SET attempt=2,status='failed' WHERE work_id=?",
                [work_id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let score = DebateRoundScore {
        debate_id: state.debate_id,
        case_id: state.case_id.unwrap(),
        round: 1,
        scores: None,
        tally: Default::default(),
        reason: Some(Text::new("unscored").unwrap()),
    };
    let event = envelope(
        EventId::new(),
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::DebateRoundScore(score),
    );
    let out = router.produce(event.clone()).unwrap();
    let EnvelopePayload::DebateRoundScore(score) = &out.events[0].content else {
        panic!("score")
    };
    assert_eq!(score.tally.values().sum::<u32>(), 0);
    assert!(router.produce(event).unwrap().duplicate);
    let history = store
        .call(move |db| {
            squad_store::history::read(
                db,
                &ContextId(format!("judge:{}", state.case_id.unwrap())),
                HistoryPosition::new(100).unwrap(),
            )
        })
        .unwrap();
    assert_eq!(history.len(), 2);
}
#[test]
fn case_bundle_is_case_bound_and_retains_all_case_ids() {
    let (store, mut router) = fixture();
    let (state, trigger) = score_fixture(&store);
    let case = CaseId::new();
    let setup = DebateSetup {
        debate_id: state.debate_id,
        topic_message_id: MessageId::from_event(trigger),
        topic_text: "Frozen topic".into(),
        solutions: [
            DebateSolution {
                nick: nick("alice"),
                message_id: MessageId::new(),
                text: "A solution".into(),
            },
            DebateSolution {
                nick: nick("bob"),
                message_id: MessageId::new(),
                text: "B solution".into(),
            },
        ],
        instruction: "Compare".into(),
        rounds: 2,
        judge_model: "test/model".into(),
        judge_swapped: false,
    };
    let bundle = DebateAdjudicationBundle {
        topic: AdjudicationTopic {
            message_id: setup.topic_message_id,
            text: setup.topic_text.clone(),
        },
        solutions: setup.solutions.clone(),
        transcript: vec![],
        instruction: setup.instruction.clone(),
        rubric: FrozenDebateRubric {
            version: "v1".into(),
            criteria: vec![],
            verdict_schema: serde_json::json!({}),
        },
        judge: JudgeProvenance {
            model: "test/model".into(),
            prompt_pack_version: "v1".into(),
            rendered_template_hashes: Default::default(),
        },
        round_scoring: DebateScoringEvidence {
            round_scores: vec![],
            round_tally: [(nick("alice"), 0), (nick("bob"), 0)].into_iter().collect(),
            unscored_rounds: vec![],
        },
    };
    let opened = CaseOpened {
        case_id: case,
        judge_invocation_id: EventId::new(),
        work_id: None,
        judge_model: Text::new("test/model").unwrap(),
        material: CaseMaterial::Debate {
            setup,
            rubric_version: Text::new("v1").unwrap(),
        },
        debate_id: Some(state.debate_id),
        contest_id: Some(state.debate_id),
    };
    let event = envelope(
        EventId::new(),
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::CaseOpened(opened),
    );
    router
        .open_debate_case(event.clone(), bundle.clone())
        .unwrap();
    assert!(router.open_debate_case(event, bundle).unwrap().duplicate);
    let latest = router.debate_state(state.debate_id).unwrap();
    assert_eq!(latest.case_ids, vec![state.case_id.unwrap(), case]);
    let rows=store.call(move|db|Ok((db.query_row("SELECT COUNT(*) FROM history WHERE context_id=? AND projection_key='case.bundle'",[format!("judge:{case}")],|r|r.get::<_,i64>(0))?,db.query_row("SELECT COUNT(*) FROM history WHERE context_id IN ('alice','bob')",[],|r|r.get::<_,i64>(0))?))).unwrap();
    assert_eq!(rows, (1, 0));
}

#[tokio::test]
async fn case_followup_keeps_the_original_decision_sealed() {
    let (store, mut router) = fixture();
    let source = send(&mut router, "@alice source material").events[0]
        .base
        .id;
    let meta = parser::SubmissionMeta {
        quote: vec![source],
        reply_to: vec![],
    };
    let parsed = parse(
        "@judge assess this",
        &meta,
        &router.parse_context().unwrap(),
    )
    .unwrap();
    let out = router
        .process(
            Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap(),
            None,
        )
        .unwrap();
    let opened = out
        .events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::CaseOpened(c) = &e.content {
                Some(c.clone())
            } else {
                None
            }
        })
        .unwrap();
    let reader = squad_store::readers::ScopedReaders::new(
        store.clone(),
        ExecutorKey::Case(opened.case_id),
        1,
    );
    let work = reader.unfinished().await.unwrap().remove(0);
    let grant = start(&mut router, work.work_id, opened.case_id);
    let ruling = CaseRuling {
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
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: Some(0.01),
            }),
        },
        case_id: opened.case_id,
        judge_invocation_id: opened.judge_invocation_id,
        in_reply_to_event_id: grant.work.trigger_event_id,
        content: Text::new("Accepted decision.").unwrap(),
    };
    let out = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                work_id: work.work_id,
                attempt: 1,
                fence: grant.fence,
                epoch: 1,
                output: EnvelopePayload::CaseRuling(ruling),
            })),
            Some(ExecutorKey::Case(opened.case_id)),
        )
        .unwrap();
    let decision = out.events[0].clone();
    let meta = parser::SubmissionMeta {
        quote: vec![decision.base.id],
        reply_to: vec![],
    };
    let parsed = parse(
        "@judge explain further",
        &meta,
        &router.parse_context().unwrap(),
    )
    .unwrap();
    let out = router
        .process(
            Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap(),
            None,
        )
        .unwrap();
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::CaseFollowup(_)))
    );
    assert!(
        !out.events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::CaseOpened(_)))
    );
    let case = opened.case_id;
    assert_eq!(
        store
            .call(move |db| Ok(db.query_row(
                "SELECT decision_event_seq FROM cases WHERE case_id=?",
                [case.to_string()],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        decision.commit.seq.get()
    );
}
#[test]
fn generic_preflight_outcome_is_durable_without_admitting_unbuilt_mode() {
    let (store, mut router) = fixture();
    let plan = ContestPreflight {
        contest_id: ContestId::new(),
        kind: ContestKind::Grill,
        plan_id: PlanId::new(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        estimate: DebateEstimate {
            estimated_usd: 1.0,
            reserved_usd: 2.0,
            remaining_usd: 20.0,
            model_calls: 1,
        },
        participants: vec![nick("alice"), nick("bob")],
        confirmation_required: false,
        warning: None,
        settings: serde_json::json!({}),
    };
    let event = envelope(
        EventId::new(),
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestPreflight(plan),
    );
    let out = router.produce(event.clone()).unwrap();
    assert!(out.events.iter().any(|e|matches!(&e.content,EnvelopePayload::ContestPreflightResult(r) if r.status==PreflightStatus::Rejected)));
    assert!(router.produce(event).unwrap().duplicate);
    assert_eq!(
        store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM contests WHERE active=1",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        0
    );
}

#[test]
fn failed_round_score_transaction_keeps_work_and_tally_unchanged() {
    let (store, mut router) = fixture();
    let (state, trigger) = score_fixture(&store);
    let work = judge_work(&state, trigger, Some(1));
    router
        .allocate_work(EventId::new(), state.debate_id, 1, vec![work.clone()])
        .unwrap();
    let grant = start(&mut router, work.work_id, state.case_id.unwrap());
    store.call(|db|{db.execute_batch("CREATE TRIGGER reject_score BEFORE INSERT ON round_scores BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;")?;Ok(())}).unwrap();
    let score = DebateRoundScore {
        debate_id: state.debate_id,
        case_id: state.case_id.unwrap(),
        round: 1,
        scores: Some(
            [
                (
                    nick("alice"),
                    RoundScore {
                        score: 5,
                        reason: "Clear.".into(),
                    },
                ),
                (
                    nick("bob"),
                    RoundScore {
                        score: 5,
                        reason: "Clear.".into(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
        ),
        tally: Default::default(),
        reason: None,
    };
    let result = router.process(
        Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
            work_id: work.work_id,
            attempt: 1,
            fence: grant.fence,
            epoch: 1,
            output: EnvelopePayload::DebateRoundScore(score),
        })),
        Some(ExecutorKey::Case(state.case_id.unwrap())),
    );
    assert!(result.is_err());
    let work_id = work.work_id;
    let status = store
        .call(move |db| Ok(squad_store::work::record(db, work_id)?.unwrap().status))
        .unwrap();
    assert_eq!(status, WorkStatus::Running);
    let counts = store
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM round_scores", [], |r| {
                    r.get::<_, i64>(0)
                })?,
                db.query_row(
                    "SELECT COUNT(*) FROM events WHERE kind='debate.round_score'",
                    [],
                    |r| r.get::<_, i64>(0),
                )?,
            ))
        })
        .unwrap();
    assert_eq!(counts, (0, 0));
}
#[test]
fn future_history_boundary_cannot_authorize_later_messages() {
    let (store, mut router) = fixture();
    let (state, trigger) = score_fixture(&store);
    let mut work = judge_work(&state, trigger, Some(1));
    work.history_through = HistoryPosition::new(100).unwrap();
    assert!(
        router
            .allocate_work(EventId::new(), state.debate_id, 1, vec![work])
            .is_err()
    );
}

#[tokio::test]
async fn plan_reservation_metadata_and_frozen_turn_inputs_survive_mid_round_messages() {
    let (store, mut router) = fixture();
    let state = prepared(&mut router);
    let plan = plan(&state, 1.0);
    let plan_id = plan.plan_id;
    let admitted = router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan),
        ))
        .unwrap();
    let topic = admitted
        .events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::UserMessage(m) = &e.content {
                Some(m.input.message_id)
            } else {
                None
            }
        })
        .unwrap();
    let opening = DebateOpening {
        debate_id: state.debate_id,
        debaters: state.debaters.clone(),
        topic_message_id: topic,
        topic_text: Text::new("Compare approaches").unwrap(),
        instruction: "Compare approaches".into(),
        rounds: 2,
        round: 0,
    };
    let opened = router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebateOpening(opening),
        ))
        .unwrap();
    let trigger = opened.events[0].base.id;
    let ready = router.debate_state(state.debate_id).unwrap();
    router
        .transition(
            EventId::new(),
            ContestTransitionSubmission {
                contest_id: state.debate_id,
                kind: ContestKind::Debate,
                expected_state_version: ready.state_version,
                transition: ContestTransition::Debate(DebateTransition::DebatersReady),
            },
        )
        .unwrap();
    let running = router.debate_state(state.debate_id).unwrap();
    let work = state
        .debaters
        .iter()
        .enumerate()
        .map(|(i, n)| Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: WorkKind::DebateTurn,
            target: Target::Member(n.clone()),
            context_id: ContextId(n.to_string()),
            trigger_event_id: trigger,
            history_through: HistoryPosition::new(i as i64 + 1).unwrap(),
            attempt: 1,
            debate_id: Some(state.debate_id),
            round: Some(0),
            replay: None,
            case_id: None,
            prompt_ref: None,
            max_output_tokens: None,
            contest_id: Some(state.debate_id),
            step_id: None,
        })
        .collect::<Vec<_>>();
    let alice_work = work[0].clone();
    router
        .allocate_work(EventId::new(), state.debate_id, running.state_version, work)
        .unwrap();
    let work_id = alice_work.work_id;
    let meta = store
        .call(move |db| {
            Ok(db.query_row(
                "SELECT scheduling_meta FROM work WHERE work_id=?",
                [work_id.to_string()],
                |r| r.get::<_, String>(0),
            )?)
        })
        .unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta).unwrap();
    assert_eq!(meta["planId"], plan_id.to_string());
    assert_eq!(meta["attemptReservationMicros"], 83_333);
    send(&mut router, "@alice mid-round private message");
    let owner = ExecutorKey::Member(nick("alice"));
    let out = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = out.result else {
        panic!("claim turn")
    };
    let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
    let history = reader.for_work(work_id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert!(!history[0].content.contains("mid-round"));
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                input_tokens: None,
                work_id,
                attempt: 1,
                fence: grant.fence,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let turn = DebateTurn {
        generation: GenerationIdentity {
            message_id: alice_work.message_id,
            work_id,
            trigger_event_id: trigger,
            attempt: 1,
        },
        debate_id: state.debate_id,
        round: 0,
        speaker: nick("alice"),
        outcome: DebateTurnOutcome::Completed {
            content: "Private opening".into(),
            model: grant.binding.model,
            finish_reason: FinishReason::Stop,
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 1,
                cost_usd: Some(0.01),
            }),
        },
    };
    let outcome = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                work_id,
                attempt: 1,
                fence: grant.fence,
                epoch: 1,
                output: EnvelopePayload::DebateTurn(turn),
            })),
            Some(owner),
        )
        .unwrap();
    let turn_id = outcome.events[0].base.id;
    assert!(matches!(
        store.call(move |db| squad_core::router::references::quotes(db, &[turn_id])),
        Err(StoreError::Domain(ErrorCode::INVALID_REFERENCE, _))
    ));
}
#[test]
fn resolved_barrier_is_committed_exactly_once() {
    let (store, mut router) = fixture();
    let state = prepared(&mut router);
    let step = ContestStep {
        contest_id: state.debate_id,
        step_id: StepId::new(),
        ordinal: 1,
        script_version: Text::new("fixture.v1").unwrap(),
        move_name: ContestMove::Barrier,
        inputs: vec![],
        assignments: vec![],
    };
    router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestStep(step.clone()),
        ))
        .unwrap();
    let commit = BarrierCommit {
        contest_id: state.debate_id,
        step_id: step.step_id,
        expected_state_version: state.state_version,
        barrier_event: envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestReport(ContestReport {
                contest_id: state.debate_id,
                step_id: step.step_id,
                artifact_event_ids: vec![],
                skipped_work_ids: vec![],
                data: serde_json::json!({}),
            }),
        ),
        projections: vec![],
        next_work: vec![],
    };
    let id = EventId::new();
    router.barrier(id, commit.clone()).unwrap();
    assert!(router.barrier(id, commit).unwrap().duplicate);
    assert_eq!(
        store
            .call(|db| Ok(db
                .query_row("SELECT SUM(committed) FROM contest_steps", [], |r| r
                    .get::<_, i64>(0))?))
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn regeneration_supersedes_atomically_and_retry_has_a_fresh_identity() {
    let (store, mut router) = fixture();
    send(&mut router, "@alice original input");
    complete_reply(&store, &mut router, "alice").await;
    let request = Request::Regen {
        request_id: EventId::new(),
        message_id: None,
    };
    let out = router.process(request.clone(), None).unwrap();
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::MessageSuperseded(_)))
    );
    assert!(router.process(request, None).unwrap().duplicate);
    let owner = ExecutorKey::Member(nick("alice"));
    let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner, 1);
    let work = reader.unfinished().await.unwrap().remove(0);
    let failed = work.work_id;
    store
        .call(move |db| {
            db.execute(
                "UPDATE work SET status='failed',runnable=0 WHERE work_id=?",
                [failed.to_string()],
            )?;
            Ok(())
        })
        .unwrap();
    let request = Request::Retry {
        request_id: EventId::new(),
        work_id: failed,
    };
    router.process(request.clone(), None).unwrap();
    assert!(router.process(request, None).unwrap().duplicate);
    let retried = reader.unfinished().await.unwrap().remove(0);
    assert_ne!(retried.work_id, failed);
    assert_eq!(retried.in_reply_to_message_id, work.in_reply_to_message_id);
    let counts = store
        .call(|db| {
            Ok((
                db.query_row("SELECT COUNT(*) FROM superseded", [], |r| {
                    r.get::<_, i64>(0)
                })?,
                db.query_row("SELECT COUNT(*) FROM history WHERE excluded=1", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            ))
        })
        .unwrap();
    assert_eq!(counts, (1, 1));
}

#[tokio::test]
async fn streaming_ingress_deduplicates_chunks_and_rejects_obsolete_or_foreign_scope() {
    let (store, mut router) = fixture();
    send(&mut router, "@alice stream input");
    let owner = ExecutorKey::Member(nick("alice"));
    let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
    let work = reader.unfinished().await.unwrap().remove(0);
    let out = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: work.work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap();
    let RequestResult::Internal(InternalResult::Claimed(grant)) = out.result else {
        panic!("claim")
    };
    let bus = squad_core::bus::InProcessBus::start(router);
    let submit = bus.executor_submit(owner);
    let mut feed = bus.subscribe();
    let delta = Delta {
        generation: GenerationIdentity {
            message_id: grant.work.message_id,
            work_id: work.work_id,
            trigger_event_id: grant.work.trigger_event_id,
            attempt: 1,
        },
        chunk_index: 1,
        text: "visible text".into(),
        debate_id: None,
        round: None,
        case_id: None,
        contest_id: None,
        step_id: None,
    };
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Delta {
                fence: grant.fence,
                epoch: 1,
                delta: delta.clone()
            })
            .await,
        RequestResult::Accepted { .. }
    ));
    assert!(matches!(feed.recv().await.unwrap(), FeedItem::Delta(_)));
    submit
        .submit(ExecutorRequest::Delta {
            fence: grant.fence,
            epoch: 1,
            delta: delta.clone(),
        })
        .await;
    assert!(matches!(
        feed.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    let mut forged = delta.clone();
    forged.case_id = Some(CaseId::new());
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Delta {
                fence: grant.fence,
                epoch: 1,
                delta: forged
            })
            .await,
        RequestResult::Rejected { .. }
    ));
    assert!(matches!(
        submit
            .submit(ExecutorRequest::Delta {
                fence: grant.fence + 1,
                epoch: 1,
                delta
            })
            .await,
        RequestResult::Rejected {
            error: ErrorPayload {
                code: ErrorCode::STALE_WORK_ATTEMPT,
                ..
            },
            ..
        }
    ));
    let presence = Presence {
        nick: nick("alice"),
        status: PresenceStatus::Streaming,
        work_id: Some(work.work_id),
        queue_depth: 0,
        error_code: None,
        observed_at: chrono::Utc::now(),
    };
    assert!(matches!(
        submit.submit(ExecutorRequest::Presence(presence)).await,
        RequestResult::Accepted { .. }
    ));
    assert!(matches!(feed.recv().await.unwrap(), FeedItem::Presence(_)));
}

#[tokio::test]
async fn assigned_artifacts_and_skipped_outcomes_commit_with_their_barriers() {
    let (store, mut router) = fixture();
    let source = send(&mut router, "@bob input for assigned work").events[0].clone();
    let state = prepared(&mut router);
    router
        .produce(envelope(
            EventId::new(),
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan(&state, 1.0)),
        ))
        .unwrap();
    let state = router.debate_state(state.debate_id).unwrap();
    for (ordinal, name, skip) in [(1, "alice", false), (2, "bob", true)] {
        let step_id = StepId::new();
        let work_id = WorkId::new();
        let context = ContextId(format!("contest:{}:{step_id}:{name}", state.debate_id));
        let step = ContestStep {
            contest_id: state.debate_id,
            step_id,
            ordinal,
            script_version: Text::new("fixture.v1").unwrap(),
            move_name: ContestMove::Fanout,
            inputs: vec![source.base.id],
            assignments: vec![ContestAssignment {
                target: Target::Member(nick(name)),
                context_id: context.clone(),
                payload_refs: vec![source.base.id],
                work_id: Some(work_id),
            }],
        };
        let work = Work {
            work_id,
            message_id: MessageId::new(),
            in_reply_to_message_id: Some(MessageId::from_event(source.base.id)),
            kind: WorkKind::Reply,
            target: Target::Member(nick(name)),
            context_id: context.clone(),
            trigger_event_id: source.base.id,
            history_through: HistoryPosition::new(1).unwrap(),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: None,
            prompt_ref: None,
            max_output_tokens: None,
            contest_id: Some(state.debate_id),
            step_id: Some(step_id),
        };
        let projection = HistoryProjection {
            context_id: context,
            origin_event_id: source.base.id,
            projection_key: Text::new("blind.assignment").unwrap(),
            role: HistoryRole::User,
            name: None,
            content: "Assigned input".into(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        let maps = [(
            format!("step:{step_id}"),
            serde_json::json!({"privatePermutation":["bob","alice"]}),
        )]
        .into_iter()
        .collect();
        router
            .allocate_step(EventId::new(), step, vec![projection], vec![work], maps)
            .unwrap();
        let mut artifacts = vec![];
        if skip {
            store
                .call(move |db| {
                    let failure = squad_core::router::error(
                        ErrorCode::PROVIDER_TIMEOUT,
                        "Provider timed out",
                    );
                    db.execute(
                        "UPDATE work SET status='failed',attempt=2,error_json=? WHERE work_id=?",
                        rusqlite::params![serde_json::to_string(&failure)?, work_id.to_string()],
                    )?;
                    Ok(())
                })
                .unwrap();
        } else {
            let owner = ExecutorKey::Member(nick(name));
            let out = router
                .process(
                    Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                        work_id,
                    })),
                    Some(owner.clone()),
                )
                .unwrap();
            let RequestResult::Internal(InternalResult::Claimed(grant)) = out.result else {
                panic!("claim")
            };
            router
                .process(
                    Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                        input_tokens: None,
                        work_id,
                        attempt: 1,
                        fence: grant.fence,
                    })),
                    Some(owner.clone()),
                )
                .unwrap();
            let artifact = ContestArtifact {
                contest_id: state.debate_id,
                step_id,
                artifact_kind: Text::new("fixture.answer").unwrap(),
                work_id: Some(work_id),
                source_refs: vec![source.base.id],
                data: serde_json::json!({"answer":"accepted"}),
            };
            let mut forged = artifact.clone();
            forged.step_id = StepId::new();
            assert!(
                router
                    .process(
                        Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                            work_id,
                            attempt: 1,
                            fence: grant.fence,
                            epoch: 1,
                            output: EnvelopePayload::ContestArtifact(forged)
                        })),
                        Some(owner.clone())
                    )
                    .is_err()
            );
            let out = router
                .process(
                    Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                        work_id,
                        attempt: 1,
                        fence: grant.fence,
                        epoch: 1,
                        output: EnvelopePayload::ContestArtifact(artifact),
                    })),
                    Some(owner),
                )
                .unwrap();
            artifacts.push(out.events[0].base.id);
        }
        let report = ContestReport {
            contest_id: state.debate_id,
            step_id,
            artifact_event_ids: artifacts,
            skipped_work_ids: if skip { vec![work_id] } else { vec![] },
            data: serde_json::json!({}),
        };
        router
            .barrier(
                EventId::new(),
                BarrierCommit {
                    contest_id: state.debate_id,
                    step_id,
                    expected_state_version: state.state_version,
                    barrier_event: envelope(
                        EventId::new(),
                        Actor::System,
                        vec![Recipient::Me],
                        EnvelopePayload::ContestReport(report),
                    ),
                    projections: vec![],
                    next_work: vec![],
                },
            )
            .unwrap();
        let outcome = store
            .call(move |db| {
                Ok(db.query_row(
                    "SELECT outcome FROM contest_assignments WHERE step_id=?",
                    [step_id.to_string()],
                    |r| r.get::<_, String>(0),
                )?)
            })
            .unwrap();
        assert_eq!(outcome, if skip { "skipped" } else { "completed" });
    }
    assert_eq!(
        store
            .call(|db| Ok(db.query_row(
                "SELECT COUNT(*) FROM contest_private_maps WHERE scope_key LIKE 'step:%'",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
        2
    );
}
