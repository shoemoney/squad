use squad_core::state::{DailySpend, RoomState, RoomVars};
use squad_protocol::*;
use squad_tui::{
    fx::{FeedOrigin, FxQueue},
    room_adapter::snapshot,
    session::Session,
};
use std::{collections::BTreeMap, time::Duration};

#[test]
fn core_snapshot_and_feed_use_the_same_reducer() {
    let nick: Nick = "a".parse().unwrap();
    let state = RoomState {
        room: "main".into(),
        vars: RoomVars {
            rounds: 4,
            grill_rounds: 3,
            census_passes: 2,
        },
        participants: vec![Presence {
            nick: nick.clone(),
            status: PresenceStatus::Idle,
            work_id: None,
            queue_depth: 0,
            error_code: None,
            observed_at: std::time::SystemTime::now().into(),
        }],
        debates: vec![],
        pending_preflights: vec![],
        models: vec![],
        scoreboard: vec![],
        trophies: BTreeMap::from([(nick, 2)]),
        active_contest: None,
        daily_spend: DailySpend {
            day: "2026-11-24".into(),
            spent_usd: 0.25,
            reserved_usd: 0.1,
            limit_usd: 2.,
            unknown_calls: 1,
            has_unreconciled_charges: true,
        },
    };
    let id = EventId::new();
    let message_id = MessageId::new();
    let event = CommittedEnvelope {
        base: EnvelopeBase {
            version: 1,
            id,
            room_id: MainRoom::Main,
            correlation_id: id,
            causation_id: None,
            sender: Actor::Me,
            recipients: vec![Recipient::Member("a".parse().unwrap())],
            created_at: std::time::SystemTime::now().into(),
        },
        commit: CommitFields {
            seq: Cursor::new(1).unwrap(),
            accepted_at: std::time::SystemTime::now().into(),
            commit_index: 0,
            commit_size: 1,
        },
        content: EnvelopePayload::UserMessage(UserMessage {
            input: UserMessageInput {
                message_id,
                text: Text::new("@a fixture").unwrap(),
                quote: None,
                reply_to: None,
            },
            body: "fixture".into(),
            resolved_quotes: vec![],
            debate_id: None,
            case_id: None,
            contest_id: None,
        }),
    };
    let view = snapshot(&state, vec![event.clone()]);
    assert_eq!(view.context.vars[&VariableName::Rounds], 4);
    assert_eq!(view.daily_spend.unknown_calls, 1);
    assert_eq!(view.trophies.values().copied().sum::<u64>(), 2);
    assert_eq!(view.context.records[0].event_id, id);
    let mut session = Session::default();
    session.restore(view);
    let mut cues = FxQueue::new();
    assert!(!session.reduce(
        FeedItem::Committed(event),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut cues
    ));
    assert_eq!(session.transcript.cards[&message_id].text, "fixture");
    assert_eq!(session.seen.len(), 1);
}
