use super::*;
use crate::*;
use serde_json::Value;
fn context(case: &Value) -> ParserSnapshot {
    let mut ctx = ParserSnapshot {
        roster: ["a", "b", "c", "d"].map(|n| Nick::new(n).unwrap()).to_vec(),
        ..Default::default()
    };
    let condition = case["condition"].as_str().unwrap_or("");
    if condition == "small" {
        ctx.roster.truncate(1);
    }
    if let Some(kind) = condition.strip_prefix("busy:") {
        ctx.active_contest = Some(serde_json::from_value(Value::String(kind.into())).unwrap());
    }
    if matches!(condition, "latest" | "quote") {
        let id = EventId::new();
        ctx.records.push(SourceSummary {
            id,
            kind: RecordKind::MemberReply {
                author: Nick::new("a").unwrap(),
                to_me: true,
                question: MessageId::new(),
            },
            eligible: true,
        });
        if condition == "quote" {
            ctx.metadata.quote.push(id);
        }
    }
    if matches!(condition, "debug" | "readback" | "debug_literal" | "prd") {
        ctx.pending_human = Some(PendingHuman {
            contest_id: ContestId::new(),
            kind: if condition == "prd" {
                ContestKind::BassAckwardsPrd
            } else if condition == "readback" {
                ContestKind::Readback
            } else {
                ContestKind::Debugging
            },
            state_version: 2,
            step_id: StepId::new(),
            prompt: "test".into(),
            items: vec!["first".into(), "second".into()],
            choices: vec![vec!["a".into(), "b".into()], vec!["a".into()]],
            outcome_labels: vec!["O1".into(), "O2".into(), "@all".into()],
            answers: Default::default(),
        });
    }
    ctx
}
#[test]
fn new_mode_examples_and_refusals() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("examples.json")).unwrap();
    for case in cases.iter().skip(166) {
        if case["admission"].as_bool() == Some(true) {
            let raw = case["input"].as_str().unwrap();
            let error = super::errors::tune_prompt(
                raw,
                Span {
                    start: 0,
                    end: raw.len(),
                },
            )
            .unwrap_err();
            assert_eq!(error.message, case["error"].as_str().unwrap());
            continue;
        }
        let ctx = context(case);
        let raw = case["input"].as_str().unwrap();
        let run = || super::parser::parse_snapshot(raw, &ctx);
        let result = if case["condition"] == "disabled" {
            run()
        } else {
            with_new_modes(run)
        };
        if let Some(error) = case["error"].as_str() {
            assert_eq!(result.unwrap_err().message, error, "{raw}");
        } else {
            let ast = result.unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert_eq!(
                serde_json::to_value(&ast).unwrap()["kind"],
                case["kind"],
                "{raw}"
            );
            let json = serde_json::to_value(&ast).unwrap();
            let decoded: ParsedInput = serde_json::from_value(json).unwrap();
            assert_eq!(ast, decoded);
            assert_eq!(
                with_new_modes(|| super::parser::parse_snapshot(ast.raw(), &ctx)).unwrap(),
                ast
            );
        }
    }
}
#[test]
fn override_resets_after_fixture() {
    let ctx = ParserSnapshot {
        roster: ["a", "b", "c"].map(|n| Nick::new(n).unwrap()).to_vec(),
        ..Default::default()
    };
    assert!(with_new_modes(|| super::parser::parse_snapshot("@all $forge brief", &ctx)).is_ok());
    assert_eq!(
        super::parser::parse_snapshot("@all $forge brief", &ctx)
            .unwrap_err()
            .code,
        ErrorCode::COMMAND_NOT_IMPLEMENTED
    );
}

#[test]
fn tune_excludes_named_executor_and_preserves_quoted_prompt_identity() {
    let ctx = ParserSnapshot {
        roster: ["a", "b", "c", "d"].map(|n| Nick::new(n).unwrap()).to_vec(),
        ..Default::default()
    };
    let parsed = with_new_modes(|| {
        super::parser::parse_snapshot("@all $tune exact prompt\n--\ntask\n--\n@d", &ctx)
    })
    .unwrap();
    let ParsedInput::Tune {
        participants,
        executor,
        prompt,
        task,
        ..
    } = parsed
    else {
        panic!()
    };
    assert_eq!(participants.len(), 3);
    assert!(!participants.contains(&Nick::new("d").unwrap()));
    assert_eq!(executor, Some(Nick::new("d").unwrap()));
    assert_eq!(task, "task");
    assert_eq!(
        prompt,
        PlanSource::Pasted {
            text: "exact prompt".into()
        }
    );
}

#[test]
fn prd_review_enforces_both_note_caps_and_allows_continue() {
    let mut ctx = context(&serde_json::json!({"condition":"prd"}));
    let accepted = format!("/answer {}", "word ".repeat(500));
    assert!(super::parser::parse_snapshot(&accepted, &ctx).is_ok());
    let too_long = format!("/answer {}", "word ".repeat(501));
    assert!(super::parser::parse_snapshot(&too_long, &ctx).is_err());
    ctx.pending_human.as_mut().unwrap().answers = (1..=10).map(|i| (i, "note".into())).collect();
    assert!(super::parser::parse_snapshot("/answer another", &ctx).is_err());
    assert!(super::parser::parse_snapshot("/continue", &ctx).is_ok());
}
#[test]
fn enabled_export_usage_lists_new_scopes() {
    let ctx = ParserSnapshot::default();
    let error =
        with_new_modes(|| super::parser::parse_snapshot("/export unknown", &ctx)).unwrap_err();
    assert!(error.message.contains("tune|bass_ackwards_prd"));
}
