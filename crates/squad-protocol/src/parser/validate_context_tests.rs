use super::{
    SourceSummary as RecordSummary, parse_fixture as parse, validate_snapshot as validate_context,
};
use super::{
    errors::Rule,
    parser_tests::{context, event, message, pending, reply},
    *,
};
use crate::{ContestKind, ErrorCode};

#[test]
fn quoted_text_is_never_executed_and_judge_contexts() {
    let mut ctx = context();
    ctx.metadata.quote = vec![event(1)];
    ctx.records.push(RecordSummary {
        id: event(1),
        kind: RecordKind::Verdict,
        eligible: true,
    });
    for raw in [
        "@judge Decide",
        "@grok Critique this",
        "@all $survey Which option?",
    ] {
        assert!(parse(raw, &ctx).is_ok());
    }
    for raw in [
        "@all @judge Decide",
        "@judge @grok Decide",
        "@judge @me Decide",
    ] {
        assert_eq!(
            parse(raw, &ctx).unwrap_err().code,
            ErrorCode::JUDGE_QUOTE_REQUIRED
        );
    }
    assert!(matches!(
        parse("@grok @gemini $debate @judge", &ctx).unwrap(),
        ParsedInput::Debate {
            judge_requested: true,
            ..
        }
    ));
    assert!(matches!(
        parse("@all $tournament Question @judge", &ctx).unwrap(),
        ParsedInput::Tournament { .. }
    ));
    assert_eq!(
        parse("@grok $grill @judge", &ctx).unwrap_err().to_string(),
        "@judge cannot be used with $grill."
    );
    assert!(matches!(
        parse("@grok Explain `@judge $debate $ROUNDS=0`", &ctx).unwrap(),
        ParsedInput::Message { .. }
    ));
}
#[test]
fn direct_solutions_are_optional_but_explicit_selections_are_strict() {
    let mut ctx = context();
    let raw = "@grok @gemini $debate Fresh topic";
    assert!(parse(raw, &ctx).is_ok());
    ctx.records.push(reply(1, "grok", 30));
    assert!(parse(raw, &ctx).is_ok());
    ctx.metadata.reply_to = vec![event(1), event(2)];
    assert_eq!(
        parse(raw, &ctx).unwrap_err().to_string(),
        "Selected solution is not a reply to me from @gemini."
    );
    ctx.records.push(reply(2, "claude", 30));
    assert_eq!(
        parse(raw, &ctx).unwrap_err().code,
        ErrorCode::DEBATE_SOURCE_MISSING
    );
    ctx.records[1] = reply(2, "gemini", 31);
    assert_eq!(
        parse(raw, &ctx).unwrap_err().code,
        ErrorCode::DEBATE_TOPIC_MISMATCH
    );
    ctx.records[1] = reply(2, "gemini", 30);
    let parsed = parse(raw, &ctx).unwrap();
    ctx.records[1].eligible = false;
    assert_eq!(
        validate_context(&parsed, &ctx).unwrap_err().code,
        ErrorCode::DEBATE_SOURCE_MISSING
    );
    ctx.metadata.reply_to = vec![event(1), event(2), event(3)];
    assert_eq!(
        parse(raw, &ctx).unwrap_err().to_string(),
        "Select no more than two solutions for $debate."
    );
}
#[test]
fn sources_and_stale_plan_summaries() {
    let mut ctx = context();
    ctx.metadata.quote = vec![event(1), event(2)];
    ctx.records.extend([
        reply(1, "grok", 30),
        RecordSummary {
            id: event(2),
            kind: RecordKind::Other,
            eligible: true,
        },
    ]);
    for raw in ["@all $census", "@all $crosscheck Goal", "@grok $grill"] {
        let parsed = parse(raw, &ctx).unwrap();
        let mut stale = ctx.clone();
        stale.records[0].eligible = false;
        assert!(validate_context(&parsed, &stale).is_err());
        stale.records.remove(0);
        let err = validate_context(&parsed, &stale).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("Quoted message {} is unavailable.", event(1))
        );
    }
    ctx.records[0].kind = RecordKind::GrillPlan;
    assert!(parse("@all $census", &ctx).is_ok());
    assert!(parse("@all $crosscheck", &ctx).is_ok());
    assert_eq!(
        parse("@grok $grill", &ctx).unwrap_err().to_string(),
        "Select exactly one reply from @grok as the plan to grill."
    );
    ctx.metadata.quote.clear();
    ctx.records.clear();
    ctx.records.push(reply(3, "grok", 30));
    for raw in ["@grok $census", "@grok $crosscheck Goal", "@grok $grill"] {
        let parsed = parse(raw, &ctx).unwrap();
        let mut stale = ctx.clone();
        stale.records[0].eligible = false;
        assert!(validate_context(&parsed, &stale).is_err());
    }
    // Explicit participant sets and @all never infer a source from the first member.
    assert!(parse("@grok @gemini @claude $census", &ctx).is_err());
    assert!(parse("@all $crosscheck", &ctx).is_err());
    assert!(
        matches!(parse("@grok $census Pasted",&ctx).unwrap(),ParsedInput::Census{plan:PlanSource::Pasted{text},..} if text=="Pasted")
    );
}
#[test]
fn survey_reuse_is_one_batch_not_unrelated_latest_replies() {
    let mut ctx = context();
    ctx.records
        .extend([reply(1, "grok", 30), reply(2, "gemini", 31)]);
    assert_eq!(
        parse("@all $survey", &ctx).unwrap_err().code,
        ErrorCode::SURVEY_SOURCE_MISSING
    );
    ctx.latest_parallel_answers = Some(ParallelAnswerSet {
        question: message(30),
        answers: vec![event(1)],
    });
    let parsed = parse("@all $survey", &ctx).unwrap();
    assert!(parse("@all -grok $survey", &ctx).is_err());
    ctx.latest_parallel_answers
        .as_mut()
        .unwrap()
        .answers
        .clear();
    assert!(validate_context(&parsed, &ctx).is_err());
    assert!(parse("@all $survey New question", &ctx).is_ok());
}
#[test]
fn picks_revalidate_count_eligibility_and_identity() {
    let mut ctx = context();
    ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
    let before = format!("{:?}", ctx.pending_pick);
    for (raw, expected) in [
        ("@muse", "Pick exactly two of: @claude, @gemini, @grok."),
        (
            "@grok @grok",
            "Pick exactly two of: @claude, @gemini, @grok.",
        ),
        (
            "@muse @fable",
            "@muse did not answer, so it cannot be picked.",
        ),
    ] {
        assert_eq!(parse(raw, &ctx).unwrap_err().to_string(), expected);
    }
    assert_eq!(before, format!("{:?}", ctx.pending_pick));
    let parsed = parse("@grok @gemini", &ctx).unwrap();
    ctx.pending_pick
        .as_mut()
        .unwrap()
        .eligible
        .retain(|n| n.as_str() != "gemini");
    assert_eq!(
        validate_context(&parsed, &ctx).unwrap_err().code,
        ErrorCode::CONTEST_PICK_INVALID
    );
    ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
    ctx.pending_pick.as_mut().unwrap().contest_id = event(21).to_string().parse().unwrap();
    assert_eq!(
        validate_context(&parsed, &ctx).unwrap_err().code,
        ErrorCode::CONTEST_PICK_INVALID
    );
    ctx.pending_pick = None;
    assert!(validate_context(&parsed, &ctx).is_err());
    ctx.pending_pick = Some(pending(QuestionFirstMode::Grill));
    assert!(matches!(
        parse("@grok Explain", &ctx).unwrap(),
        ParsedInput::Message { .. }
    ));
    assert!(matches!(
        parse("@grok `@gemini`", &ctx).unwrap(),
        ParsedInput::Message { .. }
    ));
    assert!(matches!(
        parse("@grok $ROUNDS", &ctx).unwrap(),
        ParsedInput::Message { .. }
    ));
    assert!(
        matches!(parse("@grok $ROUNDS=3",&ctx),Err(e) if e.to_string()=="Enter message text after the recipients.")
    );
}
#[test]
fn all_modes_against_each_active_kind_and_pending_pick() {
    let mut ctx = context();
    ctx.records.push(reply(1, "grok", 30));
    let modes = [
        "@grok @gemini $debate Topic",
        "@all $tournament Topic",
        "@grok $grill",
        "@all $grill Topic",
        "@all $debate Topic",
        "@all $census Plan",
        "@all $crosscheck Plan",
        "@all $survey Topic",
        "@all $probe Brief",
    ];
    for raw in modes {
        ctx.active_contest = None;
        ctx.pending_pick = None;
        // Busy members are execution scheduling state, absent from parser facts.
        assert!(parse(raw, &ctx).is_ok(), "{raw}");
        ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
        for kind in [
            ContestKind::Debate,
            ContestKind::Tournament,
            ContestKind::Grill,
            ContestKind::Census,
            ContestKind::Crosscheck,
            ContestKind::Survey,
            ContestKind::Probe,
        ] {
            ctx.active_contest = Some(kind);
            let error = parse(raw, &ctx).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "A {} is already running. Stop it before starting another.",
                    kind.as_str()
                )
            );
            assert_eq!(
                error.code,
                if raw.contains("$debate") {
                    ErrorCode::DEBATE_BUSY
                } else {
                    ErrorCode::CONTEST_BUSY
                }
            );
        }
    }
    ctx.active_contest = None;
    ctx.pending_pick = None;
    for mode in ["census", "crosscheck", "survey", "probe"] {
        assert_eq!(
            parse(&format!("@all ${mode} Body $debate"), &ctx)
                .unwrap_err()
                .to_string(),
            "Use only one $ command per submission."
        );
        let mut selected = ctx.clone();
        selected.metadata.reply_to = vec![event(1)];
        assert_eq!(
            parse(&format!("@all ${mode} Body"), &selected)
                .unwrap_err()
                .to_string(),
            "Solution selection is only allowed with $debate."
        );
        assert!(parse(&format!("@all ${mode} Body @judge"), &ctx).is_ok());
    }
    for movement in [
        "collect", "pick", "fanout", "turns", "judge", "tally", "barrier", "report",
    ] {
        assert_eq!(
            parse(&format!("@all ${movement} Body"), &ctx)
                .unwrap_err()
                .to_string(),
            format!("Unknown command: ${movement}.")
        );
    }
}
#[test]
fn engine_diagnostics_are_shared_not_parser_execution() {
    let span = Span { start: 0, end: 0 };
    let values = [
        Rule::Busy.error(span.clone(), &[("kind", "grill")]),
        Rule::InsufficientAnswers.error(span.clone(), &[("n", "1"), ("m", "6")]),
        Rule::DailyMax.error(span, &[("max", &format!("{:.2}", 20.0))]),
    ];
    assert_eq!(values[1].code, ErrorCode::CONTEST_NOT_ENOUGH_ANSWERS);
    assert_eq!(
        values[1].to_string(),
        "Not enough answers to start. 1 of 6 members answered."
    );
    assert_eq!(values[2].code, ErrorCode::DAILY_MAX_REACHED);
    assert_eq!(
        values[2].to_string(),
        "Daily max of $20.00 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow."
    );
    insta::assert_json_snapshot!("engine_errors", values);
}

#[test]
fn explicit_solution_authorship_overrides_attachment_order() {
    let mut ctx = context();
    ctx.records
        .extend([reply(1, "grok", 30), reply(2, "gemini", 30)]);
    ctx.metadata.reply_to = vec![event(2), event(1)];
    let raw = "@grok @gemini $debate Compare";
    assert!(parse(raw, &ctx).is_ok());
    ctx.metadata.reply_to = vec![event(2)];
    assert!(parse(raw, &ctx).is_ok());
    ctx.metadata.reply_to = vec![event(1), event(1)];
    assert_eq!(
        parse(raw, &ctx).unwrap_err().to_string(),
        "Selected solution is not a reply to me from @gemini."
    );
}

#[test]
fn authoritative_roster_and_quote_order() {
    let mut ctx = context();
    let parsed = parse("@grok Ask", &ctx).unwrap();
    ctx.roster.retain(|n| n.as_str() != "grok");
    assert_eq!(
        validate_context(&parsed, &ctx).unwrap_err().code,
        ErrorCode::UNKNOWN_RECIPIENT
    );
    ctx.metadata.quote = vec![event(2), event(1)];
    ctx.metadata.reply_to = vec![event(3)];
    assert_eq!(
        parse("@claude @gemini $debate Compare", &ctx)
            .unwrap_err()
            .to_string(),
        format!("Quoted message {} is unavailable.", event(2))
    );
    ctx = context();
    ctx.records.push(reply(1, "grok", 30));
    if let RecordKind::MemberReply { to_me, .. } = &mut ctx.records[0].kind {
        *to_me = false;
    }
    ctx.metadata.quote = vec![event(1)];
    for raw in ["@grok $grill", "@all $census", "@all $crosscheck"] {
        assert!(parse(raw, &ctx).is_err());
    }
}
