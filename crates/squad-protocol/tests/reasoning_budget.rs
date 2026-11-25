use squad_protocol::{ReasoningEffort::*, WorkKind, constants::*};

#[test]
fn judge_round_reserves_the_low_budget_whatever_the_judge_binding_says() {
    for bound in [Low, Medium, High] {
        assert_eq!(
            reasoning_budget(effective_effort(&WorkKind::JudgeRound, bound)),
            REASONING_LOW_TOKENS
        );
    }
    assert_eq!(
        reasoning_budget(effective_effort(&WorkKind::Judge, High)),
        REASONING_HIGH_TOKENS
    );
    assert_eq!(
        reasoning_budget(effective_effort(&WorkKind::DebateTurn, Medium)),
        REASONING_MEDIUM_TOKENS
    );
}
