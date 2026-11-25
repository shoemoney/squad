use squad_protocol::{
    parser::{ParseContext, SubmissionMeta},
    *,
};
use squad_tui::{validation, widgets::completion::CompletionState};
fn context() -> ParseContext {
    ParseContext {
        roster: ["claude", "openai", "gemini"]
            .map(|n| Nick::new(n).unwrap())
            .to_vec(),
        ..Default::default()
    }
}
#[test]
fn parser_errors_are_verbatim_and_recipients_resolve() {
    let ctx = context();
    let meta = SubmissionMeta::default();
    for raw in [
        "hello",
        "@all -missing hello",
        "@claude $debate",
        "@all `$grill` hello",
    ] {
        assert_eq!(
            validation::validate(raw, &meta, &ctx).map(|v| v.parsed),
            squad_protocol::parse(raw, &meta, &ctx)
        );
    }
    let valid = validation::validate("@all -openai hello", &meta, &ctx).unwrap();
    assert_eq!(valid.recipients, ["@claude", "@gemini"]);
}
#[test]
fn assignments_use_shared_parser() {
    let v = validation::validate("$GRILL_ROUNDS=4", &Default::default(), &context()).unwrap();
    assert_eq!(
        v.updates,
        [VariableUpdate {
            name: VariableName::GrillRounds,
            value: 4
        }]
    );
}
#[test]
fn completion_respects_utf8_caret_code_and_exclusions() {
    let ctx = context();
    let meta = SubmissionMeta::default();
    for raw in ["`@cl`", "```rust\n@cl", "\\@cl"] {
        assert!(
            CompletionState::at(raw, raw.len(), &ctx, &meta)
                .items
                .is_empty()
        );
    }
    assert!(
        CompletionState::at("```\n@cl\n```\n`unfinished", 7, &ctx, &meta)
            .items
            .is_empty()
    );
    let raw = "é @clau suffix";
    let c = CompletionState::at(raw, 6, &ctx, &meta);
    assert_eq!(c.items, ["@claude"]);
    assert_eq!(&raw[c.replace], "@clau");
    let c = CompletionState::at("@all -claude -", 14, &ctx, &meta);
    assert!(!c.items.contains(&"-claude".into()));
    assert!(c.items.contains(&"-openai".into()));
    assert!(
        CompletionState::at("$GRILL", 6, &ctx, &meta)
            .items
            .contains(&"$GRILL_ROUNDS=".into())
    );
}

#[test]
fn pending_pick_completion_prioritises_eligible_without_hiding_other_members() {
    use squad_protocol::parser::{PendingPick, QuestionFirstMode};
    let mut ctx = context();
    ctx.pending_pick = Some(PendingPick {
        contest_id: ContestId::new(),
        mode: QuestionFirstMode::Debate,
        eligible: vec![Nick::new("gemini").unwrap()],
    });
    assert!(matches!(
        validation::validate("@claude ordinary chat", &Default::default(), &ctx)
            .unwrap()
            .parsed,
        ParsedInput::Message { .. }
    ));
    let completion = CompletionState::at("@", 1, &ctx, &Default::default());
    assert_eq!(completion.items[0], "@gemini");
    assert!(completion.items.contains(&"@claude".into()));
    let c = CompletionState::at("(@cl", 4, &ctx, &Default::default());
    assert_eq!(c.items, ["@claude"]);
    assert_eq!(c.replace, 1..4);
}
