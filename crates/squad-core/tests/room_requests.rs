#[path = "support/m6_room.rs"]
mod live;
#[path = "room_support.rs"]
mod support;
use squad_protocol::*;
use support::*;
#[tokio::test]
async fn receipt_identity_and_conflicts_pass_through_shared_router() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    let id = EventId::new();
    let request = message("@all -b $ROUNDS=4 hello", id);
    let result = room.submit(request.clone()).await;
    let RequestResult::Accepted {
        request_id,
        event_ids,
        cursor,
    } = &result
    else {
        panic!("{result:?}")
    };
    assert_eq!(*request_id, id);
    assert_eq!(event_ids.len(), 2);
    assert!(cursor.is_some());
    let wakes = launcher.wakes.lock().unwrap().clone();
    assert_eq!(wakes.len(), 1);
    assert_eq!(room.submit(request).await, result);
    assert_eq!(*launcher.wakes.lock().unwrap(), wakes);
    assert!(
        matches!(room.submit(message("@a changed",id)).await,RequestResult::Rejected {error,..} if error.code==ErrorCode::IDEMPOTENCY_CONFLICT)
    );
    assert_eq!(room.state().await.vars.rounds, 4);
    let json = serde_json::to_value(result).unwrap().to_string();
    assert!(json.contains("requestId"));
    assert!(!json.contains("commandId"));
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn restarted_one_shot_receipt_replay_does_not_wake_prior_work() {
    let root = tempfile::tempdir().unwrap();
    let first = Launcher::default();
    let (room, _) = open(root.path(), &first, OpenMode::OneShot).await;
    let request = message("@a hello", EventId::new());
    let result = room.submit_commit_only(request.clone()).await;
    accepted(result.clone());
    room.shutdown().await.unwrap();
    let next = Launcher::default();
    let (room, _) = open(root.path(), &next, OpenMode::OneShot).await;
    assert_eq!(room.submit(request).await, result);
    assert!(next.wakes.lock().unwrap().is_empty());
    room.shutdown().await.unwrap();
}
#[tokio::test]
async fn one_shot_retry_wakes_new_work_with_the_original_trigger() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a retry me", EventId::new()))
            .await,
    );
    let original = store(&launcher)
        .call(|db| {
            let id: String = db.query_row("SELECT work_id FROM work", [], |r| r.get(0))?;
            db.execute("UPDATE work SET status='failed'", [])?;
            Ok(id.parse::<WorkId>().unwrap())
        })
        .unwrap();
    accepted(
        room.submit(Request::Retry {
            request_id: EventId::new(),
            work_id: original,
        })
        .await,
    );
    let wakes = launcher.wakes.lock().unwrap().clone();
    assert_eq!(wakes.len(), 1);
    assert_ne!(wakes[0], original);
    room.shutdown().await.unwrap();
}

// ---- M3.P1.L2.T2 / T8 / T10 / T11 ----

fn code(result: &RequestResult) -> ErrorCode {
    match result {
        RequestResult::Rejected { error, .. } => error.code,
        other => panic!("expected a rejection, got {other:?}"),
    }
}

fn record_of(event: &CommittedEnvelope) -> parser::RecordSummary {
    let (message_id, content) = match &event.content {
        EnvelopePayload::MessageFinal(m) => {
            (Some(m.completed.generation.message_id), m.content.clone())
        }
        EnvelopePayload::UserMessage(m) => (Some(m.input.message_id), m.body.clone()),
        _ => (None, String::new()),
    };
    parser::RecordSummary {
        event_id: event.base.id,
        message_id,
        seq: event.commit.seq,
        kind: Text::new(
            serde_json::to_value(&event.content).unwrap()["kind"]
                .as_str()
                .unwrap(),
        )
        .unwrap(),
        author: event.base.sender.clone(),
        recipients: event.base.recipients.clone(),
        content,
        in_reply_to_message_id: match &event.content {
            EnvelopePayload::MessageFinal(m) => Some(m.in_reply_to_message_id),
            _ => None,
        },
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    }
}

fn with_records(
    roster: Vec<MemberNick>,
    records: Vec<parser::RecordSummary>,
    text: &str,
    quote: Vec<EventId>,
    reply_to: Vec<EventId>,
) -> Result<Request, parser::ParseError> {
    let context = parser::ParseContext {
        roster,
        records,
        ..Default::default()
    };
    let meta = parser::SubmissionMeta { quote, reply_to };
    let parsed = parse(text, &meta, &context)?;
    Ok(Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap())
}

#[tokio::test]
async fn quotes_exclusions_and_sticky_variables_commit_through_the_shared_router() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message("@a original thought", EventId::new()))
            .await,
    );
    let first = room.page_events(None, 10).await.unwrap().remove(0);
    let roster = vec![nick("a"), nick("b")];
    // A quote resolves into the committed message and is shared content, not a pointer.
    let quoted = with_records(
        roster.clone(),
        vec![record_of(&first)],
        "@b $ROUNDS=4 what about it?",
        vec![first.base.id],
        vec![],
    )
    .unwrap();
    let receipt = room.submit_commit_only(quoted.clone()).await;
    accepted(receipt.clone());
    let events = room.page_events(None, 50).await.unwrap();
    let EnvelopePayload::UserMessage(committed) = &events
        .iter()
        .rev()
        .find(|e| matches!(e.content, EnvelopePayload::UserMessage(_)))
        .unwrap()
        .content
    else {
        unreachable!()
    };
    assert_eq!(committed.resolved_quotes.len(), 1);
    assert_eq!(committed.resolved_quotes[0].event_id, first.base.id);
    assert_eq!(committed.resolved_quotes[0].text, "original thought");
    assert_eq!(
        room.state().await.vars.rounds,
        4,
        "variables stick with the message"
    );
    assert_eq!(
        room.submit_commit_only(quoted).await,
        receipt,
        "receipt identity is stable"
    );
    // A quote the store has never seen is an invalid reference, even if the parser context offered it.
    let mut ghost = record_of(&first);
    ghost.event_id = EventId::new();
    let rejected = room
        .submit_commit_only(
            with_records(
                roster.clone(),
                vec![ghost.clone()],
                "@b hi",
                vec![ghost.event_id],
                vec![],
            )
            .unwrap(),
        )
        .await;
    assert_eq!(code(&rejected), ErrorCode::INVALID_REFERENCE);
    // Operational events (here a variable change) are not shareable content.
    let var_event = events
        .iter()
        .find(|e| matches!(e.content, EnvelopePayload::VarChange(_)))
        .expect("var.change committed");
    let rejected = room
        .submit_commit_only(
            with_records(
                roster.clone(),
                vec![record_of(var_event)],
                "@b hi",
                vec![var_event.base.id],
                vec![],
            )
            .unwrap(),
        )
        .await;
    assert_eq!(code(&rejected), ErrorCode::INVALID_REFERENCE);
    // Exclusions narrow `@all`; excluding everyone is a parse error that never reaches the room.
    let before = room.state().await;
    accepted(
        room.submit_commit_only(message("@all -b everyone but b", EventId::new()))
            .await,
    );
    let after = room.state().await;
    assert_eq!(
        after.participants[0].queue_depth,
        before.participants[0].queue_depth + 1
    );
    assert_eq!(
        after.participants[1].queue_depth,
        before.participants[1].queue_depth
    );
    let context = parser::ParseContext {
        roster,
        ..Default::default()
    };
    assert!(
        parse(
            "@all -a -b nobody",
            &parser::SubmissionMeta::default(),
            &context
        )
        .is_err()
    );
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn solution_selections_freeze_the_chosen_replies_and_reject_misuse() {
    let mut f = live::Fixture::new().await;
    f.config.budget.debate_confirm_usd = 0.;
    f.fake
        .completions(vec![
            live::fake_openrouter::streaming("solution zero", "stop"),
            live::fake_openrouter::streaming("solution one", "stop"),
        ])
        .await;
    let room = f.open(OpenMode::Interactive).await;
    room.submit(f.request("@member0 @member1 propose")).await;
    f.finals(2).await;
    let events = room.page_events(None, 100).await.unwrap();
    let finals: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
        .collect();
    let roster: Vec<MemberNick> = f.config.agents.iter().map(|a| a.nick.clone()).collect();
    let records: Vec<_> = finals.iter().map(|e| record_of(e)).collect();
    let selected: Vec<EventId> = finals.iter().map(|e| e.base.id).collect();
    // Selection only makes sense for a plain `$debate`, and at most two sources.
    assert!(
        with_records(
            roster.clone(),
            records.clone(),
            "@member0 @member1 hi",
            vec![],
            selected.clone()
        )
        .is_err()
    );
    let result = room
        .submit(
            with_records(
                roster.clone(),
                records.clone(),
                "@member0 @member1 $debate $ROUNDS=1 Compare the two",
                vec![],
                selected.clone(),
            )
            .unwrap(),
        )
        .await;
    let RequestResult::PendingConfirmation { preflight, .. } = &result else {
        panic!("{result:?}")
    };
    assert!(matches!(preflight, PendingPreflight::Debate(_)));
    let frozen: serde_json::Value = serde_json::from_str(
        &f.db()
            .query_row(
                "SELECT map_json FROM contest_private_maps WHERE scope_key='admission'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
    )
    .unwrap();
    let ids: Vec<String> = frozen["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        ids,
        selected.iter().map(|id| id.to_string()).collect::<Vec<_>>()
    );
    // Nothing runs before confirmation: only the two ordinary replies hit the provider.
    assert_eq!(f.fake.captured_bodies().await.len(), 2);
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn pick_routing_validates_the_pending_contest_and_preserves_request_identity() {
    let f = live::Fixture::new().await;
    let room = f.open(OpenMode::OneShot).await;
    room.submit_commit_only(f.request("@all $debate $ROUNDS=2 Compare designs?"))
        .await;
    let Some(ContestState::Selection(pick)) = room.state().await.active_contest else {
        panic!("a question-first debate starts by collecting answers")
    };
    assert_eq!(pick.state, ContestSelectionStatus::CollectingAnswers);
    // Picks for unknown contests are CONTEST_NOT_FOUND and mutate nothing.
    let unknown = room
        .submit_commit_only(Request::ContestPick {
            request_id: EventId::new(),
            contest_id: ContestId::new(),
            nicks: vec![nick("member0"), nick("member1")],
        })
        .await;
    assert_eq!(code(&unknown), ErrorCode::CONTEST_NOT_FOUND);
    // Even for the right contest, a pick needs the answers to be collected first.
    let early = room
        .submit_commit_only(Request::ContestPick {
            request_id: EventId::new(),
            contest_id: pick.contest_id,
            nicks: vec![nick("member0"), nick("member1")],
        })
        .await;
    assert!(matches!(early, RequestResult::Rejected { .. }), "{early:?}");
    // Nicks outside the roster are refused with the parser's wording.
    let stranger = room
        .submit_commit_only(Request::ContestPick {
            request_id: EventId::new(),
            contest_id: pick.contest_id,
            nicks: vec![nick("ghost"), nick("member0")],
        })
        .await;
    assert!(
        matches!(stranger, RequestResult::Rejected { .. }),
        "{stranger:?}"
    );
    assert!(room.state().await.active_contest.is_some());
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn kind_specific_confirmation_rejects_the_wrong_request_family_and_stale_plans() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
    let result = room
        .submit(message4(
            "@m0 @m1 @m2 $tournament Design a bridge",
            EventId::new(),
        ))
        .await;
    let RequestResult::PendingConfirmation { preflight, .. } = &result else {
        panic!("{result:?}")
    };
    let PendingPreflight::Contest(plan) = preflight else {
        panic!("{preflight:?}")
    };
    // A stale plan id never confirms or cancels the live preflight.
    for stale in [
        Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: PlanId::new(),
        },
        Request::ContestCancel {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: PlanId::new(),
        },
    ] {
        assert!(matches!(
            room.submit_commit_only(stale).await,
            RequestResult::Rejected { .. }
        ));
    }
    assert_eq!(room.state().await.pending_preflights.len(), 1);
    // Cancelling with the right identity removes the preflight and wakes nothing.
    accepted(
        room.submit(Request::ContestCancel {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        })
        .await,
    );
    assert!(room.state().await.pending_preflights.is_empty());
    assert!(launcher.wakes.lock().unwrap().is_empty());
    // The cancelled plan can no longer be confirmed.
    assert!(matches!(
        room.submit_commit_only(Request::ContestConfirm {
            request_id: EventId::new(),
            contest_id: plan.contest_id,
            plan_id: plan.plan_id,
        })
        .await,
        RequestResult::Rejected { .. }
    ));
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn new_mode_requests_validate_and_keep_receipts_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
    // Modes needing several participants fail in the shared parser, before any state changes.
    let context = parser::ParseContext {
        roster: (0..4).map(|i| nick(&format!("m{i}"))).collect(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    for single in [
        "@m0 $census plan",
        "@m0 $survey q",
        "@m0 $probe brief",
        "@m0 $crosscheck a\n--\nb",
    ] {
        // Census always uses the whole room; the other modes honour the addressed members.
        let Ok(parsed) = parse(single, &meta, &context) else {
            // Probe and crosscheck need several named members, which one mention cannot supply.
            assert!(
                single.contains("$probe") || single.contains("$crosscheck"),
                "{single}"
            );
            continue;
        };
        let request = Request::from_parsed(EventId::new(), parsed, &meta, false).unwrap();
        let result = room.submit(request).await;
        let RequestResult::PendingConfirmation {
            preflight: PendingPreflight::Contest(plan),
            ..
        } = &result
        else {
            panic!("{single}: {result:?}")
        };
        assert!(plan.participants.contains(&nick("m0")), "{single}");
        if plan.kind == ContestKind::Census {
            assert_eq!(
                plan.participants.len(),
                4,
                "census always uses the whole room"
            );
        }
        accepted(
            room.submit(Request::Stop {
                request_id: EventId::new(),
                target: None,
            })
            .await,
        );
    }
    assert!(room.state().await.pending_preflights.is_empty());
    let id = EventId::new();
    let result = room
        .submit(message4("@all $survey Which checks help?", id))
        .await;
    assert!(matches!(result, RequestResult::PendingConfirmation { .. }));
    assert_eq!(
        room.submit(message4("@all $survey Which checks help?", id))
            .await,
        result
    );
    assert_eq!(
        code(
            &room
                .submit(message4("@all $probe different command", id))
                .await
        ),
        ErrorCode::IDEMPOTENCY_CONFLICT
    );
    // Only one contest may be active; the second mode request is refused with CONTEST_BUSY.
    assert_eq!(
        code(
            &room
                .submit(message4("@all $census Build a service.", EventId::new()))
                .await
        ),
        ErrorCode::CONTEST_BUSY
    );
    // Continuing a census or accepting crosscheck risk for a contest that does not exist is refused.
    let missing = ContestId::new();
    for request in [
        Request::CensusContinue {
            request_id: EventId::new(),
            contest_id: missing,
            expected_state_version: 1,
            plan_event_id: None,
            replacement_text: None,
        },
        Request::CrosscheckAccept {
            request_id: EventId::new(),
            contest_id: missing,
            expected_state_version: 1,
            plan_event_id: EventId::new(),
            grid_event_id: EventId::new(),
            cell_ids: vec![],
            reason: Text::new("accepted risk").unwrap(),
        },
    ] {
        assert!(matches!(
            room.submit_commit_only(request).await,
            RequestResult::Rejected { .. }
        ));
    }
    room.shutdown().await.unwrap();
}

#[tokio::test]
async fn mode_submissions_never_expose_quoted_plans_or_member_context_automatically() {
    let root = tempfile::tempdir().unwrap();
    let launcher = Launcher::default();
    let (room, _) = open4(root.path(), &launcher, OpenMode::OneShot).await;
    accepted(
        room.submit_commit_only(message4(
            "@m0 PRIVATE_M0_CANARY only for m0",
            EventId::new(),
        ))
        .await,
    );
    let result = room
        .submit(message4(
            "@all $crosscheck PLAN_CANARY Maintain service.\n--\nKeep service available.",
            EventId::new(),
        ))
        .await;
    assert!(
        matches!(result, RequestResult::PendingConfirmation { .. }),
        "{result:?}"
    );
    // The public preflight carries counts and participants, never plan text or member history.
    let json = serde_json::to_string(&room.state().await.pending_preflights).unwrap();
    assert!(!json.contains("PLAN_CANARY"));
    assert!(!json.contains("PRIVATE_M0_CANARY"));
    // Admitted-but-unconfirmed work does not exist, so no context can have been delivered.
    let delivered: i64 = store(&launcher)
        .call(|db| {
            Ok(db.query_row(
                "SELECT COUNT(*) FROM work WHERE kind LIKE 'crosscheck%'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(delivered, 0);
    // m1 can never see what was said privately to m0: its ordinary history is empty.
    let leaked: i64 = store(&launcher)
        .call(|db| {
            Ok(db.query_row(
                "SELECT COUNT(*) FROM history WHERE context_id<>'m0' AND instr(content,'PRIVATE_M0_CANARY')>0",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(leaked, 0);
    room.shutdown().await.unwrap();
}
