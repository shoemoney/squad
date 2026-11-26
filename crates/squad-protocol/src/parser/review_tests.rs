use super::{
    errors::{self, Rule},
    parser_tests::{context, event, message, nick, pending, reply},
    *,
};
use super::{parse_fixture as parse, validate_snapshot as validate_context};
use crate::{ContestKind, ErrorCode};

fn body(raw: &str) -> String {
    let ParsedInput::Message { body, .. } = parse(raw, &context()).unwrap() else {
        panic!("expected message: {raw}")
    };
    body
}

#[test]
fn escapes_follow_the_single_backslash_grammar() {
    assert_eq!(body(r"@grok \@gemini"), "@gemini");
    // Exact divergence input: the escaped backslash prevents a token boundary.
    assert_eq!(body(r"@grok \\@gemini"), r"\@gemini");
    assert_eq!(body(r"@grok \\$ROUNDS"), r"\$ROUNDS");
}

#[test]
fn census_conflict_competes_with_assignment_errors_leftmost() {
    let mut ctx = context();
    ctx.metadata.quote = vec![event(1)];
    for (raw, rule) in [
        ("@all $census Plan $ROUNDS=0", Rule::CensusConflict),
        ("$ROUNDS=0 @all $census Plan", Rule::Rounds),
        ("@all $census $OTHER Plan", Rule::UnknownVariable),
        ("@all $census Plan $OTHER", Rule::CensusConflict),
    ] {
        let err = parse(raw, &ctx).unwrap_err();
        assert_eq!(err.code, rule.code(), "{raw}");
        assert_eq!(
            err.message,
            rule.error(err.span.clone(), &[("name", "OTHER")]).message,
            "{raw}"
        );
    }
}

#[test]
fn slash_attachment_errors_precede_unknown_recipients() {
    let mut ctx = context();
    ctx.metadata.quote = vec![event(1)];
    let err = parse("/stop @missing", &ctx).unwrap_err();
    assert_eq!(
        err,
        Rule::ControlQuotes.error(Span { start: 0, end: 14 }, &[])
    );
    ctx.metadata.quote.clear();
    ctx.metadata.reply_to = vec![event(1)];
    assert_eq!(
        parse("/stop @missing", &ctx).unwrap_err().code,
        ErrorCode::INVALID_REFERENCE
    );
    assert_eq!(
        parse("/stop @", &context()).unwrap_err().message,
        "Usage: /stop [@nick]"
    );
}

#[test]
fn exclusions_fail_before_later_recipient_errors_at_the_all_span() {
    let mut ctx = context();
    ctx.roster = vec![nick("grok"), nick("gemini")];
    for raw in [
        "@all -grok -gemini @unknown hi",
        "@all -grok -gemini @all hi",
    ] {
        assert_eq!(
            parse(raw, &ctx).unwrap_err(),
            Rule::AllEmpty.error(Span { start: 0, end: 4 }, &[])
        );
    }
}

#[test]
fn a_second_all_is_always_a_combination_error() {
    for raw in ["@all @all hi", "@all -grok @all hi", "@grok @all hi"] {
        let err = parse(raw, &context()).unwrap_err();
        assert_eq!(err.message, Rule::AllMixed.template());
        let start = raw.rfind("@all").unwrap();
        assert_eq!(
            err.span,
            Span {
                start,
                end: start + 4
            }
        );
    }
}

#[test]
fn whitespace_collapses_only_on_both_sides_of_removed_tokens() {
    assert_eq!(body("@grok A\n@gemini!"), "A\n!");
    assert_eq!(body("@grok A\n@gemini !"), "A !");
    assert_eq!(body("@grok A (@gemini) B"), "A () B");
    assert_eq!(body("@grok A  B"), "A  B");
}

#[test]
fn removed_mentions_never_join_text_to_a_closing_fence() {
    for newline in ["\n", "\r\n"] {
        for removed in ["@gemini", "@gemini @claude", "@gemini\n@claude"] {
            let raw =
                format!("@grok{newline}```{newline}code{newline}```   {newline}{removed} hello");
            let rendered = body(&raw);
            assert!(
                rendered.starts_with(&format!("```{newline}code{newline}```   {newline}")),
                "{rendered:?}"
            );
            assert_eq!(rendered.lines().last().unwrap(), "hello");
        }
    }
    assert_eq!(
        body("@grok\n```\ncode\n```   \n@gemini"),
        "```\ncode\n```   "
    );
    assert_eq!(body("@grok `code ` \n@gemini hi"), "`code ` hi");
}

#[test]
fn pick_eligible_order_is_supplied_and_ineligible_span_is_the_mention() {
    let mut ctx = context();
    let mut pick = pending(QuestionFirstMode::Debate);
    pick.eligible = vec![nick("gemini"), nick("grok"), nick("claude")];
    ctx.pending_pick = Some(pick);
    assert_eq!(
        parse("@grok", &ctx).unwrap_err().message,
        "Pick exactly two of: @gemini, @grok, @claude."
    );
    let raw = "@grok @muse";
    assert_eq!(
        parse(raw, &ctx).unwrap_err(),
        Rule::PickIneligible.error(Span { start: 6, end: 11 }, &[("nick", "muse")])
    );
    for raw in ["@grok @unknown", "@Grok @gemini"] {
        assert_eq!(
            parse(raw, &ctx).unwrap_err().code,
            ErrorCode::UNKNOWN_RECIPIENT
        );
    }
}

#[test]
fn stale_pick_uses_the_shared_diagnostic() {
    let mut ctx = context();
    ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
    let parsed = parse("@grok @gemini", &ctx).unwrap();
    ctx.pending_pick = None;
    assert_eq!(
        validate_context(&parsed, &ctx).unwrap_err(),
        Rule::StalePick.error(Span { start: 0, end: 13 }, &[])
    );
}

#[test]
fn reply_to_binds_by_author_not_attachment_position() {
    let mut ctx = context();
    ctx.records
        .extend([reply(1, "grok", 30), reply(2, "gemini", 30)]);
    for selected in [vec![event(2), event(1)], vec![event(2)]] {
        ctx.metadata.reply_to = selected.clone();
        assert!(
            matches!(parse("@grok @gemini $debate", &ctx).unwrap(), ParsedInput::Debate { reply_to, .. } if reply_to == selected)
        );
    }
}

#[test]
fn question_first_grill_uses_the_question_selection_error() {
    let mut ctx = context();
    ctx.metadata.reply_to = vec![event(1)];
    assert_eq!(
        parse("@all $grill Question", &ctx).unwrap_err().message,
        Rule::QuestionSelection.template()
    );
}

#[test]
fn pending_pick_counts_as_an_active_contest() {
    let mut ctx = context();
    for mode in [QuestionFirstMode::Debate, QuestionFirstMode::Grill] {
        ctx.pending_pick = Some(pending(mode.clone()));
        let active = if mode == QuestionFirstMode::Debate {
            ContestKind::Debate
        } else {
            ContestKind::Grill
        };
        let raw = "@all $survey Question";
        assert_eq!(
            parse(raw, &ctx).unwrap_err(),
            errors::contest_busy(
                ContestKind::Survey,
                active,
                Span {
                    start: 0,
                    end: raw.len()
                }
            )
        );
        assert!(parse("@grok hello", &ctx).is_ok());
        assert!(parse("/stop", &ctx).is_ok());
    }
}

#[test]
fn unknown_second_verb_is_a_multiple_command_error() {
    let raw = "@all $survey Question $unknown";
    assert_eq!(
        parse(raw, &context()).unwrap_err(),
        Rule::MultipleCommands.error(Span { start: 22, end: 30 }, &[])
    );
}

#[test]
fn judge_overrides_only_recipient_combination_errors() {
    for raw in ["@unknown @judge Q", "@me @judge Q"] {
        let error = parse(raw, &context()).unwrap_err();
        assert_eq!(error.span.start, 0);
        assert_ne!(error.code, ErrorCode::JUDGE_QUOTE_REQUIRED);
    }
    for raw in ["@all @grok @judge Q", "@all @all @judge Q"] {
        assert_eq!(
            parse(raw, &context()).unwrap_err().code,
            ErrorCode::JUDGE_QUOTE_REQUIRED
        );
    }
    // B regression cases, preserving A's token-position precedence.
    assert_eq!(
        parse("@unknown $grill @judge", &context())
            .unwrap_err()
            .message,
        "Unknown recipient: @unknown."
    );
    assert_eq!(
        parse("@judge $grill @unknown", &context())
            .unwrap_err()
            .message,
        Rule::GrillJudge.template()
    );
}

#[test]
fn room_export_accepts_no_identifier() {
    let ctx = context();
    assert!(parse("/export room", &ctx).is_ok());
    let raw = format!("/export room {}", event(1));
    assert!(
        parse(&raw, &ctx)
            .unwrap_err()
            .message
            .starts_with("Usage: /export room")
    );
    assert!(parse(&format!("/export debate {}", event(1)), &ctx).is_ok());
}

#[test]
fn grill_selector_and_count_errors_have_the_participant_code() {
    let mut ctx = context();
    ctx.roster = vec![nick("grok")];
    for raw in ["$grill", "@grok $grill"] {
        assert_eq!(
            parse(raw, &ctx).unwrap_err().code,
            ErrorCode::GRILL_PARTICIPANTS_REQUIRED
        );
    }
}

#[test]
fn question_first_grill_minimum_and_missing_survey_selector_codes() {
    let mut ctx = context();
    ctx.roster = vec![nick("grok")];
    assert_eq!(
        parse("@all $grill Question", &ctx).unwrap_err().code,
        ErrorCode::INVALID_COMMAND
    );
    assert_eq!(
        parse("$survey Question", &ctx).unwrap_err().code,
        ErrorCode::SURVEY_PARTICIPANTS_REQUIRED
    );
}

#[test]
fn survey_batch_ids_are_revalidated_against_authoritative_records() {
    let mut ctx = context();
    ctx.records
        .extend([reply(1, "grok", 30), reply(2, "gemini", 31)]);
    ctx.latest_parallel_answers = Some(ParallelAnswerSet {
        question: message(30),
        answers: vec![event(1), event(2)],
    });
    let parsed = parse("@grok $survey", &ctx).unwrap();
    // A different question, absent record, failed reply, or wrong direction cannot provide the batch.
    for invalid in 0..5 {
        let mut stale = ctx.clone();
        match invalid {
            0 => {
                stale.records.remove(0);
            }
            1 => stale.records[0].eligible = false,
            2 => stale.records[0] = reply(1, "grok", 32),
            3 => stale.records[0] = reply(1, "muse", 30),
            _ => {
                if let RecordKind::MemberReply { to_me, .. } = &mut stale.records[0].kind {
                    *to_me = false;
                }
            }
        }
        assert_eq!(
            validate_context(&parsed, &stale).unwrap_err().code,
            ErrorCode::SURVEY_SOURCE_MISSING
        );
    }
}

#[test]
fn metadata_round_trips_camel_case_and_help_examples_parse() {
    let metadata = SubmissionMetadata {
        quote: vec![event(1)],
        reply_to: vec![event(2)],
    };
    let value = serde_json::to_value(&metadata).unwrap();
    assert!(value.get("replyTo").is_some());
    assert!(value.get("reply_to").is_none());
    assert_eq!(
        serde_json::from_value::<SubmissionMetadata>(value).unwrap(),
        metadata
    );
    let mut ctx = context();
    ctx.records.push(reply(1, "grok", 30));
    for raw in HELP_EXAMPLES {
        let result = parse(raw, &ctx);
        if raw.starts_with("/continue") {
            assert_eq!(
                result.unwrap_err().code,
                ErrorCode::INVALID_STATE_TRANSITION
            );
        } else if raw.starts_with("/accept") {
            assert_eq!(result.unwrap_err().code, ErrorCode::CONTEST_NOT_FOUND);
        } else {
            assert!(result.is_ok(), "{raw}");
        }
    }
}

#[test]
fn complete_exact_validation_table_and_execution_helpers() {
    let expected: Vec<String> =
        serde_json::from_str(include_str!("validation_strings.json")).unwrap();
    assert_eq!(expected.len(), errors::ALL_RULES.len());
    let codes: Vec<ErrorCode> =
        serde_json::from_str(include_str!("validation_codes.json")).unwrap();
    assert_eq!(codes.len(), errors::ALL_RULES.len());
    for (rule, code) in errors::ALL_RULES.iter().zip(codes) {
        assert_eq!(rule.code(), code, "{}", rule.template());
    }
    let values = [
        ("token", "$Bad"),
        ("verb", "dance"),
        ("name", "OTHER"),
        ("usage", "/help"),
        ("argument", "id"),
        ("value", "bad"),
        ("id", "missing"),
        ("nick", "grok"),
        ("kind", "survey"),
        ("mode", "forge"),
        ("x", "b"),
        ("eligible", "@grok, @gemini"),
        ("n", "1"),
        ("m", "6"),
        ("max", "20.00"),
    ];
    for (rule, text) in errors::ALL_RULES.iter().zip(expected) {
        assert_eq!(rule.template(), text);
        assert!(
            !rule
                .error(Span { start: 0, end: 0 }, &values)
                .message
                .contains('{')
        );
    }
    let span = Span { start: 0, end: 0 };
    assert_eq!(
        errors::daily_max(20.0, span.clone()),
        Rule::DailyMax.error(span.clone(), &[("max", "20.00")])
    );
    assert_eq!(
        errors::insufficient_answers(1, 6, span.clone()),
        Rule::InsufficientAnswers.error(span.clone(), &[("n", "1"), ("m", "6")])
    );
    assert_eq!(
        errors::contest_busy(ContestKind::Debate, ContestKind::Grill, span).code,
        ErrorCode::DEBATE_BUSY
    );
}
