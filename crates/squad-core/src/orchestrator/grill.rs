//! Deterministic grill script. IDs and authoritative commits belong to the router.
use super::types::StepSpec;
use squad_protocol::*;

pub const ANGLES: [&str; 8] = [
    "cost",
    "failure modes",
    "security",
    "scale",
    "edge cases",
    "hidden assumptions",
    "user experience",
    "operations",
];

pub fn initial_state(setup: &GrillSetup) -> Result<GrillState, ErrorCode> {
    setup.validate().map_err(|_| ErrorCode::INVALID_COMMAND)?;
    Ok(GrillState {
        contest_id: setup.contest_id,
        state: GrillStatus::WaitingForParticipants,
        state_version: 1,
        round: 1,
        total_rounds: setup.total_rounds,
        leader: setup.leader.clone(),
        griller_order: setup.grillers.clone(),
        turn: 0,
        plan_version: 1,
        questions_asked: 0,
        passed: 0,
        terminal_reason: None,
    })
}

pub fn angle(state: &GrillState) -> &'static str {
    ANGLES[((state.round - 1) as usize * state.griller_order.len() + state.turn as usize)
        % ANGLES.len()]
}

/// Expand the maximum no-pass path for preflight. Runtime skips answer/rebuild
/// moves on passes; the attempt reservation remains conservative.
pub fn script(
    setup: &GrillSetup,
    answer_words: u32,
    plan_words: u32,
) -> Result<Vec<StepSpec>, ErrorCode> {
    setup.validate().map_err(|_| ErrorCode::INVALID_COMMAND)?;
    let mut steps = Vec::new();
    for round in 1..=setup.total_rounds {
        let mut order = setup.grillers.clone();
        let len = order.len();
        order.rotate_left((round as usize - 1) % len);
        for griller in order {
            steps.push(model_step(
                round,
                griller,
                "grill.question",
                constants::GRILL_QUESTION_WORDS,
            ));
            steps.push(barrier(round));
            steps.push(model_step(
                round,
                setup.leader.clone(),
                "grill.answer",
                answer_words,
            ));
            steps.push(barrier(round));
        }
        steps.push(model_step(
            round,
            setup.leader.clone(),
            "grill.rebuild",
            plan_words,
        ));
        steps.push(barrier(round));
    }
    steps.push(StepSpec {
        move_name: ContestMove::Report,
        round: None,
        actors: vec![],
        composition: None,
        word_limit: None,
        attempts: 0,
        deadline_ms: 0,
    });
    Ok(steps)
}
fn model_step(round: u32, nick: MemberNick, composition: &str, words: u32) -> StepSpec {
    StepSpec {
        move_name: ContestMove::Turns,
        round: Some(round),
        actors: vec![Target::Member(nick)],
        composition: Some(composition.into()),
        word_limit: Some(words),
        attempts: 2,
        deadline_ms: constants::DEBATE_TURN_DEADLINE_MS,
    }
}
fn barrier(round: u32) -> StepSpec {
    StepSpec {
        move_name: ContestMove::Barrier,
        round: Some(round),
        actors: vec![],
        composition: None,
        word_limit: None,
        attempts: 0,
        deadline_ms: 0,
    }
}

/// Advance after an authoritative result was committed. Round evidence comes
/// from retained records, so a failed griller is never counted as a PASS.
pub fn transition(
    state: &GrillState,
    transition: &GrillTransition,
    round_questions: u32,
    voluntary_passes: u32,
) -> Result<GrillState, ErrorCode> {
    state
        .validate()
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    if state.state.is_terminal() {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    let mut next = state.clone();
    match transition {
        GrillTransition::ParticipantsReady
            if state.state == GrillStatus::WaitingForParticipants =>
        {
            next.state = GrillStatus::Questioning
        }
        GrillTransition::QuestionsCommitted { .. }
            if state.state == GrillStatus::Questioning && round_questions > 0 =>
        {
            next.state = GrillStatus::Answering;
        }
        GrillTransition::AnswerCommitted { .. } if state.state == GrillStatus::Answering => {
            advance_turn(&mut next, round_questions, voluntary_passes)
        }
        GrillTransition::PassCommitted { .. } if state.state == GrillStatus::Questioning => {
            advance_turn(&mut next, round_questions, voluntary_passes)
        }
        GrillTransition::PlanCommitted { .. } if state.state == GrillStatus::Rebuilding => {
            next.plan_version += 1;
            if state.round == state.total_rounds {
                next.state = GrillStatus::Complete;
            } else {
                next.round += 1;
                next.turn = 0;
                next.griller_order.rotate_left(1);
                next.state = GrillStatus::Questioning;
            }
        }
        GrillTransition::Stop => {
            next.state = GrillStatus::Stopped;
            next.terminal_reason = Some("stopped".into());
        }
        GrillTransition::LeaderFailed
            if matches!(
                state.state,
                GrillStatus::Answering | GrillStatus::Rebuilding
            ) =>
        {
            next.state = GrillStatus::Failed;
            next.terminal_reason = Some("leader_failed".into());
        }
        _ => return Err(ErrorCode::INVALID_STATE_TRANSITION),
    }
    next.state_version += 1;
    Ok(next)
}
fn advance_turn(state: &mut GrillState, questions: u32, passes: u32) {
    if state.turn as usize + 1 < state.griller_order.len() {
        state.turn += 1;
        state.state = GrillStatus::Questioning;
    } else if questions > 0 {
        state.state = GrillStatus::Rebuilding;
    } else if passes as usize == state.griller_order.len() {
        state.state = GrillStatus::Complete;
        state.terminal_reason = Some("survived".into());
    } else if state.round < state.total_rounds {
        state.round += 1;
        state.turn = 0;
        state.griller_order.rotate_left(1);
        state.state = GrillStatus::Questioning;
    } else {
        state.state = GrillStatus::Complete;
        state.terminal_reason = Some("grillers_exhausted".into());
    }
}

/// Build the appendix from committed evidence; missing answers remain missing.
pub fn appendix(events: &[CommittedEnvelope]) -> Result<Vec<GrillAppendixEntry>, ErrorCode> {
    let mut result = Vec::new();
    for event in events {
        let EnvelopePayload::GrillQuestion(q) = &event.content else {
            continue;
        };
        q.validate().map_err(|_| ErrorCode::INVALID_REFERENCE)?;
        let answer = events.iter().find_map(|e| match &e.content {
            EnvelopePayload::GrillAnswer(a) if a.question_event_id == event.base.id => Some(a),
            _ => None,
        });
        if let Some(a) = answer {
            a.validate_against(q, event.base.id, u32::MAX)
                .map_err(|_| ErrorCode::INVALID_REFERENCE)?;
        }
        for (index, question) in q.questions.iter().enumerate() {
            let item = answer.and_then(|a| {
                a.answers
                    .iter()
                    .find(|a| a.question_index as usize == index)
            });
            result.push(GrillAppendixEntry {
                question_event_id: event.base.id,
                question_index: index as u8,
                griller: q.griller.clone(),
                question: question.clone(),
                answer: item.map(|a| a.answer.clone()),
                plan_changes: item.map(|a| a.plan_changes.clone()),
            });
        }
    }
    Ok(result)
}
