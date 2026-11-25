use squad_core::orchestrator::grill::*;
use squad_protocol::*;
fn setup() -> GrillSetup {
    GrillSetup {
        contest_id: ContestId::new(),
        leader: "alpha".parse().unwrap(),
        grillers: ["beta", "gamma", "delta"]
            .map(|n| n.parse().unwrap())
            .into(),
        source_message_id: MessageId::new(),
        initial_plan: "A complete plan".into(),
        focus: String::new(),
        total_rounds: 2,
    }
}
#[test]
fn rotated_script_matches_call_count_and_has_no_judge() {
    let s = setup();
    let script = script(&s, 400, 1200).unwrap();
    let calls: Vec<_> = script.iter().filter(|s| s.composition.is_some()).collect();
    assert_eq!(calls.len(), s.normal_calls() as usize);
    assert!(
        calls
            .iter()
            .all(|s| s.move_name == ContestMove::Turns && s.attempts == 2)
    );
    assert_eq!(
        calls[0].actors,
        vec![Target::Member("beta".parse().unwrap())]
    );
    assert_eq!(
        calls[7].actors,
        vec![Target::Member("gamma".parse().unwrap())]
    );
    assert!(script.iter().all(|s| s.move_name != ContestMove::Judge));
}
#[test]
fn passes_survive_but_failures_do_not() {
    let mut state = initial_state(&setup()).unwrap();
    state = transition(&state, &GrillTransition::ParticipantsReady, 0, 0).unwrap();
    for _ in 0..3 {
        state = transition(
            &state,
            &GrillTransition::PassCommitted {
                pass_event_id: EventId::new(),
            },
            0,
            3,
        )
        .unwrap();
    }
    assert_eq!(state.state, GrillStatus::Complete);
    assert_eq!(state.terminal_reason.as_deref(), Some("survived"));
    let mut state = initial_state(&setup()).unwrap();
    state = transition(&state, &GrillTransition::ParticipantsReady, 0, 0).unwrap();
    for _ in 0..3 {
        state = transition(
            &state,
            &GrillTransition::PassCommitted {
                pass_event_id: EventId::new(),
            },
            0,
            2,
        )
        .unwrap();
    }
    assert_eq!(state.state, GrillStatus::Questioning);
    assert_eq!(state.round, 2);
    assert_eq!(state.griller_order[0].as_str(), "gamma");
}
#[test]
fn rebuild_rotation_and_leader_failure_preserve_plan() {
    let mut state = initial_state(&setup()).unwrap();
    state = transition(&state, &GrillTransition::ParticipantsReady, 0, 0).unwrap();
    assert_eq!(angle(&state), "cost");
    for _ in 0..3 {
        state = transition(
            &state,
            &GrillTransition::QuestionsCommitted {
                question_event_id: EventId::new(),
            },
            1,
            0,
        )
        .unwrap();
        state = transition(
            &state,
            &GrillTransition::AnswerCommitted {
                answer_event_id: EventId::new(),
            },
            1,
            0,
        )
        .unwrap();
    }
    assert_eq!(state.state, GrillStatus::Rebuilding);
    state = transition(
        &state,
        &GrillTransition::PlanCommitted {
            plan_event_id: EventId::new(),
        },
        1,
        0,
    )
    .unwrap();
    assert_eq!((state.round, state.plan_version), (2, 2));
    assert_eq!(angle(&state), "scale");
    state = transition(
        &state,
        &GrillTransition::QuestionsCommitted {
            question_event_id: EventId::new(),
        },
        1,
        0,
    )
    .unwrap();
    state = transition(&state, &GrillTransition::LeaderFailed, 1, 0).unwrap();
    assert_eq!(state.state, GrillStatus::Failed);
    assert_eq!(state.plan_version, 2);
    assert!(transition(&state, &GrillTransition::ParticipantsReady, 0, 0).is_err());
}
#[test]
fn final_round_rebuild_completes_with_a_reason() {
    let mut s = setup();
    s.total_rounds = 1;
    let mut state = initial_state(&s).unwrap();
    state = transition(&state, &GrillTransition::ParticipantsReady, 0, 0).unwrap();
    state = transition(
        &state,
        &GrillTransition::QuestionsCommitted {
            question_event_id: EventId::new(),
        },
        1,
        0,
    )
    .unwrap();
    state = transition(
        &state,
        &GrillTransition::AnswerCommitted {
            answer_event_id: EventId::new(),
        },
        1,
        0,
    )
    .unwrap();
    for _ in 0..2 {
        state = transition(
            &state,
            &GrillTransition::PassCommitted {
                pass_event_id: EventId::new(),
            },
            1,
            0,
        )
        .unwrap();
    }
    assert_eq!(state.state, GrillStatus::Rebuilding);
    state = transition(
        &state,
        &GrillTransition::PlanCommitted {
            plan_event_id: EventId::new(),
        },
        1,
        0,
    )
    .unwrap();
    assert_eq!(state.state, GrillStatus::Complete);
    assert_eq!(state.terminal_reason.as_deref(), Some("rounds_complete"));
}
