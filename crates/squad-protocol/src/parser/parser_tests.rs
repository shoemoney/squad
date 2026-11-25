use super::*;
use crate::{ContestKind, EventId, MessageId, Nick};
use serde_json::{Value, json};

pub(super) fn event(n: usize) -> EventId {
    format!("01900000-0000-7000-8000-{n:012x}").parse().unwrap()
}
pub(super) fn message(n: usize) -> MessageId {
    event(n).to_string().parse().unwrap()
}
pub(super) fn nick(n: &str) -> Nick {
    Nick::new(n).unwrap()
}
pub(super) fn context() -> ParseContext {
    ParseContext {
        roster: ["claude", "fable", "openai", "gemini", "grok", "muse"]
            .map(nick)
            .to_vec(),
        ..Default::default()
    }
}
pub(super) fn reply(n: usize, author: &str, question: usize) -> RecordSummary {
    RecordSummary {
        id: event(n),
        kind: RecordKind::MemberReply {
            author: nick(author),
            to_me: true,
            question: message(question),
        },
        eligible: true,
    }
}
pub(super) fn pending(mode: QuestionFirstMode) -> PendingPick {
    PendingPick {
        contest_id: event(20).to_string().parse().unwrap(),
        mode,
        eligible: ["claude", "gemini", "grok"].map(nick).to_vec(),
    }
}
fn example_context(i: usize) -> ParseContext {
    let mut ctx = context();
    if i == 6 {
        ctx.roster = vec![nick("grok")];
    }
    if i == 31 {
        ctx.variables.rounds = 3;
    }
    let quote_ids: &[usize] = match i {
        38 | 41 | 43 => {
            if i == 41 {
                &[1, 2]
            } else {
                &[1]
            }
        }
        42 => &[3],
        88 | 113 | 118 | 121 | 124 | 133 | 134 | 138 | 149 | 152 | 155 | 156 => &[6],
        89 | 122 | 139 => &[6, 7],
        123 => &[8],
        _ => &[],
    };
    ctx.metadata.quote = quote_ids.iter().map(|n| event(*n)).collect();
    for id in quote_ids {
        ctx.records.push(match id {
            3 => RecordSummary {
                id: event(*id),
                kind: RecordKind::Verdict,
                eligible: true,
            },
            8 => RecordSummary {
                id: event(*id),
                kind: RecordKind::GrillPlan,
                eligible: true,
            },
            _ => reply(*id, if *id <= 2 { "gemini" } else { "grok" }, 30),
        });
    }
    if [44, 45, 48, 49, 50, 51].contains(&i) {
        ctx.records.extend([
            reply(4, "grok", 30),
            reply(5, "gemini", if i == 51 { 31 } else { 30 }),
        ]);
    }
    if [47, 85, 86, 90, 92, 93, 115, 116, 135].contains(&i) {
        ctx.records.push(reply(40, "grok", 30));
    }
    if [50, 51].contains(&i) {
        ctx.metadata.reply_to = vec![event(4), event(5)];
    }
    if [39, 100].contains(&i) {
        ctx.metadata.reply_to = vec![event(1)];
    }
    if i == 152 {
        ctx.metadata.reply_to = vec![event(6)];
    }
    if [101, 102, 103, 104, 107, 109].contains(&i) {
        ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
    }
    if [105, 106].contains(&i) {
        ctx.pending_pick = Some(pending(QuestionFirstMode::Grill));
    }
    if [141, 142].contains(&i) {
        ctx.records
            .extend([reply(41, "grok", 30), reply(42, "gemini", 30)]);
        ctx.latest_parallel_answers = Some(ParallelAnswerSet {
            question: message(30),
            answers: vec![event(41), event(42)],
        });
    }
    ctx.active_contest = match i {
        57 => Some(ContestKind::Debate),
        110 => Some(ContestKind::Grill),
        153 => Some(ContestKind::Census),
        154 => Some(ContestKind::Crosscheck),
        155 => Some(ContestKind::Survey),
        156 => Some(ContestKind::Probe),
        _ => None,
    };
    ctx
}
#[test]
fn every_spec_example() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("examples.json")).unwrap();
    assert_eq!(cases.len(), 166);
    let mut snapshots = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let ctx = example_context(i);
        let before = format!("{ctx:?}");
        let input = case["input"].as_str().unwrap();
        let result = parse(input, &ctx);
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.as_ref().unwrap_err().to_string(),
                error.as_str().unwrap(),
                "example {}: {input}",
                i + 1
            );
            let actual_error = result.as_ref().unwrap_err();
            assert_eq!(
                serde_json::to_value(actual_error.code).unwrap(),
                case["code"],
                "example {}",
                i + 1
            );
            assert_eq!(
                serde_json::to_value(&actual_error.span).unwrap(),
                case["span"],
                "example {}",
                i + 1
            );
        } else {
            let actual = serde_json::to_value(
                result
                    .as_ref()
                    .unwrap_or_else(|e| panic!("example {}: {input}: {e}", i + 1)),
            )
            .unwrap();
            assert_eq!(actual, case["ast"], "example {}: {input}", i + 1);
            let decoded: ParsedInput = serde_json::from_value(actual).unwrap();
            assert_eq!(result.as_ref().unwrap(), &decoded);
        }
        assert_eq!(
            before,
            format!("{ctx:?}"),
            "parsing cannot change room snapshots"
        );
        snapshots.push(json!({"row":i+1,"result":result}));
    }
    insta::assert_json_snapshot!("all_examples", snapshots);
}
#[test]
fn limits_and_assignment_values() {
    let ctx = context();
    for (input, expected) in [
        ("", "Enter a message or command."),
        (" \n\t", "Enter a message or command."),
        ("$ROUNDS=+1", "ROUNDS must be an integer from 1 to 20."),
        ("$ROUNDS=-1", "ROUNDS must be an integer from 1 to 20."),
        ("$ROUNDS=1e1", "ROUNDS must be an integer from 1 to 20."),
        (
            "$ROUNDS=999999999999999999999999",
            "ROUNDS must be an integer from 1 to 20.",
        ),
    ] {
        assert_eq!(parse(input, &ctx).unwrap_err().to_string(), expected);
    }
    let at_limit = format!("@grok {}", "é".repeat((32768 - 6) / 2));
    assert_eq!(at_limit.len(), 32768);
    assert!(parse(&at_limit, &ctx).is_ok());
    let too_big = format!("{at_limit}é");
    assert_eq!(
        parse(&too_big, &ctx).unwrap_err().to_string(),
        "Keep the submission within 32,768 UTF-8 bytes."
    );
    for (name, max) in [("ROUNDS", 20), ("GRILL_ROUNDS", 10), ("CENSUS_PASSES", 5)] {
        for v in [1, max] {
            assert!(parse(&format!("${name}={v}"), &ctx).is_ok());
        }
        for v in ["0", "-1", "+1", "1.5", "1e0", "", "９", "256"] {
            assert!(parse(&format!("${name}={v}"), &ctx).is_err());
        }
    }
    let ast = parse(
        "$ROUNDS=2 $GRILL_ROUNDS=4 $CENSUS_PASSES=3 $ROUNDS=20",
        &ctx,
    )
    .unwrap();
    let ParsedInput::Assignment { updates, .. } = ast else {
        panic!()
    };
    assert_eq!(
        updates.iter().map(|u| u.name.as_str()).collect::<Vec<_>>(),
        ["GRILL_ROUNDS", "CENSUS_PASSES", "ROUNDS"]
    );
}
#[test]
fn slash_argument_table() {
    let ctx = context();
    let cases = [
        (
            "/set",
            format!(
                "Usage: {}",
                "/set ROUNDS <1-20> | /set GRILL_ROUNDS <1-10> | /set CENSUS_PASSES <1-5>"
            ),
        ),
        ("/set OTHER 2", "Unknown variable: OTHER.".into()),
        (
            "/set ROUNDS 21",
            "ROUNDS must be an integer from 1 to 20.".into(),
        ),
        ("/stop @unknown", "Unknown recipient: @unknown.".into()),
        ("/stop @grok extra", "Usage: /stop [@nick]".into()),
        ("/judge x y", "Usage: /judge [debateId]".into()),
        ("/regen x y", "Usage: /regen [messageId]".into()),
        ("/judge invalid", "Invalid debateId: invalid.".into()),
        ("/regen invalid", "Invalid messageId: invalid.".into()),
        ("/export debate invalid", "Invalid id: invalid.".into()),
        ("/clear extra", "Usage: /clear".into()),
        ("/who extra", "Usage: /who".into()),
        ("/help extra", "Usage: /help".into()),
        ("/Help", "Unknown slash command: /Help.".into()),
    ];
    for (raw, expected) in cases {
        assert_eq!(parse(raw, &ctx).unwrap_err().to_string(), expected, "{raw}");
    }
    for scope in [
        "room",
        "debate",
        "grill",
        "tournament",
        "census",
        "crosscheck",
        "survey",
        "probe",
    ] {
        assert!(parse(&format!("/export {scope}"), &ctx).is_ok());
    }
    assert!(
        parse("/help ` @unknown $OTHER", &ctx)
            .unwrap_err()
            .to_string()
            .starts_with("Usage:")
    );
}
#[test]
fn precedence_table() {
    let mut ctx = context();
    for (raw, expected) in [
        (
            "@unknown $dance `unclosed",
            "Close the inline code with a backtick.",
        ),
        ("@unknown $dance $ROUNDS=0", "Unknown command: $dance."),
        (
            "@unknown $ROUNDS=0",
            "ROUNDS must be an integer from 1 to 20.",
        ),
        (
            "$ROUNDS=0 $GRILL_ROUNDS=0",
            "ROUNDS must be an integer from 1 to 20.",
        ),
        (
            "$GRILL_ROUNDS=0 $ROUNDS=0",
            "GRILL_ROUNDS must be an integer from 1 to 10.",
        ),
        (
            "@all @judge",
            "Quote at least one message for @judge to rule on.",
        ),
        (
            "@all @grok @judge",
            "Quote at least one message for @judge to rule on.",
        ),
        (
            "@grok @gemini $census",
            "$census needs at least three participants.",
        ),
        ("@unknown @grok $census", "Unknown recipient: @unknown."),
        ("@all -unknown $probe", "Unknown recipient: @unknown."),
    ] {
        assert_eq!(parse(raw, &ctx).unwrap_err().to_string(), expected, "{raw}");
    }
    ctx.metadata.quote = vec![event(1); 13];
    ctx.metadata.reply_to = vec![event(2); 3];
    assert_eq!(
        parse("@unknown", &ctx).unwrap_err().to_string(),
        "Attach no more than 12 quoted messages."
    );
    ctx.metadata.quote.truncate(1);
    assert_eq!(
        parse("$ROUNDS=2", &ctx).unwrap_err().to_string(),
        "Quotes require a conversational message."
    );
}
#[test]
fn decision_table_boundaries() {
    let mut ctx = context();
    ctx.roster = vec![nick("grok")];
    for (raw, expected) in [
        (
            "@all $debate Question",
            "$debate needs at least two candidates.",
        ),
        (
            "@all $grill Question",
            "$grill needs at least two candidates.",
        ),
        (
            "$grill Question",
            "$grill needs a leader or at least two candidates.",
        ),
        ("@grok $grill", "$grill needs at least one griller."),
        (
            "@grok @grok $debate",
            "$debate requires exactly two distinct debaters.",
        ),
        (
            "$survey Question",
            "Add at least one recipient: @nick or @all.",
        ),
    ] {
        assert_eq!(parse(raw, &ctx).unwrap_err().to_string(), expected, "{raw}");
    }
    let ctx = context();
    assert!(
        matches!(parse("@grok @gemini $debate",&ctx),Ok(ParsedInput::Debate{instruction,..}) if instruction.is_empty())
    );
    assert_eq!(
        parse("@all $debate @judge", &ctx).unwrap_err().to_string(),
        "Add a question after $debate."
    );
}
#[test]
fn crosscheck_separator_cases() {
    let ctx = context();
    for (raw, plan, brief) in [
        (
            "@all $crosscheck\nPlan\n--\nGoal\nConstraints",
            "Plan",
            Some("Goal\nConstraints"),
        ),
        (
            "@all $crosscheck\nPlan\n \t-- \t\nGoal\n--\nMore",
            "Plan",
            Some("Goal\n--\nMore"),
        ),
        ("@all $crosscheck\nPlan\n--\n  ", "Plan", None),
        (
            "@all $crosscheck\nPlan\n```txt\n--\n```\n--\nGoal",
            "Plan\n```txt\n--\n```",
            Some("Goal"),
        ),
        (
            "@all $crosscheck\nPlan $ROUNDS\n--\nGoal $ROUNDS=5",
            "Plan 5",
            Some("Goal"),
        ),
    ] {
        let ast = parse(raw, &ctx).unwrap();
        assert!(
            matches!(ast,ParsedInput::Crosscheck{plan:PlanSource::Pasted{text},goal_constraints,..} if text==plan && goal_constraints.as_deref()==brief),
            "{raw}"
        );
    }
    assert!(parse("@all $crosscheck\n--\nGoal", &ctx).is_err());
    let mut ctx = ctx;
    ctx.records.push(reply(1, "grok", 30));
    ctx.metadata.quote.push(event(1));
    for raw in [
        "@all $crosscheck Plan\n--\nGoal",
        "@grok $crosscheck Plan\n--\nGoal",
    ] {
        assert!(
            matches!(parse(raw,&ctx).unwrap(),ParsedInput::Crosscheck{goal_constraints:Some(text),..} if text=="Plan\n--\nGoal")
        );
    }
}

#[test]
fn shared_consumer_api_and_pick_requests() {
    let ctx = context();
    let raw = "@all -muse Explain";
    let cli: ParseResult = parse(raw, &ctx);
    let tui: ParseResult = parse(raw, &ctx);
    let router: ParseResult = parse(raw, &ctx);
    assert_eq!(cli, tui);
    assert_eq!(tui, router);
    let mut ctx = ctx;
    ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
    let ast = parse("@grok @gemini", &ctx).unwrap();
    assert_eq!(
        ast.contest_pick_request(event(31)),
        Some(crate::Request::ContestPick {
            request_id: event(31),
            contest_id: event(20).to_string().parse().unwrap(),
            nicks: vec![nick("grok"), nick("gemini")]
        })
    );
    assert!(
        parse("@grok Message", &ctx)
            .unwrap()
            .contest_pick_request(event(32))
            .is_none()
    );
    assert_eq!(
        parse("@all $probe Brief", &ctx).unwrap_err().to_string(),
        "A debate is already running. Stop it before starting another."
    );
}
#[test]
fn slash_spans_point_at_the_argument() {
    let ctx = context();
    for (raw, start, end) in [
        ("/judge judge", 7, 12),
        (" /regen regen", 8, 13),
        ("/set ROUNDS ROUNDS", 12, 18),
        ("/export debate room", 15, 19),
    ] {
        assert_eq!(
            parse(raw, &ctx).unwrap_err().span,
            Span { start, end },
            "{raw}"
        );
    }
}
#[test]
fn same_category_errors_are_leftmost() {
    let ctx = context();
    for (raw, expected) in [
        ("@unknown @judge Question", "Unknown recipient: @unknown."),
        (
            "@judge @unknown Question",
            "Quote at least one message for @judge to rule on.",
        ),
        ("@unknown @judge $grill", "Unknown recipient: @unknown."),
        (
            "@judge @unknown $grill",
            "@judge cannot be used with $grill.",
        ),
        ("@me @judge Question", "@me cannot be a recipient."),
    ] {
        assert_eq!(parse(raw, &ctx).unwrap_err().to_string(), expected, "{raw}");
    }
}
