use super::{
    lexer::tokenize,
    parser_tests::{context, event, message, pending, reply},
    *,
};
use proptest::prelude::*;
use serde_json::Value;

fn literal(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if matches!(c, '@' | '$' | '\\' | '`' | '/') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
fn mentions(nicks: &[crate::Nick]) -> String {
    nicks
        .iter()
        .map(|n| format!("@{n}"))
        .collect::<Vec<_>>()
        .join(" ")
}
fn candidates(nicks: &[crate::Nick], ctx: &ParseContext) -> String {
    if nicks.len() < 3 {
        let mut selector = "@all".to_owned();
        for n in &ctx.roster {
            if !nicks.contains(n) {
                selector.push_str(&format!(" -{n}"));
            }
        }
        selector
    } else {
        mentions(nicks)
    }
}
fn source(plan: &PlanSource) -> String {
    match plan {
        PlanSource::Pasted { text } => literal(text),
        _ => String::new(),
    }
}
fn format_valid(ast: &ParsedInput, ctx: &ParseContext) -> String {
    let mut out = match ast {
        ParsedInput::Message {
            recipients, body, ..
        } => format!("{} {}", mentions(recipients), literal(body)),
        ParsedInput::Judge { question, .. } => format!("@judge {}", literal(question)),
        ParsedInput::Assignment { updates, .. } => {
            return updates
                .iter()
                .map(|u| format!("${}={}", u.name.as_str(), u.value))
                .collect::<Vec<_>>()
                .join(" ");
        }
        ParsedInput::Slash { action, .. } => {
            return match action {
                SlashAction::Set { variable, value } => {
                    format!("/set {} {value}", variable.as_str())
                }
                SlashAction::Stop { nick } => format!(
                    "/stop{}",
                    nick.as_ref().map_or(String::new(), |n| format!(" @{n}"))
                ),
                SlashAction::Judge { debate_id } => format!(
                    "/judge{}",
                    debate_id.map_or(String::new(), |id| format!(" {id}"))
                ),
                SlashAction::Regen { message_id } => format!(
                    "/regen{}",
                    message_id.map_or(String::new(), |id| format!(" {id}"))
                ),
                SlashAction::Export { scope, id } => format!(
                    "/export {}{}",
                    serde_json::to_value(scope).unwrap().as_str().unwrap(),
                    id.as_ref().map_or(String::new(), |id| format!(" {id}"))
                ),
                SlashAction::Clear => "/clear".into(),
                SlashAction::Who => "/who".into(),
                SlashAction::Help => "/help".into(),
            };
        }
        ParsedInput::Debate {
            debaters,
            instruction,
            judge_requested,
            ..
        } => {
            let mut instruction = literal(instruction);
            if *judge_requested {
                instruction = instruction.replacen(r"\@judge", "@judge", 1);
            }
            format!("{} $debate {instruction}", mentions(debaters))
        }
        ParsedInput::Tournament {
            entrants, prompt, ..
        } => format!("{} $tournament {}", mentions(entrants), literal(prompt)),
        ParsedInput::Grill {
            leader,
            grillers,
            focus,
            ..
        } => {
            let all_others = ctx
                .roster
                .iter()
                .filter(|n| *n != leader)
                .cloned()
                .collect::<Vec<_>>();
            let selector = if *grillers == all_others {
                format!("@{leader}")
            } else {
                format!("@{leader} {}", mentions(grillers))
            };
            format!("{selector} $grill {}", literal(focus))
        }
        ParsedInput::QuestionFirst {
            mode,
            candidates: members,
            question,
            ..
        } => format!(
            "{} ${} {}",
            candidates(members, ctx),
            if *mode == QuestionFirstMode::Debate {
                "debate"
            } else {
                "grill"
            },
            literal(question)
        ),
        ParsedInput::ContestPick { nicks, .. } => return mentions(nicks),
        ParsedInput::Census {
            participants, plan, ..
        } => {
            let selector = if let PlanSource::LatestReply { nick } = plan {
                format!("@{nick}")
            } else {
                mentions(participants)
            };
            format!("{selector} $census {}", source(plan))
        }
        ParsedInput::Crosscheck {
            participants,
            plan,
            goal_constraints,
            ..
        } => {
            let selector = if let PlanSource::LatestReply { nick } = plan {
                format!("@{nick}")
            } else {
                mentions(participants)
            };
            let brief = goal_constraints.as_deref().map(literal).unwrap_or_default();
            if matches!(plan, PlanSource::Pasted { .. }) {
                format!("{selector} $crosscheck {}\n--\n{brief}", source(plan))
            } else {
                format!("{selector} $crosscheck {brief}")
            }
        }
        ParsedInput::Survey {
            participants,
            source,
            ..
        } => format!(
            "{} $survey {}",
            mentions(participants),
            if let SurveySource::Question { text } = source {
                literal(text)
            } else {
                String::new()
            }
        ),
        ParsedInput::Probe {
            participants,
            brief,
            ..
        } => format!("{} $probe {}", mentions(participants), literal(brief)),
    };
    if let Some(input) = ast.conversational() {
        for u in &input.variable_updates {
            out.push_str(&format!(" ${}={}", u.name.as_str(), u.value));
        }
    }
    out
}
fn semantic(ast: &ParsedInput) -> Value {
    let mut json = serde_json::to_value(ast).unwrap();
    json.as_object_mut().unwrap().remove("raw");
    json
}
fn generated(case: u8, text: &str, rounds: u8, grills: u8, passes: u8) -> (String, ParseContext) {
    let mut ctx = context();
    ctx.records.push(reply(1, "grok", 30));
    ctx.latest_parallel_answers = Some(ParallelAnswerSet {
        question: message(30),
        answers: vec![event(1)],
    });
    let body = format!("Topic {text} é 日本語");
    let encoded = literal(&body);
    let raw = match case {
        0 => format!("@grok @grok {encoded}"),
        1 => format!("@grok @gemini $debate {encoded}"),
        2 => format!("@grok @gemini $debate {encoded} @judge"),
        3 => format!("@all -muse $debate {encoded}"),
        4 => format!("@all $tournament {encoded}"),
        5 => format!("@grok $grill {encoded}"),
        6 => "@grok @claude @gemini $grill".into(),
        7 => format!("@all -muse $grill {encoded}"),
        8 => {
            ctx.pending_pick = Some(pending(QuestionFirstMode::Debate));
            return ("@grok @gemini @grok".into(), ctx);
        }
        9 => {
            ctx.pending_pick = Some(pending(QuestionFirstMode::Grill));
            return ("@grok @grok".into(), ctx);
        }
        10 => format!("@all $census {encoded}"),
        11 => "@grok @grok $census".into(),
        12 => {
            ctx.metadata.quote = vec![event(1)];
            "@all $census".into()
        }
        13 => format!("@all $crosscheck {encoded}\n--\nGoal {encoded}"),
        14 => format!("@grok $crosscheck {encoded}"),
        15 => {
            ctx.metadata.quote = vec![event(1)];
            format!("@all $crosscheck {encoded}")
        }
        16 => format!("@all $survey {encoded}"),
        17 => "@all $survey".into(),
        18 => format!("@all -muse $probe {encoded}"),
        19 => {
            ctx.metadata.quote = vec![event(1)];
            format!("@judge {encoded}")
        }
        20 => {
            return (
                format!("$ROUNDS={rounds} $GRILL_ROUNDS={grills} $CENSUS_PASSES={passes}"),
                ctx,
            );
        }
        21 => return (format!("/set ROUNDS {rounds}"), ctx),
        22 => return (format!("/set GRILL_ROUNDS {grills}"), ctx),
        23 => return (format!("/set CENSUS_PASSES {passes}"), ctx),
        24 => return ("/export census".into(), ctx),
        25 => return ("/export crosscheck".into(), ctx),
        26 => return ("/export survey".into(), ctx),
        27 => return ("/export probe".into(), ctx),
        28 => return ("/export room".into(), ctx),
        29 => return (format!("/export debate {}", event(9)), ctx),
        30 => return ("/export grill".into(), ctx),
        31 => return (format!("/export tournament {}", event(9)), ctx),
        32 => return (format!("/judge {}", event(9)), ctx),
        33 => return (format!("/regen {}", event(9)), ctx),
        34 => return ("/stop @grok".into(), ctx),
        35 => return ("/help".into(), ctx),
        36 => return ("/clear".into(), ctx),
        37 => return ("/who".into(), ctx),
        38 => {
            ctx.metadata.quote = vec![event(1)];
            format!("@grok $grill {encoded}")
        }
        39 => format!("@all -claude -fable -openai -muse $debate {encoded}"),
        40 => format!("@grok @gemini @claude @openai $probe {encoded}"),
        41 => {
            ctx.metadata.reply_to = vec![event(1)];
            format!("@grok @gemini $debate {encoded}")
        }
        42 => {
            ctx.metadata.quote = vec![event(2)];
            ctx.records.push(RecordSummary {
                id: event(2),
                kind: RecordKind::GrillPlan,
                eligible: true,
            });
            "@all $census".into()
        }
        _ => unreachable!(),
    };
    (
        format!("{raw} $ROUNDS={rounds} $GRILL_ROUNDS={grills} $CENSUS_PASSES={passes}"),
        ctx,
    )
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn arbitrary_input_never_panics_and_spans_are_valid(raw in proptest::collection::vec(any::<char>(),0..2048).prop_map(|v|v.into_iter().collect::<String>())) {
        let mut ctx=context();ctx.pending_pick=Some(pending(QuestionFirstMode::Debate));
        if let Err(error)=parse(&raw,&ctx) {
            prop_assert!(error.span.start<=error.span.end && error.span.end<=raw.len());
            prop_assert!(raw.is_char_boundary(error.span.start)&&raw.is_char_boundary(error.span.end));
        }
        if let Ok(tokens)=tokenize(&raw) {
            let mut end=0;
            for token in tokens {
                prop_assert_eq!(token.span.start,end);
                prop_assert!(token.span.start<=token.span.end && token.span.end<=raw.len());
                prop_assert!(raw.is_char_boundary(token.span.start)&&raw.is_char_boundary(token.span.end));
                end=token.span.end;
            }
            prop_assert_eq!(end,raw.len());
        }
    }
    #[test]
    fn structural_fragments_never_panic(parts in proptest::collection::vec(prop::sample::select(vec![
        "@all", " -grok", "@unknown", "(@grok)", "$Rounds", "$ROUNDS=0", "$ROUNDS=20", "$ROUNDS", "$OTHER",
        "$debate", "$survey", "$probe", "$vote", "`", "```", "\n```rust\n", "\n```\n", "\\@", "\\\\@", "\\$",
        "é", "日本語", "person@example.com", "/help", "\n--\n", " ", "\t", "\n", "text",
    ]),0..128)) {
        let raw=parts.concat();
        let mut ctx=context();ctx.pending_pick=Some(pending(QuestionFirstMode::Debate));
        if let Err(error)=parse(&raw,&ctx) {
            prop_assert!(error.span.start<=error.span.end && error.span.end<=raw.len());
            prop_assert!(raw.is_char_boundary(error.span.start)&&raw.is_char_boundary(error.span.end));
        }
    }
    #[test]
    fn parse_format_parse_round_trip(case in 0u8..43, text in "[a-zA-Z0-9 @$/`\\\\]{0,80}",rounds in 1u8..=20,grills in 1u8..=10,passes in 1u8..=5) {
        let (raw,ctx)=generated(case,&text,rounds,grills,passes);
        let first=parse(&raw,&ctx).unwrap();
        prop_assert_eq!(first.raw(),raw.as_str());
        let formatted=format_valid(&first,&ctx);
        let second=parse(&formatted,&ctx).unwrap();
        prop_assert_eq!(second.raw(),formatted.as_str());
        prop_assert_eq!(semantic(&first),semantic(&second));
    }
}
#[test]
fn all_round_trip_generator_branches() {
    for case in 0..43 {
        let (raw, ctx) = generated(case, "quoted @all $ROUNDS `code` \\ path", 20, 10, 5);
        let first = parse(&raw, &ctx).unwrap();
        let formatted = format_valid(&first, &ctx);
        assert_eq!(
            semantic(&first),
            semantic(&parse(&formatted, &ctx).unwrap()),
            "case {case}: {formatted}"
        );
    }
}
