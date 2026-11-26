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

#[test]
fn provider_question_contract_requires_exact_pass_and_new_bounded_questions() {
    let setup = setup();
    let state = GrillState {
        contest_id: setup.contest_id,
        state: GrillStatus::Questioning,
        state_version: 2,
        round: 1,
        total_rounds: 2,
        leader: setup.leader,
        griller_order: setup.grillers,
        turn: 0,
        plan_version: 1,
        questions_asked: 0,
        passed: 0,
        terminal_reason: None,
    };
    let input = GrillWorkInput {
        state,
        question: None,
        earlier_questions: vec!["Who pays?".into()],
        answer_words: 20,
        plan_words: 30,
    };
    assert!(matches!(
        input.parse_output(&WorkKind::GrillQuestion, "PASS"),
        Ok(EnvelopePayload::GrillPass(_))
    ));
    for raw in [
        " PASS",
        "PASS\n",
        "pass",
        r#"{"questions":[]}"#,
        r#"{"questions":["who pays?"]}"#,
        r#"{"questions":["New?"],"leader":"intruder"}"#,
    ] {
        assert!(
            input.parse_output(&WorkKind::GrillQuestion, raw).is_err(),
            "{raw}"
        );
    }
    assert!(
        matches!(input.parse_output(&WorkKind::GrillQuestion,r#"{"questions":["What fails?","How does it recover?"]}"#),Ok(EnvelopePayload::GrillQuestion(q)) if q.angle=="cost"&&q.questions.len()==2)
    );
}
#[test]
fn provider_answers_require_complete_indices_changes_and_limits() {
    let setup = setup();
    let q = GrillQuestion {
        contest_id: setup.contest_id,
        round: 1,
        turn: 0,
        griller: setup.grillers[0].clone(),
        leader: setup.leader.clone(),
        plan_version: 1,
        angle: "cost".into(),
        questions: vec!["Who pays?".into(), "How much?".into()],
    };
    let state = GrillState {
        contest_id: setup.contest_id,
        state: GrillStatus::Answering,
        state_version: 3,
        round: 1,
        total_rounds: 2,
        leader: setup.leader,
        griller_order: setup.grillers,
        turn: 0,
        plan_version: 1,
        questions_asked: 1,
        passed: 0,
        terminal_reason: None,
    };
    let input = GrillWorkInput {
        state,
        question: Some((EventId::new(), q)),
        earlier_questions: vec![],
        answer_words: 10,
        plan_words: 30,
    };
    let valid = serde_json::json!({"answers":[{"questionIndex":0,"answer":"Users","planChanges":"Price transparently"},{"questionIndex":1,"answer":"One dollar","planChanges":"Publish pricing"}]});
    assert!(
        input
            .parse_output(&WorkKind::GrillAnswer, &valid.to_string())
            .is_ok()
    );
    for invalid in [
        serde_json::json!({"answers":[]}),
        serde_json::json!({"answers":[valid["answers"][0].clone(),valid["answers"][0].clone()]}),
        serde_json::json!({"answers":[valid["answers"][0].clone()]}),
        serde_json::json!({"answers":[valid["answers"][0].clone(),{"questionIndex":1,"answer":"one","planChanges":""}]}),
    ] {
        assert!(
            input
                .parse_output(&WorkKind::GrillAnswer, &invalid.to_string())
                .is_err()
        );
    }
}

/// Live capture (Opus 5.5 as grill leader, three griller questions, 2026-11-25): valid
/// JSON, finish `stop`, but 506 words across `answer` and `planChanges` against the 400-word
/// `turnWords` budget. The validator is right; the old prompt let the model read the limit as
/// per-question prose. This pins the PRD limit so a loosened validator cannot hide the cause.
#[test]
fn live_leader_answer_over_the_shared_turn_budget_is_rejected_at_400_and_fits_at_600() {
    let setup = setup();
    let raw = include_str!("fixtures/live_grill_answer_opus_over_budget.json");
    let words: usize = serde_json::from_str::<serde_json::Value>(raw).unwrap()["answers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            ["answer", "planChanges"]
                .iter()
                .map(|k| a[k].as_str().unwrap().split_whitespace().count())
                .sum::<usize>()
        })
        .sum();
    assert_eq!(words, 506);
    let q = GrillQuestion {
        contest_id: setup.contest_id,
        round: 1,
        turn: 0,
        griller: setup.grillers[0].clone(),
        leader: setup.leader.clone(),
        plan_version: 1,
        angle: "cost".into(),
        questions: vec!["Q0?".into(), "Q1?".into(), "Q2?".into()],
    };
    let state = GrillState {
        contest_id: setup.contest_id,
        state: GrillStatus::Answering,
        state_version: 3,
        round: 1,
        total_rounds: 1,
        leader: setup.leader,
        griller_order: setup.grillers,
        turn: 0,
        plan_version: 1,
        questions_asked: 1,
        passed: 0,
        terminal_reason: None,
    };
    let input = |answer_words| GrillWorkInput {
        state: state.clone(),
        question: Some((EventId::new(), q.clone())),
        earlier_questions: vec![],
        answer_words,
        plan_words: 1200,
    };
    assert!(
        input(400)
            .parse_output(&WorkKind::GrillAnswer, raw)
            .is_err()
    );
    assert!(input(600).parse_output(&WorkKind::GrillAnswer, raw).is_ok());
    let fenced = format!("```json\n{raw}\n```");
    assert!(
        input(600)
            .parse_output(&WorkKind::GrillAnswer, &fenced)
            .is_ok()
    );
}

/// Live (Opus 5.5 grill leader, no response_format, 2026-11-25): the same JSON came back wrapped
/// in a single ```json fence and the strict parse rejected it. The PRD allows removing one
/// surrounding fence, and nothing looser.
#[test]
fn one_surrounding_code_fence_is_removed_and_nothing_looser() {
    use squad_protocol::validation::strip_code_fence;
    let json = r#"{"answers":[]}"#;
    assert_eq!(strip_code_fence(&format!("```json\n{json}\n```")), json);
    assert_eq!(strip_code_fence(&format!("  ```\n{json}\n```\n")), json);
    for unchanged in [
        json.to_string(),
        format!("Here you go:\n```json\n{json}\n```"),
        format!("```json\n{json}\n```\nThat is all."),
        format!("```json\n{json}\n```\n```json\n{json}\n```"),
        format!("```json {json}```"),
    ] {
        assert_eq!(strip_code_fence(&unchanged), unchanged);
    }
}
