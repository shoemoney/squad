use squad_protocol::*;
fn setup() -> GrillSetup {
    GrillSetup {
        contest_id: ContestId::new(),
        leader: "alpha".parse().unwrap(),
        grillers: vec!["beta".parse().unwrap()],
        source_message_id: MessageId::new(),
        initial_plan: "A complete plan".into(),
        focus: "Costs".into(),
        total_rounds: 2,
    }
}
#[test]
fn setup_bounds_and_floor() {
    let mut s = setup();
    assert_eq!(s.normal_calls(), 6);
    assert!(s.validate().is_ok());
    s.grillers.push(s.leader.clone());
    assert!(s.validate().is_err());
    s = setup();
    s.total_rounds = 11;
    assert!(s.validate().is_err());
    s = setup();
    s.initial_plan = " ".into();
    assert!(s.validate().is_err());
}
#[test]
fn question_answer_coverage_and_word_limits() {
    let s = setup();
    let mut q = GrillQuestion {
        contest_id: s.contest_id,
        round: 1,
        turn: 0,
        griller: s.grillers[0].clone(),
        leader: s.leader,
        plan_version: 1,
        angle: "cost".into(),
        questions: vec!["Who pays?".into(), "What fails?".into()],
    };
    assert!(q.validate().is_ok());
    let id = EventId::new();
    let mut a = GrillAnswer {
        contest_id: q.contest_id,
        round: 1,
        turn: 0,
        leader: q.leader.clone(),
        question_event_id: id,
        answers: vec![
            GrillAnswerItem {
                question_index: 0,
                answer: "Users pay".into(),
                plan_changes: "Add pricing".into(),
            },
            GrillAnswerItem {
                question_index: 1,
                answer: "Network fails".into(),
                plan_changes: "Add retries".into(),
            },
        ],
    };
    assert!(a.validate_against(&q, id, 8).is_ok());
    assert!(a.validate_against(&q, id, 7).is_err());
    a.answers[1].question_index = 0;
    assert!(a.validate_against(&q, id, 20).is_err());
    q.questions = vec!["PASS".into()];
    assert!(q.validate().is_err());
    q.questions = vec!["Question?".into(); 4];
    assert!(q.validate().is_err());
}
#[test]
fn unknown_fields_are_rejected_before_deserialization() {
    let mut value = serde_json::to_value(setup()).unwrap();
    value["secret"] = serde_json::json!("not authorized");
    assert!(validation::decode::<GrillSetup>(&value).is_err());
}
