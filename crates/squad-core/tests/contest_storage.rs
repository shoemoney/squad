//! M1.P4.L4.T1 (+ M1.P3.L4.T11): competing claims, persistent pending picks,
//! rejected picks keeping state, ordinary routing while pending, and
//! `pick_cancelled` retention, all against the real store and router.
use squad_core::router::{Router, error};
use squad_protocol::*;
use squad_store::Store;

fn nick(s: &str) -> MemberNick {
    MemberNick::new(s).unwrap()
}

fn fixture() -> (Store, Router) {
    fixture_with_members(&["alice", "bob", "carol"])
}

fn fixture_with_members(names: &[&str]) -> (Store, Router) {
    let store = Store::open(":memory:").unwrap();
    store.call(|db| {
        db.execute("INSERT INTO catalog VALUES ('test/model',131072,65536,'{\"prompt\":\"0\",\"completion\":\"0\"}','[]','2026-11-25T00:00:00Z')", [])?;
        Ok(())
    }).unwrap();
    let roster = names.iter().map(|name| nick(name)).collect();
    let bindings = names
        .iter()
        .copied()
        .chain(std::iter::once("judge"))
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

fn send(router: &mut Router, text: &str) -> RequestResult {
    let meta = parser::SubmissionMeta::default();
    let parsed = parse(text, &meta, &router.parse_context().unwrap()).unwrap();
    router
        .process(
            Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap(),
            None,
        )
        .unwrap()
        .result
}

async fn grant_for(store: &Store, router: &mut Router, name: &str) -> ExecutionGrant {
    let owner = ExecutorKey::Member(nick(name));
    let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
    let record = reader.unfinished().await.unwrap().remove(0);
    let RequestResult::Internal(InternalResult::Claimed(grant)) = router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
                work_id: record.work_id,
            })),
            Some(owner.clone()),
        )
        .unwrap()
        .result
    else {
        panic!("claim")
    };
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::StartAttempt {
                input_tokens: None,
                work_id: record.work_id,
                attempt: grant.work.attempt,
                fence: grant.fence,
            })),
            Some(owner),
        )
        .unwrap();
    grant
}

async fn answer(store: &Store, router: &mut Router, name: &str) {
    let grant = grant_for(store, router, name).await;
    let reply = Completion {
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
        content: format!("{name} proposal"),
    };
    router
        .process(
            Request::Internal(InternalRequest::Executor(ExecutorRequest::Result {
                work_id: grant.work.work_id,
                attempt: grant.work.attempt,
                fence: grant.fence,
                epoch: grant.epoch,
                output: EnvelopePayload::AgentReply(reply),
            })),
            Some(ExecutorKey::Member(nick(name))),
        )
        .unwrap();
}

async fn fail(store: &Store, router: &mut Router, name: &str) {
    // Transport failures are retried once before they are terminal.
    for _ in 0..3 {
        let owner = ExecutorKey::Member(nick(name));
        let reader = squad_store::readers::ScopedReaders::new(store.clone(), owner.clone(), 1);
        if reader.unfinished().await.unwrap().is_empty() {
            return;
        }
        let grant = grant_for(store, router, name).await;
        let mut failure = error(ErrorCode::MODEL_UNAVAILABLE, "model went away");
        failure.work_id = Some(grant.work.work_id);
        failure.contest_id = grant.work.contest_id;
        failure.debate_id = grant.work.debate_id;
        failure.case_id = grant.work.case_id;
        router
            .process(
                Request::Internal(InternalRequest::Executor(ExecutorRequest::Failed {
                    work_id: grant.work.work_id,
                    attempt: grant.work.attempt,
                    fence: grant.fence,
                    epoch: grant.epoch,
                    error: failure,
                    repair: None,
                })),
                Some(owner),
            )
            .unwrap();
    }
}

fn scalar(store: &Store, sql: &'static str) -> i64 {
    store
        .call(move |db| Ok(db.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

async fn pending_debate_pick() -> (Store, Router, ContestId) {
    let (store, mut router) = fixture();
    assert!(matches!(
        send(&mut router, "@all $debate which approach?"),
        RequestResult::Accepted { .. }
    ));
    for name in ["alice", "bob", "carol"] {
        answer(&store, &mut router, name).await;
    }
    let pending = router.parse_context().unwrap().pending_pick.unwrap();
    (store, router, pending.contest_id)
}

#[tokio::test]
async fn one_contest_claim_at_a_time_and_ordinary_routing_continues_while_a_pick_is_pending() {
    let (store, mut router, contest) = pending_debate_pick().await;
    assert_eq!(
        scalar(&store, "SELECT COUNT(*) FROM contests WHERE active=1"),
        1
    );
    // A second contest of any shape cannot claim while the pick is pending.
    let meta = parser::SubmissionMeta::default();
    for text in [
        "@all $debate another question",
        "@all $grill another question",
    ] {
        let parsed = parse(text, &meta, &router.parse_context().unwrap());
        let rejected = match parsed {
            Err(e) => e.code,
            Ok(parsed) => {
                match router.process(
                    Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap(),
                    None,
                ) {
                    Ok(out) => match out.result {
                        RequestResult::Rejected { error, .. } => error.code,
                        other => panic!("second claim admitted: {other:?}"),
                    },
                    Err(squad_store::StoreError::Domain(code, _)) => code,
                    Err(other) => panic!("{other:?}"),
                }
            }
        };
        assert!(
            matches!(rejected, ErrorCode::DEBATE_BUSY | ErrorCode::CONTEST_BUSY),
            "{text}: {rejected:?}"
        );
    }
    assert_eq!(
        scalar(&store, "SELECT COUNT(*) FROM contests WHERE active=1"),
        1
    );
    // The pick is persistent: a fresh Router over the same store still sees it.
    let again = Router::new(
        store.clone(),
        vec![nick("alice"), nick("bob"), nick("carol")],
        ["alice", "bob", "carol", "judge"]
            .into_iter()
            .map(|n| {
                (
                    n.to_owned(),
                    ExecutorBinding {
                        model: Text::new("test/model").unwrap(),
                        effort: ReasoningEffort::Low,
                    },
                )
            })
            .collect(),
        20_000_000,
    )
    .unwrap();
    assert_eq!(
        again
            .parse_context()
            .unwrap()
            .pending_pick
            .unwrap()
            .contest_id,
        contest
    );
    // Ordinary routing is unaffected: a plain message to a member queues work.
    let before = scalar(&store, "SELECT COUNT(*) FROM work");
    assert!(matches!(
        send(&mut router, "@alice any thoughts?"),
        RequestResult::Accepted { .. }
    ));
    assert_eq!(scalar(&store, "SELECT COUNT(*) FROM work"), before + 1);
    assert!(router.parse_context().unwrap().pending_pick.is_some());
}

#[tokio::test]
async fn rejected_picks_leave_the_pending_selection_untouched() {
    let (store, mut router, contest) = pending_debate_pick().await;
    let snapshot = |store: &Store| -> (Option<String>, i64) {
        store
            .call(move |db| {
                Ok((
                    db.query_row(
                        "SELECT picked_json FROM contest_picks WHERE contest_id=?",
                        [contest.to_string()],
                        |r| r.get(0),
                    )?,
                    db.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?,
                ))
            })
            .unwrap()
    };
    let before = snapshot(&store);
    for nicks in [
        vec![nick("alice")],
        vec![nick("alice"), nick("alice")],
        vec![nick("alice"), nick("bob"), nick("carol")],
    ] {
        let RequestResult::Rejected { error, .. } = router
            .process(
                Request::ContestPick {
                    request_id: EventId::new(),
                    contest_id: contest,
                    nicks,
                },
                None,
            )
            .unwrap()
            .result
        else {
            panic!("invalid pick was accepted")
        };
        assert_eq!(error.code, ErrorCode::CONTEST_PICK_INVALID);
    }
    // Rejections leave no durable trace beyond receipts: same pick row, same events.
    assert_eq!(snapshot(&store), before);
    assert_eq!(before.0, None);
    assert!(router.parse_context().unwrap().pending_pick.is_some());
    // A pick for the wrong contest id is not found, and the real one still works.
    assert!(matches!(
        router.process(
            Request::ContestPick {
                request_id: EventId::new(),
                contest_id: ContestId::new(),
                nicks: vec![nick("alice"), nick("bob")],
            },
            None
        ),
        Err(squad_store::StoreError::Domain(
            ErrorCode::CONTEST_NOT_FOUND,
            _
        ))
    ));
    assert!(matches!(
        router
            .process(
                Request::ContestPick {
                    request_id: EventId::new(),
                    contest_id: contest,
                    nicks: vec![nick("alice"), nick("bob")],
                },
                None
            )
            .unwrap()
            .result,
        RequestResult::Accepted { .. }
    ));
    assert!(router.parse_context().unwrap().pending_pick.is_none());
}

#[tokio::test]
async fn stop_while_pick_is_pending_retains_answers_as_pick_cancelled() {
    let (store, mut router, contest) = pending_debate_pick().await;
    let stopped = router
        .process(
            Request::Stop {
                request_id: EventId::new(),
                target: None,
            },
            None,
        )
        .unwrap();
    assert!(matches!(stopped.result, RequestResult::Accepted { .. }));
    assert!(router.parse_context().unwrap().pending_pick.is_none());
    assert_eq!(
        scalar(&store, "SELECT COUNT(*) FROM contests WHERE active=1"),
        0
    );
    let reason: String = store
        .call(move |db| {
            Ok(db.query_row(
                "SELECT json_extract(pending_request_json,'$.terminalReason') FROM contests WHERE contest_id=?",
                [contest.to_string()],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(reason, "pick_cancelled");
    // The three answers stay in the retained record and in the members' own histories.
    assert_eq!(
        scalar(
            &store,
            "SELECT COUNT(*) FROM contest_answers WHERE outcome='answered' AND answer_event_seq IS NOT NULL"
        ),
        3
    );
    let history = store
        .call(|db| {
            squad_store::history::read(
                db,
                &ContextId("alice".into()),
                HistoryPosition::new(1_000).unwrap(),
            )
        })
        .unwrap();
    assert!(history.iter().any(|e| e.content == "alice proposal"));
    // A late pick cannot revive the cancelled contest, and a new contest may start.
    assert!(matches!(
        router.process(
            Request::ContestPick {
                request_id: EventId::new(),
                contest_id: contest,
                nicks: vec![nick("alice"), nick("bob")],
            },
            None
        ),
        Err(squad_store::StoreError::Domain(
            ErrorCode::CONTEST_NOT_FOUND,
            _
        ))
    ));
    assert!(matches!(
        send(&mut router, "@all $debate a fresh question"),
        RequestResult::Accepted { .. }
    ));
}

#[tokio::test]
async fn too_few_answers_end_the_collection_with_the_canonical_error() {
    let (store, mut router) = fixture();
    assert!(matches!(
        send(&mut router, "@all $debate which approach?"),
        RequestResult::Accepted { .. }
    ));
    answer(&store, &mut router, "alice").await;
    fail(&store, &mut router, "bob").await;
    fail(&store, &mut router, "carol").await;
    assert!(router.parse_context().unwrap().pending_pick.is_none());
    assert_eq!(
        scalar(&store, "SELECT COUNT(*) FROM contests WHERE active=1"),
        0
    );
    let events = store
        .call(|db| squad_store::events::page(db, None, usize::MAX))
        .unwrap();
    let error = events
        .iter()
        .find_map(|e| match &e.content {
            EnvelopePayload::Error(p) if p.code == ErrorCode::CONTEST_NOT_ENOUGH_ANSWERS => Some(p),
            _ => None,
        })
        .expect("not-enough-answers error is committed");
    assert_eq!(
        error.text.as_str(),
        "Not enough answers to start. 1 of 3 members answered."
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e.content, EnvelopePayload::ContestPickRequested(_)))
    );
}

#[test]
fn eighteen_kinds_share_exactly_one_active_claim() {
    let (store, _) = fixture();
    store.call(|db|{
  let kinds=["debate","tournament","grill","census","crosscheck","survey","probe","forge","forecast","decide","negotiate","blow_it_up","readback","debug","trace","rank","tune","bass_ackwards_prd"];
  for first in kinds {
   db.execute("DELETE FROM contests",[])?;
   db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('first',?,1,1,'{}',1)",[first])?;
   for second in kinds {assert!(db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES('second',?,1,1,'{}',1)",[second]).is_err(),"{first} and {second}");}
  }Ok(())
 }).unwrap();
}

#[test]
fn new_mode_admission_names_each_active_kind_without_starting_work() {
    let (store, router) = fixture_with_members(&["alice", "bob", "carol", "dave"]);
    let starts = [
        ("forge", "@all $forge Write a short launch post"),
        (
            "forecast",
            "@all $forecast Will the project ship this year?",
        ),
        (
            "decide",
            "@all $decide Choose storage\n--\nSQLite\nPostgreSQL",
        ),
        ("negotiate", "@all $negotiate Agree a support contract"),
        (
            "blow_it_up",
            "@all $blow_it_up Deploy without a rollback plan",
        ),
        ("readback", "@all $readback Keep records for thirty days"),
        ("debug", "@all $debug Requests stall\n--\nInspect logs only"),
        (
            "trace",
            "@all $trace When does it run?\n--\nIt runs on Mondays.",
        ),
        (
            "rank",
            "@all $rank Prioritize features\n--\nExport\nRecovery\nSearch",
        ),
        (
            "tune",
            "@all $tune Be concise\n--\nReturn one correct label",
        ),
        (
            "bass_ackwards_prd",
            "@all $bass_ackwards_prd A reliable status page",
        ),
    ];
    for active in [
        "debate",
        "tournament",
        "grill",
        "census",
        "crosscheck",
        "survey",
        "probe",
        "forge",
        "forecast",
        "decide",
        "negotiate",
        "blow_it_up",
        "readback",
        "debug",
        "trace",
        "rank",
        "tune",
        "bass_ackwards_prd",
    ] {
        let contest = ContestId::new();
        let state = if starts.iter().any(|(kind, _)| *kind == active) {
            serde_json::json!({"contestId":contest,"stateVersion":1,"state":"PREFLIGHT",
                "stepId":null,"round":0,"artifactEventIds":[],"terminalReason":null})
            .to_string()
        } else {
            serde_json::to_string(&ContestSelectionState {
                contest_id: contest,
                kind: serde_json::from_value(serde_json::json!(active)).unwrap(),
                state: ContestSelectionStatus::CollectingAnswers,
                state_version: 1,
                question_message_id: MessageId::new(),
                addressed: vec![nick("alice"), nick("bob"), nick("carol")],
                answers: vec![],
                required_picks: 1,
            })
            .unwrap()
        };
        store.call(move |db| {
            db.execute("DELETE FROM contests", [])?;
            db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES(?,?,1,1,?,1)",rusqlite::params![contest.to_string(),active,state])?;
            Ok(())
        }).unwrap();
        for (kind, command) in starts {
            let mode: ContestKind = serde_json::from_value(serde_json::json!(kind)).unwrap();
            if !mode.is_built() {
                continue;
            }
            let context = router.parse_context().unwrap();
            let error = parse(command, &parser::SubmissionMeta::default(), &context)
                .expect_err("an active contest must refuse a new mode");
            assert_eq!(error.code, ErrorCode::CONTEST_BUSY, "{kind} vs {active}");
            assert_eq!(
                error.message,
                format!("A {active} is already running. Stop it before starting another.")
            );
        }
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM contests"), 1);
        assert_eq!(scalar(&store, "SELECT COUNT(*) FROM work"), 0);
    }
}
