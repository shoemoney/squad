use squad_protocol::*;
use squad_tui::{
    fx::{FeedOrigin, FxQueue},
    session::{FakeRoom, RoomPort, Session},
};
use std::time::Duration;
fn budget() -> EnvelopePayload {
    EnvelopePayload::BudgetState(BudgetStateEvent {
        spent_usd: 4.,
        reserved_usd: 1.,
        limit_usd: 25.,
        estimated_usd: 0.,
        unresolved_attempts: 0,
        day: "2026-11-24".into(),
        unknown_calls: 1,
    })
}
#[tokio::test]
async fn subscribe_before_snapshot_reconciles_duplicates_and_budget() {
    let room = FakeRoom::new();
    let mut feed = room.subscribe(None).unwrap();
    room.commit(Actor::System, budget());
    let state = room.state().await;
    let mut session = Session::default();
    session.restore(state);
    let mut q = FxQueue::new();
    assert!(!session.reduce(
        feed.recv().await.unwrap().item,
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q
    ));
    assert_eq!(session.snapshot.daily_spend.spent_usd, 4.);
    assert!(q.is_empty());
    let event = session.snapshot.events[0].clone();
    let mut fresh = Session::default();
    fresh.reduce(
        FeedItem::Committed(event),
        FeedOrigin::Replay,
        Duration::ZERO,
        &mut q,
    );
    assert_eq!(fresh.snapshot.daily_spend.spent_usd, 4.);
    assert!(q.is_empty());
}
#[tokio::test]
async fn paging_and_cursor_catchup_are_exclusive() {
    let room = FakeRoom::new();
    room.commit(Actor::System, budget());
    let cursor = room.state().await.cursor.unwrap();
    room.commit(Actor::System, budget());
    let mut feed = room.subscribe(Some(cursor)).unwrap();
    let FeedItem::Committed(e) = feed.recv().await.unwrap().item else {
        panic!()
    };
    assert!(e.commit.seq > cursor);
    let page = room.page_events(Some(e.commit.seq), 256).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].commit.seq, cursor);
    assert!(room.subscribe(Some(Cursor::new(100).unwrap())).is_err());
}
#[tokio::test]
async fn restored_pick_has_no_expiry_and_no_entrance() {
    let room = FakeRoom::new();
    let id = ContestId::new();
    let nick = Nick::new("claude").unwrap();
    room.commit(
        Actor::System,
        EnvelopePayload::ContestState(ContestState::Selection(ContestSelectionState {
            contest_id: id,
            kind: ContestKind::Debate,
            state: ContestSelectionStatus::AwaitingPick,
            state_version: 1,
            question_message_id: MessageId::new(),
            addressed: vec![nick.clone()],
            answers: vec![ContestAnswer {
                nick,
                message_id: MessageId::new(),
                text: "answer".into(),
            }],
            required_picks: 2,
        })),
    );
    let mut session = Session::default();
    session.restore(room.state().await);
    assert_eq!(session.pick.unwrap().request.contest_id, id);
    assert!(session.snapshot.context.pending_pick.is_some());
}
#[tokio::test(start_paused = true)]
async fn realistic_demo_finishes_six_streams_debate_and_verdict_without_network() {
    let room = FakeRoom::new();
    room.start_demo().await.unwrap();
    let state = room.state().await;
    for event in &state.events {
        event
            .validate()
            .unwrap_or_else(|e| panic!("invalid demo event {:?}: {e:?}", event.content));
    }
    assert_eq!(state.context.roster.len(), 6);
    assert_eq!(
        state
            .events
            .iter()
            .filter(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
            .count(),
        6
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|e| matches!(e.content, EnvelopePayload::DebateTurn(_)))
            .count(),
        6
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|e| matches!(e.content, EnvelopePayload::Verdict(_)))
            .count(),
        1
    );
    let mut session = Session::default();
    session.restore(state);
    assert_eq!(session.transcript.cards.len(), 13);
    assert!(!session.active_contest());
    assert!(session.contests.values().any(|v| v.verdict.is_some()));
}

#[test]
fn streamed_attempts_batch_and_terminal_outcomes_ignore_late_chunks() {
    let mut session = Session::default();
    let mut queue = FxQueue::new();
    let generation = GenerationIdentity {
        message_id: MessageId::new(),
        work_id: WorkId::new(),
        trigger_event_id: EventId::new(),
        attempt: 1,
    };
    let base = EnvelopeBase {
        version: 1,
        id: EventId::new(),
        room_id: MainRoom::Main,
        correlation_id: generation.trigger_event_id,
        causation_id: None,
        sender: Actor::Member(Nick::new("claude").unwrap()),
        recipients: vec![Recipient::Me],
        created_at: std::time::SystemTime::now().into(),
    };
    let delta = |attempt, chunk, text: &str| {
        FeedItem::Delta(ProducerEnvelope {
            base: base.clone(),
            content: EnvelopePayload::Delta(Delta {
                generation: GenerationIdentity {
                    attempt,
                    ..generation.clone()
                },
                chunk_index: chunk,
                text: text.into(),
                debate_id: None,
                round: None,
                case_id: None,
                contest_id: None,
                step_id: None,
            }),
        })
    };
    assert!(!session.reduce(
        delta(1, 0, "old"),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut queue
    ));
    assert_eq!(session.transcript.cards[&generation.message_id].text, "");
    assert!(!session.transcript.flush(Duration::from_millis(49)));
    assert!(session.transcript.flush(Duration::from_millis(50)));
    session.reduce(
        delta(2, 0, "new"),
        FeedOrigin::Live,
        Duration::from_millis(60),
        &mut queue,
    );
    session.reduce(
        delta(1, 1, " late"),
        FeedOrigin::Live,
        Duration::from_millis(61),
        &mut queue,
    );
    session.transcript.flush(Duration::from_millis(110));
    assert_eq!(session.transcript.cards[&generation.message_id].text, "new");
    let final_event = CommittedEnvelope {
        base: base.clone(),
        commit: CommitFields {
            seq: Cursor::new(1).unwrap(),
            accepted_at: base.created_at,
            commit_index: 0,
            commit_size: 1,
        },
        content: EnvelopePayload::MessageFinal(Completion {
            completed: CompletedGeneration {
                generation: GenerationIdentity {
                    attempt: 2,
                    ..generation.clone()
                },
                model: Text::new("demo/model").unwrap(),
                finish_reason: FinishReason::Stop,
                usage: None,
            },
            in_reply_to_message_id: MessageId::new(),
            content: "authoritative".into(),
        }),
    };
    session.reduce(
        FeedItem::Committed(final_event),
        FeedOrigin::Live,
        Duration::from_millis(120),
        &mut queue,
    );
    session.reduce(
        delta(2, 1, " late"),
        FeedOrigin::Live,
        Duration::from_millis(130),
        &mut queue,
    );
    assert_eq!(
        session.transcript.cards[&generation.message_id].text,
        "authoritative"
    );
    assert!(!session.transcript.streaming());
}

#[tokio::test]
async fn authoritative_clear_preserves_daily_spend_and_reset_removes_contest() {
    let room = FakeRoom::new();
    room.commit(Actor::System, budget());
    room.commit(
        Actor::System,
        EnvelopePayload::RoomCleared(RoomCleared {
            epoch: 2,
            previous_through: None,
        }),
    );
    let mut session = Session::default();
    session.restore(room.state().await);
    assert_eq!(session.snapshot.daily_spend.spent_usd, 4.);
    assert!(session.transcript.cards.is_empty());
    assert!(!session.active_contest());
}
#[tokio::test]
async fn older_pages_preserve_newer_snapshot_values_and_anchor() {
    let room = FakeRoom::new();
    room.commit(Actor::System, budget());
    room.commit(
        Actor::System,
        EnvelopePayload::BudgetState(BudgetStateEvent {
            spent_usd: 6.,
            reserved_usd: 0.,
            limit_usd: 30.,
            estimated_usd: 0.,
            unresolved_attempts: 0,
            day: "2026-11-25".into(),
            unknown_calls: 0,
        }),
    );
    let mut snapshot = room.state().await;
    snapshot.events.remove(0);
    let mut session = Session::default();
    session.restore(snapshot);
    assert_eq!(session.page_older(&room).await.unwrap(), 1);
    assert_eq!(session.snapshot.daily_spend.spent_usd, 6.);
    assert_eq!(session.snapshot.daily_spend.day, "2026-11-25");
}
