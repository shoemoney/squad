//! M1.P3.L1.T9 / M1.P3.L7.T10: the router re-parses every submission against
//! authoritative state, so error codes and strings come from the store's view
//! and a forged AST, staged variable or source reference can never be admitted.
use squad_core::router::Router;
use squad_protocol::*;
use squad_store::{Store, StoreError};
use std::collections::BTreeMap;

fn nick(name: &str) -> MemberNick {
    name.parse().unwrap()
}

fn router_with(count: usize, daily_max: i64) -> (Store, Router) {
    let store = Store::open(":memory:").unwrap();
    store.call(|db| {
        db.execute("INSERT INTO catalog VALUES ('test/model',131072,65536,'{\"prompt\":\"0\",\"completion\":\"0\"}','[]','2026-11-25T00:00:00Z')", [])?;
        Ok(())
    }).unwrap();
    let roster: Vec<_> = (0..count).map(|i| nick(&format!("member{i}"))).collect();
    let mut bindings: BTreeMap<String, ExecutorBinding> = roster
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
    bindings.insert(
        "judge".into(),
        ExecutorBinding {
            model: Text::new("test/model").unwrap(),
            effort: ReasoningEffort::Low,
        },
    );
    (
        store.clone(),
        Router::new(store, roster, bindings, daily_max).unwrap(),
    )
}

fn context(count: usize) -> parser::ParseContext {
    parser::ParseContext {
        roster: (0..count).map(|i| nick(&format!("member{i}"))).collect(),
        ..Default::default()
    }
}

/// A user message whose AST is valid for `parsed_text` but whose raw text is
/// `text`: exactly what a client that skips or forges the parser would send.
fn forged(text: &str, parsed_text: &str, count: usize, meta: &parser::SubmissionMeta) -> Request {
    let mut parsed = parse(parsed_text, &Default::default(), &context(count)).unwrap();
    if !meta.quote.is_empty() {
        // Attach the quote to the AST directly: the client-side context that
        // would have validated it is exactly what a forger skips.
        let mut value = serde_json::to_value(&parsed).unwrap();
        value["quote"] = serde_json::to_value(&meta.quote).unwrap();
        parsed = serde_json::from_value(value).unwrap();
    }
    let id = EventId::new();
    Request::UserMessage {
        input: UserMessageInput {
            message_id: MessageId::from_event(id),
            text: Text::new(text).unwrap(),
            quote: (!meta.quote.is_empty()).then(|| meta.quote.clone()),
            reply_to: None,
        },
        preconfirmed: false,
        parsed,
    }
}

fn rejection(router: &mut Router, request: Request) -> ErrorPayload {
    match router.process(request, None).unwrap().result {
        RequestResult::Rejected { error, .. } => error,
        other => panic!("expected a rejection, got {other:?}"),
    }
}

fn row_count(store: &Store, sql: &'static str) -> i64 {
    store
        .call(move |db| Ok(db.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn contest_shape_errors_map_to_their_canonical_codes_and_strings() {
    let cases: [(&str, ErrorCode, &str); 6] = [
        (
            "@member0 @member1 $tournament Design a useful CLI",
            ErrorCode::TOURNAMENT_ENTRANTS_REQUIRED,
            "$tournament needs at least three entrants.",
        ),
        (
            "@member0 @member0 @member1 $tournament Design a useful CLI",
            ErrorCode::TOURNAMENT_ENTRANTS_REQUIRED,
            "$tournament needs at least three entrants.",
        ),
        (
            "@member0 @member1 $census Plan text",
            ErrorCode::CENSUS_PARTICIPANTS_REQUIRED,
            "$census needs at least three participants.",
        ),
        (
            "@member0 @member1 $crosscheck",
            ErrorCode::CROSSCHECK_PARTICIPANTS_REQUIRED,
            "$crosscheck needs at least three participants.",
        ),
        (
            "@member0 @member0 @member1 @member2 $probe Requests stall.",
            ErrorCode::PROBE_PARTICIPANTS_REQUIRED,
            "$probe needs at least four members; an odd selection leaves the last member out.",
        ),
        (
            "@member0 $grill",
            ErrorCode::GRILL_SOURCE_MISSING,
            "@member0 has no plan to grill yet. Ask it something first.",
        ),
    ];
    for (text, code, message) in cases {
        let (store, mut router) = router_with(6, 20_000_000);
        let error = rejection(
            &mut router,
            forged(text, "@member0 hi", 6, &Default::default()),
        );
        assert_eq!(error.code, code, "{text}");
        assert_eq!(error.text.as_str(), message, "{text}");
        assert_eq!(row_count(&store, "SELECT COUNT(*) FROM work"), 0, "{text}");
        assert_eq!(row_count(&store, "SELECT COUNT(*) FROM contests"), 0);
    }
}

#[test]
fn grill_with_no_other_participants_is_refused_on_the_roster_it_actually_has() {
    // A one-member room has nobody who can grill the leader.
    let (store, mut router) = router_with(1, 20_000_000);
    let error = rejection(
        &mut router,
        forged("@member0 $grill", "@member0 hi", 1, &Default::default()),
    );
    assert!(
        matches!(
            error.code,
            ErrorCode::GRILL_PARTICIPANTS_REQUIRED | ErrorCode::GRILL_SOURCE_MISSING
        ),
        "{error:?}"
    );
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM contests"), 0);
}

#[test]
fn unknown_and_unshareable_source_references_are_rejected_without_side_effects() {
    let (store, mut router) = router_with(3, 20_000_000);
    // Nothing exists yet: a quote of an unknown event is an invalid reference.
    let meta = parser::SubmissionMeta {
        quote: vec![EventId::new()],
        reply_to: vec![],
    };
    let result = router.process(
        forged("@member0 see this", "@member0 see this", 3, &meta),
        None,
    );
    let code = match result {
        Ok(outcome) => match outcome.result {
            RequestResult::Rejected { error, .. } => error.code,
            other => panic!("unknown quote was admitted: {other:?}"),
        },
        Err(StoreError::Domain(code, _)) => code,
        Err(other) => panic!("{other:?}"),
    };
    assert_eq!(code, ErrorCode::INVALID_REFERENCE);
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM work"), 0);
    assert_eq!(
        row_count(
            &store,
            "SELECT COUNT(*) FROM events WHERE kind='user.message'"
        ),
        0
    );

    // A real event whose payload is not shareable message content (a variable
    // change) cannot be smuggled in as a generic source reference either.
    router
        .process(
            Request::SetVars {
                request_id: EventId::new(),
                updates: vec![VariableUpdate {
                    name: VariableName::Rounds,
                    value: 4,
                }],
            },
            None,
        )
        .unwrap();
    let var_event: String = store
        .call(|db| {
            Ok(db.query_row(
                "SELECT id FROM events WHERE kind='var.change' LIMIT 1",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let meta = parser::SubmissionMeta {
        quote: vec![var_event.parse().unwrap()],
        reply_to: vec![],
    };
    let result = router.process(
        forged("@member0 see this", "@member0 see this", 3, &meta),
        None,
    );
    let code = match result {
        Ok(outcome) => match outcome.result {
            RequestResult::Rejected { error, .. } => error.code,
            other => panic!("unshareable quote was admitted: {other:?}"),
        },
        Err(StoreError::Domain(code, _)) => code,
        Err(other) => panic!("{other:?}"),
    };
    assert_eq!(code, ErrorCode::INVALID_REFERENCE);
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM work"), 0);
}

#[test]
fn forged_parse_snapshots_are_rejected_on_revalidation() {
    let (store, mut router) = router_with(3, 20_000_000);
    // AST says @member0 but the authoritative text addresses @member1.
    let request = forged("@member1 hello", "@member0 hello", 3, &Default::default());
    match router.process(request, None) {
        Err(StoreError::Domain(ErrorCode::INVALID_COMMAND, text)) => {
            assert_eq!(text, "Parsed input does not match authoritative state");
        }
        other => panic!("forged AST was not rejected: {other:?}"),
    }
    // Same text, forged body.
    let mut request = forged("@member0 hello", "@member0 hello", 3, &Default::default());
    if let Request::UserMessage { parsed, .. } = &mut request {
        let mut value = serde_json::to_value(&*parsed).unwrap();
        value["body"] = "something the user never typed".into();
        *parsed = serde_json::from_value(value).unwrap();
    }
    assert!(matches!(
        router.process(request, None),
        Err(StoreError::Domain(ErrorCode::INVALID_COMMAND, _))
    ));
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM work"), 0);
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM events"), 0);
}

#[test]
fn forged_staged_census_passes_cannot_change_stored_variables() {
    let (store, mut router) = router_with(3, 20_000_000);
    let text = "@member0 count $CENSUS_PASSES=3";
    let mut request = forged(text, text, 3, &Default::default());
    // The AST claims five passes were staged; the text staged three.
    if let Request::UserMessage { parsed, .. } = &mut request {
        let mut value = serde_json::to_value(&*parsed).unwrap();
        assert_eq!(value["variableUpdates"][0]["name"], "CENSUS_PASSES");
        value["variableUpdates"][0]["value"] = 5.into();
        *parsed = serde_json::from_value(value).unwrap();
    }
    assert!(matches!(
        router.process(request, None),
        Err(StoreError::Domain(ErrorCode::INVALID_COMMAND, _))
    ));
    // Defaults are seeded (ROUNDS 10, CENSUS_PASSES 2) and must be untouched.
    assert_eq!(
        row_count(&store, "SELECT value FROM vars WHERE name='CENSUS_PASSES'"),
        2
    );

    // An out-of-range staged value in the raw text is refused with the exact
    // validation string, and nothing else staged in the same message lands.
    let error = rejection(
        &mut router,
        forged(
            "@member0 hi $ROUNDS=4 $CENSUS_PASSES=6",
            "@member0 hi",
            3,
            &Default::default(),
        ),
    );
    assert_eq!(error.code, ErrorCode::INVALID_COMMAND);
    assert_eq!(
        error.text.as_str(),
        "CENSUS_PASSES must be an integer from 1 to 5."
    );
    assert_eq!(
        row_count(&store, "SELECT value FROM vars WHERE name='CENSUS_PASSES'"),
        2
    );
    assert_eq!(
        row_count(&store, "SELECT value FROM vars WHERE name='ROUNDS'"),
        10
    );

    // The honest request still works and stores exactly what was staged.
    let honest = forged(text, text, 3, &Default::default());
    assert!(matches!(
        router.process(honest, None).unwrap().result,
        RequestResult::Accepted { .. }
    ));
    assert_eq!(
        row_count(&store, "SELECT value FROM vars WHERE name='CENSUS_PASSES'"),
        3
    );
}

#[test]
fn only_the_bus_may_submit_internal_requests_and_unknown_contests_are_not_found() {
    let (_store, mut router) = router_with(3, 20_000_000);
    let internal = Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim {
        work_id: WorkId::new(),
    }));
    assert!(router.process(internal, None).is_err());
    // A pick for a contest that is not the pending one is CONTEST_NOT_FOUND.
    let pick = Request::ContestPick {
        request_id: EventId::new(),
        contest_id: ContestId::new(),
        nicks: vec![nick("member0")],
    };
    assert!(matches!(
        router.process(pick, None),
        Err(StoreError::Domain(ErrorCode::CONTEST_NOT_FOUND, _))
    ));
}

#[test]
fn disabled_modes_are_rejected_before_admission_or_spend() {
    let (store, mut router) = router_with(4, 20_000_000);
    for mode in [
        "forge",
        "forecast",
        "negotiate",
        "blow_it_up",
        "readback",
        "debug",
        "trace",
        "rank",
    ] {
        let kind: ContestKind = serde_json::from_value(serde_json::json!(mode)).unwrap();
        if kind.is_built() {
            continue;
        }
        let raw = format!("@all ${mode} brief");
        let ctx = router.parse_context().unwrap();
        let e = parse(&raw, &Default::default(), &ctx).unwrap_err();
        assert_eq!(e.code, ErrorCode::COMMAND_NOT_IMPLEMENTED);
        assert!(e.message.contains("disabled"));
        let request = forged(&raw, "@all hello", 4, &Default::default());
        assert_eq!(
            rejection(&mut router, request).code,
            ErrorCode::COMMAND_NOT_IMPLEMENTED
        );
    }
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM contests"), 0);
    assert_eq!(row_count(&store, "SELECT COUNT(*) FROM reservations"), 0);
}
#[test]
fn invalid_new_mode_member_output_gets_one_repair_and_transport_gets_retry() {
    use squad_core::orchestrator::{
        rounds::failure_policy,
        types::{AttemptDecision, FailureClass},
    };
    let id = WorkId::new();
    assert!(matches!(
        failure_policy(
            id,
            &WorkKind::ForgeDraft,
            1,
            2,
            false,
            FailureClass::InvalidOutput
        ),
        AttemptDecision::Retry {
            next_attempt: 2,
            repair: true,
            ..
        }
    ));
    assert!(matches!(
        failure_policy(
            id,
            &WorkKind::ForgeDraft,
            1,
            2,
            false,
            FailureClass::Transport
        ),
        AttemptDecision::Retry {
            next_attempt: 2,
            repair: false,
            ..
        }
    ));
    assert!(matches!(
        failure_policy(
            id,
            &WorkKind::ForgeDraft,
            2,
            2,
            true,
            FailureClass::InvalidOutput
        ),
        AttemptDecision::Forfeit(_)
    ));
    assert!(matches!(
        failure_policy(
            id,
            &WorkKind::ForgeJudge,
            3,
            3,
            true,
            FailureClass::InvalidOutput
        ),
        AttemptDecision::JudgeFailed(_)
    ));
}
