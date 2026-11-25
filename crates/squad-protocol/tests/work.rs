mod common;
use common::*;
use serde_json::json;
use squad_protocol::*;
fn ordinary() -> Work {
    validation::decode(&json!({"workId":ID,"messageId":ID,"inReplyToMessageId":ID,"kind":"reply","target":"a","contextId":"a","triggerEventId":ID,"historyThrough":"10","attempt":1})).unwrap()
}
#[test]
fn member_case_and_step_contexts_cannot_be_substituted() {
    let mut work = ordinary();
    assert!(work.validate().is_ok());
    let owner = ExecutorKey::Member(Nick::new("a").unwrap());
    assert!(owner.owns(&work));
    work.context_id = ContextId("b".into());
    assert!(work.validate().is_err());
    work.contest_id = Some(ID.parse().unwrap());
    work.step_id = Some(ID.parse().unwrap());
    work.context_id = ContextId(format!("contest:{ID}:{ID}:a"));
    let assignment = ContestAssignment {
        target: work.target.clone(),
        context_id: work.context_id.clone(),
        payload_refs: vec![],
        work_id: Some(work.work_id),
    };
    assert!(work.validate_assignment(&owner, &assignment).is_ok());
    let foreign = ExecutorKey::Member(Nick::new("b").unwrap());
    assert!(work.validate_assignment(&foreign, &assignment).is_err());
    work.context_id = ContextId(format!("contest:{ID}:{ID}:b"));
    assert!(work.validate().is_err());
    work = ordinary();
    work.attempt = 0;
    assert!(work.validate().is_err());
}
#[test]
fn round_judge_has_separate_visible_and_reasoning_budgets() {
    let mut work:Work=validation::decode(&json!({"workId":ID,"messageId":ID,"inReplyToMessageId":ID,"kind":"judge.round","target":"judge","contextId":format!("judge:{ID}"),"triggerEventId":ID,"historyThrough":"1","attempt":1,"debateId":ID,"contestId":ID,"caseId":ID,"round":1,"maxOutputTokens":512,"promptRef":{"templateId":"judge.round.md","packVersion":"v1","templateHash":"hash"}})).unwrap();
    assert!(work.validate_round_bound(1).is_ok());
    assert!(ExecutorKey::Case(ID.parse().unwrap()).owns(&work));
    assert_eq!(
        constants::ROUND_JUDGE_VISIBLE_TOKENS + constants::REASONING_LOW_TOKENS,
        1536
    );
    work.max_output_tokens = Some(1536);
    assert!(work.validate().is_err());
    work.max_output_tokens = Some(512);
    work.round = Some(0);
    assert!(work.validate().is_err());
    work.round = Some(2);
    assert!(work.validate_round_bound(1).is_err());
    work.round = Some(1);
    work.case_id = None;
    assert!(work.validate().is_err());
}
#[test]
fn ceilings_rounds_and_optional_replay_are_validated() {
    let mut work = ordinary();
    work.max_output_tokens = Some(2048);
    assert!(work.validate().is_ok());
    work.max_output_tokens = Some(2049);
    assert!(work.validate().is_err());
    work.max_output_tokens = None;
    work.replay = Some(2);
    assert!(work.validate().is_err());
}
#[test]
fn capability_traits_remain_object_safe() {
    fn accepts(
        _: Option<&dyn Bus>,
        _: Option<&dyn HistoryReader>,
        _: Option<&dyn WorkReader>,
        _: Option<&dyn ExecutorSubmit>,
        _: Option<&dyn PromptRenderer>,
    ) {
    }
    accepts(None, None, None, None, None);
}

#[test]
fn work_identity_and_sql_relationship_constraints_are_checked_before_commit() {
    let mut work = ordinary();
    work.in_reply_to_message_id = None;
    assert!(work.validate().is_err());
    work = ordinary();
    work.round = Some(0);
    assert!(work.validate().is_err());
    work.round = None;
    work.kind = WorkKind::DebateTurn;
    work.debate_id = Some(ContestId::new());
    work.round = Some(1);
    assert!(work.validate().is_err());
    work.contest_id = work.debate_id;
    assert!(work.validate().is_ok());
    work.contest_id = Some(ContestId::new());
    assert!(work.validate().is_err());
    let mut work: Work = validation::decode(&serde_json::json!({"workId":ID,"messageId":ID,"kind":"judge.round","target":"judge","contextId":format!("judge:{ID}"),"triggerEventId":ID,"historyThrough":"1","attempt":3,"debateId":ID,"contestId":ID,"caseId":ID,"round":1,"maxOutputTokens":512,"promptRef":{"templateId":"judge.round.md","packVersion":"v1","templateHash":"hash"}})).unwrap();
    assert!(work.validate().is_err());
    work.attempt = 2;
    assert!(work.validate().is_ok());
    assert_eq!(
        MessageId::from_event(ID.parse().unwrap()),
        ID.parse().unwrap()
    );
}
