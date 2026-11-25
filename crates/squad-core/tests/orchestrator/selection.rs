use super::support::*;
use squad_core::orchestrator::{
    recovery, selection,
    sources::{self, SourceReply},
    stop,
};
use squad_protocol::*;

fn source(name: &str, topic: MessageId, order: u64) -> SourceReply {
    SourceReply {
        solution: DebateSolution {
            nick: nick(name),
            message_id: MessageId::new(),
            text: format!("{name}'s solution"),
        },
        topic_message_id: topic,
        topic: "Retained topic".into(),
        accepted_order: order,
        completed: true,
        superseded: false,
        ordinary_to_human: true,
    }
}
#[test]
fn sources_exclude_superseded_incomplete_nonordinary_and_after_cutoff() {
    let topic = MessageId::new();
    let command = MessageId::new();
    let a = source("alpha", topic, 1);
    let b = source("beta", topic, 2);
    let mut superseded = source("alpha", topic, 3);
    superseded.superseded = true;
    let mut incomplete = source("beta", topic, 4);
    incomplete.completed = false;
    let mut turn = source("alpha", topic, 5);
    turn.ordinary_to_human = false;
    let future = source("beta", topic, 101);
    let selected = sources::select(
        &[nick("alpha"), nick("beta")],
        &[a.clone(), b.clone(), superseded, incomplete, turn, future],
        &[],
        100,
        command,
        "instruction",
    )
    .unwrap();
    assert_eq!(
        selected.solutions.unwrap().map(|s| s.message_id),
        [a.solution.message_id, b.solution.message_id]
    );
}
#[test]
fn explicit_sources_take_precedence_and_invalid_or_mismatched_do_not_open() {
    let t = MessageId::new();
    let a = source("alpha", t, 1);
    let b = source("beta", t, 2);
    let newer = source("alpha", t, 3);
    let selected = sources::select(
        &[nick("alpha"), nick("beta")],
        &[a.clone(), b, newer],
        &[a.solution.message_id],
        100,
        MessageId::new(),
        "instruction",
    )
    .unwrap();
    assert_eq!(
        selected.solutions.unwrap()[0].message_id,
        a.solution.message_id
    );
    assert_eq!(
        sources::select(
            &[nick("alpha"), nick("beta")],
            &[],
            &[MessageId::new()],
            100,
            MessageId::new(),
            "fresh"
        ),
        Err(ErrorCode::DEBATE_SOURCE_MISSING)
    );
    assert_eq!(
        sources::select(
            &[nick("alpha"), nick("beta")],
            &[a, source("beta", MessageId::new(), 2)],
            &[],
            100,
            MessageId::new(),
            "fresh"
        ),
        Err(ErrorCode::DEBATE_TOPIC_MISMATCH)
    );
}
#[test]
fn fresh_and_single_missing_sources_require_openings_for_both() {
    let topic = MessageId::new();
    let command = MessageId::new();
    let a = source("alpha", topic, 1);
    let single = sources::select(
        &[nick("alpha"), nick("beta")],
        &[a],
        &[],
        100,
        command,
        "fresh",
    )
    .unwrap();
    assert!(single.solutions.is_none());
    assert_eq!(single.topic_message_id, topic);
    let fresh = sources::select(
        &[nick("alpha"), nick("beta")],
        &[],
        &[],
        100,
        command,
        "fresh",
    )
    .unwrap();
    assert!(fresh.solutions.is_none());
    assert_eq!(fresh.topic, "fresh");
    assert_eq!(fresh.topic_message_id, command);
}
fn collecting() -> ContestSelectionState {
    ContestSelectionState {
        contest_id: ContestId::new(),
        kind: ContestKind::Debate,
        state: ContestSelectionStatus::CollectingAnswers,
        state_version: 1,
        question_message_id: MessageId::new(),
        addressed: vec![nick("alpha"), nick("beta"), nick("gamma")],
        answers: vec![
            ContestAnswer {
                nick: nick("alpha"),
                message_id: MessageId::new(),
                text: "A".into(),
            },
            ContestAnswer {
                nick: nick("beta"),
                message_id: MessageId::new(),
                text: "B".into(),
            },
        ],
        required_picks: 2,
    }
}
#[test]
fn collection_waits_for_all_slots_then_pick_reuses_frozen_answers() {
    let mut state = collecting();
    assert!(
        selection::finish_collection(&state, &[nick("alpha"), nick("beta")])
            .unwrap()
            .is_none()
    );
    let requested = selection::finish_collection(&state, &state.addressed)
        .unwrap()
        .unwrap();
    assert_eq!(requested.banner, "CHOOSE YOUR DEBATE BROS");
    assert_eq!(requested.eligible.len(), 2);
    state.state = ContestSelectionStatus::AwaitingPick;
    let req = Request::ContestPick {
        request_id: EventId::new(),
        contest_id: state.contest_id,
        nicks: vec![nick("beta"), nick("alpha")],
    };
    let picked = selection::pick(&state, &req).unwrap();
    assert_eq!(selection::pick(&state, &req).unwrap(), picked);
    let sources = selection::debate_sources(&state, &picked, "Question", 100).unwrap();
    assert_eq!(
        sources.solutions.unwrap().map(|s| s.text),
        ["B", "A"].map(str::to_owned)
    );
    assert_eq!(sources.topic_message_id, state.question_message_id);
    assert!(recovery::collection_pending(&state, &[]).is_empty());
    assert_eq!(stop::selection_reason(&state), Some("pick_cancelled"));
}
#[test]
fn invalid_pick_preserves_pending_state_and_partial_failure_is_explicit() {
    let mut state = collecting();
    state.answers.pop();
    assert_eq!(
        selection::finish_collection(&state, &state.addressed),
        Err(ErrorCode::CONTEST_NOT_ENOUGH_ANSWERS)
    );
    state.state = ContestSelectionStatus::AwaitingPick;
    let original = state.clone();
    for nicks in [
        vec![nick("alpha"), nick("alpha")],
        vec![nick("alpha"), nick("gamma")],
        vec![nick("alpha")],
    ] {
        let request = Request::ContestPick {
            request_id: EventId::new(),
            contest_id: state.contest_id,
            nicks,
        };
        assert_eq!(
            selection::pick(&state, &request),
            Err(ErrorCode::CONTEST_PICK_INVALID)
        );
        assert_eq!(state, original);
    }
}
#[test]
fn pending_collection_restart_does_not_call_answered_members() {
    let s = collecting();
    assert_eq!(
        recovery::collection_pending(&s, &[nick("alpha"), nick("beta")]),
        vec![nick("gamma")]
    );
}
