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

#[test]
fn new_mode_judge_constants_are_medium_and_4096_and_existing_judge_constants_unchanged() {
    assert_eq!(NEW_MODE_JUDGE_REASONING_EFFORT, Medium);
    assert_eq!(NEW_MODE_JUDGE_VISIBLE_TOKENS, 4_096);
    assert_eq!(
        reasoning_budget(NEW_MODE_JUDGE_REASONING_EFFORT),
        REASONING_MEDIUM_TOKENS
    );
    assert_eq!(FINAL_JUDGE_VISIBLE_TOKENS, 4_096);
    assert_eq!(ROUND_JUDGE_VISIBLE_TOKENS, 512);
    assert_eq!(ROUND_JUDGE_REASONING_EFFORT, Low);
    assert_eq!(DEFAULT_FINAL_JUDGE_REASONING_EFFORT, High);
    assert_eq!(JUDGE_WORDS, 1_000);
    assert_eq!(FINAL_JUDGE_ATTEMPTS, 3);
    assert_eq!(ROUND_JUDGE_ATTEMPTS, 2);
}
