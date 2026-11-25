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

#[test]
fn grill_work_requires_fresh_context_matching_prompt_and_no_judge() {
    let contest = ContestId::new();
    let step = StepId::new();
    let target = Nick::new("alpha").unwrap();
    let mut work = Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: WorkKind::GrillQuestion,
        target: Target::Member(target.clone()),
        context_id: ContextId(format!("contest:{contest}:{step}:{target}")),
        trigger_event_id: EventId::new(),
        history_through: HistoryPosition::new(1).unwrap(),
        attempt: 1,
        debate_id: None,
        round: Some(1),
        replay: None,
        case_id: None,
        prompt_ref: Some(PromptRef {
            pack_version: Text::new("fixture").unwrap(),
            template_id: Text::new("grill.question").unwrap(),
            template_hash: Text::new("hash").unwrap(),
        }),
        max_output_tokens: Some(2048),
        contest_id: Some(contest),
        step_id: Some(step),
    };
    assert!(work.validate().is_ok());
    work.context_id = ContextId(target.to_string());
    assert!(work.validate().is_err());
    work.context_id = ContextId(format!("contest:{contest}:{step}:{target}"));
    work.attempt = 3;
    assert!(work.validate().is_err());
    work.attempt = 1;
    work.prompt_ref.as_mut().unwrap().template_id = Text::new("judge.round").unwrap();
    assert!(work.validate().is_err());
}
