use super::parser_tests::{event, nick};
use super::*;
use crate::{
    Actor, CellId, ContestId, ContestKind, ContestState, Cursor, ErrorCode, GenericContestState,
    Recipient, Request, Text,
};
use serde_json::json;

fn context() -> ParseContext {
    ParseContext {
        roster: ["claude", "gemini", "grok", "muse"].map(nick).to_vec(),
        ..Default::default()
    }
}
fn state(kind: ContestKind, status: &str, version: u64) -> ContestState {
    ContestState::Generic(GenericContestState {
        contest_id: contest(),
        kind,
        status: Text::new(status).unwrap(),
        state_version: version,
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
    })
}
fn contest() -> ContestId {
    event(40).to_string().parse().unwrap()
}
fn cell() -> CellId {
    "550e8400-e29b-41d4-a716-446655440000".parse().unwrap()
}
fn record(kind: &str) -> RecordSummary {
    RecordSummary {
        event_id: event(1),
        message_id: None,
        seq: Cursor::new(1).unwrap(),
        kind: Text::new(kind).unwrap(),
        author: Actor::Member(nick("grok")),
        recipients: vec![Recipient::Me],
        content: String::new(),
        in_reply_to_message_id: Some(event(2).to_string().parse().unwrap()),
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
fn complete() -> RecordSummary {
    RecordSummary {
        contest_id: Some(contest()),
        state_version: Some(4),
        plan_event_id: Some(event(3)),
        grid_event_id: Some(event(4)),
        cell_ids: vec![cell()],
        content: json!({"status":"COMPLETE"}).to_string(),
        ..record("crosscheck.state")
    }
}
fn request(raw: &str, meta: &SubmissionMeta, ctx: &ParseContext) -> Result<Request, ParseError> {
    let parsed = parse(raw, meta, ctx)?;
    Request::from_parsed_with_context(event(99), parsed, meta, ctx, false)
}

#[test]
fn continuation_keeps_replaces_and_quotes_without_expansion() {
    let mut ctx = context();
    ctx.active_contest = Some(state(ContestKind::Census, "WAITING_FOR_PLAN", 7));
    let meta = SubmissionMeta::default();
    for (raw, replacement) in [
        ("/continue", None),
        (
            r#"/continue "Plan with \"quotes\", @all and $ROUNDS=0""#,
            Some("Plan with \"quotes\", @all and $ROUNDS=0"),
        ),
        ("/continue \"Multiline\\nplan\"", Some("Multiline\nplan")),
    ] {
        let req = request(raw, &meta, &ctx).unwrap();
        assert!(
            matches!(req, Request::CensusContinue { contest_id, expected_state_version: 7, plan_event_id: None, replacement_text, .. }
            if contest_id == contest() && replacement_text.as_ref().map(Text::as_str) == replacement)
        );
        let ast = parse(raw, &meta, &ctx).unwrap();
        let canonical = match &ast {
            ParsedInput::Slash {
                action: SlashAction::Continue { plan },
                ..
            } => plan.as_ref().map_or("/continue".into(), |p| {
                format!("/continue {}", serde_json::to_string(p).unwrap())
            }),
            _ => unreachable!(),
        };
        assert_eq!(
            serde_json::to_value(ast).unwrap()["action"],
            serde_json::to_value(parse(&canonical, &meta, &ctx).unwrap()).unwrap()["action"]
        );
    }
    insta::assert_json_snapshot!(
        "continue_asts",
        [
            parse("/continue", &meta, &ctx).unwrap(),
            parse(r#"/continue "Replacement @all $ROUNDS=0""#, &meta, &ctx).unwrap(),
        ]
    );
    ctx.records.push(record("grill.plan"));
    let quote = SubmissionMeta {
        quote: vec![event(1)],
        ..Default::default()
    };
    assert!(
        matches!(request("/continue", &quote, &ctx).unwrap(), Request::CensusContinue { plan_event_id: Some(id), replacement_text: None, .. } if id == event(1))
    );
    ctx.records[0].kind = Text::new("message.final").unwrap();
    assert!(request("/continue", &quote, &ctx).is_ok());
    for raw in [
        "/continue unquoted",
        "/continue \"\"",
        "/continue \" \"",
        "/continue \"a\" \"b\"",
        "/continue \"unterminated",
        "/continue 10",
    ] {
        let error = parse(raw, &meta, &ctx).unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_COMMAND, "{raw}");
        assert_eq!(error.message, "Usage: /continue [\"<plan>\"]");
    }
    assert_eq!(
        parse("/continue \"replacement\"", &quote, &ctx)
            .unwrap_err()
            .code,
        ErrorCode::INVALID_COMMAND
    );
}

#[test]
fn continuation_checks_attachments_and_authoritative_wait() {
    let mut ctx = context();
    let mut meta = SubmissionMeta::default();
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().code,
        ErrorCode::INVALID_STATE_TRANSITION
    );
    for (kind, status) in [
        (ContestKind::Census, "COMPLETE"),
        (ContestKind::Census, "HUNTING"),
        (ContestKind::Crosscheck, "WAITING_FOR_PLAN"),
    ] {
        ctx.active_contest = Some(state(kind, status, 7));
        assert_eq!(
            parse("/continue", &meta, &ctx).unwrap_err().message,
            "No census is waiting for a plan."
        );
    }
    ctx.active_contest = Some(state(ContestKind::Census, "WAITING_FOR_PLAN", 7));
    meta.quote = vec![event(1)];
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().code,
        ErrorCode::INVALID_REFERENCE
    );
    ctx.records.push(record("other"));
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().message,
        "Select exactly one member reply or grill plan version for /continue."
    );
    ctx.records[0].kind = Text::new("message.final").unwrap();
    ctx.records[0].superseded = true;
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().code,
        ErrorCode::CENSUS_SOURCE_MISSING
    );
    ctx.records[0].superseded = false;
    meta.quote.push(event(1));
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().code,
        ErrorCode::CENSUS_SOURCE_MISSING
    );
    meta.quote.clear();
    meta.reply_to.push(event(1));
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().code,
        ErrorCode::INVALID_REFERENCE
    );
    meta.reply_to.clear();
    meta.quote = vec![event(1); 13];
    assert_eq!(
        parse("/continue", &meta, &ctx).unwrap_err().message,
        "Attach no more than 12 quoted messages."
    );
}

#[test]
fn acceptance_binds_complete_target_and_retains_literal_reason() {
    let mut ctx = context();
    ctx.records.push(complete());
    let raw = format!(
        "/accept {} -- Risk accepted:  @all $ROUNDS=0\nkeep  spacing  ",
        cell()
    );
    let parsed = parse(&raw, &SubmissionMeta::default(), &ctx).unwrap();
    let req = request(&raw, &SubmissionMeta::default(), &ctx).unwrap();
    assert!(
        matches!(req, Request::CrosscheckAccept { contest_id, expected_state_version:4, plan_event_id, grid_event_id, cell_ids, reason, .. }
        if contest_id == contest() && plan_event_id == event(3) && grid_event_id == event(4) && cell_ids == vec![cell()] && reason.as_str() == "Risk accepted:  @all $ROUNDS=0\nkeep  spacing  ")
    );
    insta::assert_json_snapshot!("accept_ast", parsed);
    let mut newer = complete();
    newer.seq = Cursor::new(10).unwrap();
    newer.state_version = Some(5);
    newer.plan_event_id = Some(event(8));
    ctx.records.insert(0, newer); // sequence order, not vector order, selects the target.
    assert!(
        matches!(request(&raw, &SubmissionMeta::default(), &ctx).unwrap(), Request::CrosscheckAccept { expected_state_version:5, plan_event_id, .. } if plan_event_id == event(8))
    );
    ctx.records[0].content = json!({"status":"RUNNING"}).to_string();
    assert_eq!(
        parse(&raw, &SubmissionMeta::default(), &ctx)
            .unwrap_err()
            .code,
        ErrorCode::CONTEST_NOT_FOUND
    );
}

#[test]
fn acceptance_shape_identifiers_cells_and_staleness() {
    let mut ctx = context();
    let meta = SubmissionMeta::default();
    let raw = format!("/accept {} -- reason", cell());
    assert_eq!(
        parse(&raw, &meta, &ctx).unwrap_err().message,
        "No complete crosscheck is available for acceptance."
    );
    ctx.records.push(complete());
    for raw in [
        "/accept".into(),
        "/accept -- reason".into(),
        format!("/accept {}", cell()),
        format!("/accept {} --   ", cell()),
        format!("/accept {} {} -- reason", cell(), cell()),
    ] {
        let error = parse(&raw, &meta, &ctx).unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_COMMAND);
        assert_eq!(error.message, "Usage: /accept <cellId…> -- <reason>");
    }
    for bad in ["nope", "01900000-0000-7000-8000-000000000001"] {
        let error = parse(&format!("/accept {bad} -- reason"), &meta, &ctx).unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_COMMAND);
        assert_eq!(error.message, format!("Invalid cellId: {bad}."));
        assert_eq!(
            &format!("/accept {bad} -- reason")[error.span.start..error.span.end],
            bad
        );
    }
    let unknown = "550e8400-e29b-41d4-a716-446655440001";
    assert_eq!(
        parse(&format!("/accept {unknown} -- reason"), &meta, &ctx)
            .unwrap_err()
            .code,
        ErrorCode::CROSSCHECK_ACCEPTANCE_STALE
    );
    let parsed = parse(&raw, &meta, &ctx).unwrap();
    for field in ["plan", "grid", "state", "cell"] {
        let mut stale = ctx.clone();
        match field {
            "plan" => stale.records[0].plan_event_id = None,
            "grid" => stale.records[0].grid_event_id = None,
            "state" => stale.records[0].state_version = None,
            _ => stale.records[0].cell_ids.clear(),
        }
        assert_eq!(
            validate_context(&parsed, &meta, &stale).unwrap_err().code,
            ErrorCode::CROSSCHECK_ACCEPTANCE_STALE
        );
    }
    let quote = SubmissionMeta {
        quote: vec![event(1)],
        ..Default::default()
    };
    assert_eq!(
        parse(&raw, &quote, &ctx).unwrap_err().message,
        "Quotes require a conversational message."
    );
}

#[test]
fn canonical_boundary_defaults_updates_and_reference_summaries() {
    let mut ctx = context();
    let meta = SubmissionMeta::default();
    let raw =
        "@grok Use $ROUNDS $GRILL_ROUNDS $CENSUS_PASSES $ROUNDS=3 $GRILL_ROUNDS=4 $CENSUS_PASSES=5";
    let req = request(raw, &meta, &ctx).unwrap();
    assert!(
        matches!(req, Request::UserMessage { parsed: ParsedInput::Message { body, input, .. }, .. } if body == "Use 3 4 5" && input.variable_updates.len() == 3)
    );
    assert!(ctx.vars.is_empty());
    assert!(parse("$ROUNDS=3 $GRILL_ROUNDS=4 $CENSUS_PASSES=0", &meta, &ctx).is_err());
    assert!(ctx.vars.is_empty());
    assert!(
        matches!(request("$ROUNDS=3 $GRILL_ROUNDS=4 $CENSUS_PASSES=5", &meta, &ctx).unwrap(), Request::SetVars { updates, .. } if updates.len() == 3)
    );
    ctx.vars.insert(VariableName::Rounds, 7);
    assert!(matches!(
        parse("@grok $debate @gemini", &meta, &ctx).unwrap(),
        ParsedInput::Debate { rounds: 7, .. }
    ));
    ctx.records.push(record("message.final"));
    let selected = SubmissionMeta {
        reply_to: vec![event(1)],
        ..Default::default()
    };
    assert!(request("@grok @gemini $debate Compare", &selected, &ctx).is_ok());
    ctx.records[0].superseded = true;
    assert_eq!(
        parse("@grok @gemini $debate Compare", &selected, &ctx)
            .unwrap_err()
            .code,
        ErrorCode::DEBATE_SOURCE_MISSING
    );
}

#[test]
fn standalone_judge_and_quoted_messages_keep_metadata_in_the_request() {
    let mut ctx = context();
    ctx.records.push(record("message.final"));
    let meta = SubmissionMeta {
        quote: vec![event(1)],
        ..Default::default()
    };
    for raw in ["@judge Decide", "@grok Critique this"] {
        let req = request(raw, &meta, &ctx).unwrap();
        assert!(matches!(req, Request::UserMessage { input, parsed, .. }
            if input.quote == Some(vec![event(1)]) && parsed.conversational().unwrap().quote == vec![event(1)]));
    }
}

#[test]
fn canonical_survey_batch_uses_latest_eligible_question_without_merging() {
    let mut ctx = context();
    let mut question = record("user.message");
    question.author = Actor::Me;
    question.message_id = Some(event(2).to_string().parse().unwrap());
    question.recipients = ctx.roster.iter().cloned().map(Recipient::Member).collect();
    question.in_reply_to_message_id = None;
    ctx.records.push(question.clone());
    let mut answer = record("message.final");
    answer.event_id = event(5);
    answer.seq = Cursor::new(2).unwrap();
    ctx.records.push(answer);
    question.event_id = event(8);
    question.message_id = Some(event(8).to_string().parse().unwrap());
    question.seq = Cursor::new(3).unwrap();
    ctx.records.push(question);
    // A later unanswered question is not an eligible answer set.
    assert!(parse("@grok $survey", &SubmissionMeta::default(), &ctx).is_ok());
    let mut newer_answer = record("message.final");
    newer_answer.event_id = event(9);
    newer_answer.seq = Cursor::new(4).unwrap();
    newer_answer.author = Actor::Member(nick("gemini"));
    newer_answer.in_reply_to_message_id = Some(event(8).to_string().parse().unwrap());
    ctx.records.push(newer_answer);
    assert_eq!(
        parse("@grok $survey", &SubmissionMeta::default(), &ctx)
            .unwrap_err()
            .code,
        ErrorCode::SURVEY_SOURCE_MISSING
    );
    assert!(parse("@gemini $survey", &SubmissionMeta::default(), &ctx).is_ok());
}

#[test]
fn snapshot_controls_reject_invalid_ast_shapes_and_changed_active_state() {
    let mut ctx = context();
    ctx.records.push(complete());
    let meta = SubmissionMeta::default();
    let raw = format!("/accept {} -- reason", cell());
    let parsed = parse(&raw, &meta, &ctx).unwrap();
    ctx.active_contest = Some(state(ContestKind::Crosscheck, "COMPLETE", 9));
    assert_eq!(
        validate_context(&parsed, &meta, &ctx).unwrap_err().code,
        ErrorCode::CROSSCHECK_ACCEPTANCE_STALE
    );
    ctx.active_contest = None;
    for kind in ["crosscheck.plan", "crosscheck.grid"] {
        let mut stale = ctx.clone();
        let mut newer = record(kind);
        newer.event_id = event(10);
        newer.contest_id = Some(contest());
        newer.seq = Cursor::new(10).unwrap();
        stale.records.push(newer);
        assert_eq!(
            validate_context(&parsed, &meta, &stale).unwrap_err().code,
            ErrorCode::CROSSCHECK_ACCEPTANCE_STALE
        );
    }
    for (cells, reason) in [
        (vec![], "reason"),
        (vec![cell(), cell()], "reason"),
        (vec![cell()], " "),
    ] {
        let parsed = ParsedInput::Slash {
            raw: raw.clone(),
            action: SlashAction::Accept {
                cell_ids: cells,
                reason: reason.into(),
            },
        };
        assert_eq!(
            Request::from_parsed_with_context(event(99), parsed, &meta, &ctx, false)
                .unwrap_err()
                .code,
            ErrorCode::INVALID_COMMAND
        );
    }
}

proptest::proptest! {
    #[test]
    fn continuation_and_acceptance_round_trip_literal_text(text in "[\\PC]{1,120}") {
        proptest::prop_assume!(!text.trim().is_empty());
        let meta = SubmissionMeta::default();
        let mut ctx = context();
        ctx.active_contest = Some(state(ContestKind::Census, "WAITING_FOR_PLAN", 7));
        let raw = format!("/continue {}", serde_json::to_string(&text).unwrap());
        let ast = parse(&raw, &meta, &ctx).unwrap();
        proptest::prop_assert!(matches!(&ast, ParsedInput::Slash { action: SlashAction::Continue { plan:Some(plan) }, .. } if plan == &text), "literal replacement survives JSON decoding");
        let reparsed = parse(&raw, &meta, &ctx).unwrap();
        proptest::prop_assert_eq!(&ast, &reparsed);
        proptest::prop_assert_eq!(ast.raw(), raw.as_str());
        ctx.active_contest = None; ctx.records.push(complete());
        let raw = format!("/accept {} -- {}", cell(), text.trim_start());
        let ast = parse(&raw, &meta, &ctx).unwrap();
        proptest::prop_assert!(matches!(&ast, ParsedInput::Slash { action: SlashAction::Accept { reason, .. }, .. } if reason == text.trim_start()), "literal reason survives parsing");
        proptest::prop_assert_eq!(&ast, &parse(&raw, &meta, &ctx).unwrap());
        proptest::prop_assert_eq!(ast.raw(), raw.as_str());
    }
}
