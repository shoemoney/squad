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
    if matches!(condition, "debug" | "readback" | "debug_literal") {
        ctx.pending_human = Some(PendingHuman {
            contest_id: ContestId::new(),
            kind: if condition == "readback" {
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
