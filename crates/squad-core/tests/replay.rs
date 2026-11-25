use squad_core::router::Router;
use squad_protocol::*;
use squad_store::Store;
use std::collections::BTreeMap;
fn nick(name: &str) -> MemberNick {
    serde_json::from_value(serde_json::json!(name)).unwrap()
}
fn fixture() -> (Store, Router) {
    let store = Store::open(":memory:").unwrap();
    let roster = vec![nick("alice"), nick("bob")];
    let mut bindings = roster
        .iter()
        .map(|nick| {
            (
                nick.to_string(),
                ExecutorBinding {
                    model: Text::new("test/model").unwrap(),
                    effort: ReasoningEffort::Low,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    bindings.insert(
        "judge".into(),
        ExecutorBinding {
            model: Text::new("test/judge").unwrap(),
            effort: ReasoningEffort::Low,
        },
    );
    let router = Router::new(store.clone(), roster, bindings, 20_000_000).unwrap();
    (store, router)
}
#[tokio::test]
async fn broadcast_lag_catches_up_whole_durable_groups_and_discards_chunks() {
    let (store, mut router) = fixture();
    let (feed, mut receiver) = tokio::sync::broadcast::channel(2);
    let mut replay = squad_store::events::FeedReplay::default();
    // Subscription precedes the durable high water and intentionally falls behind.
    for value in 1..=5 {
        let outcome = router
            .process(
                Request::SetVars {
                    request_id: EventId::new(),
                    updates: vec![
                        VariableUpdate {
                            name: VariableName::Rounds,
                            value,
                        },
                        VariableUpdate {
                            name: VariableName::CensusPasses,
                            value,
                        },
                    ],
                },
                None,
            )
            .unwrap();
        for event in outcome.events {
            feed.send(FeedItem::Committed(event)).unwrap();
        }
    }
    let chunk = squad_core::router::envelope(
        EventId::new(),
        Actor::Member(nick("alice")),
        vec![Recipient::Me],
        EnvelopePayload::Delta(Delta {
            generation: GenerationIdentity {
                message_id: MessageId::new(),
                work_id: WorkId::new(),
                trigger_event_id: EventId::new(),
                attempt: 1,
            },
            chunk_index: 0,
            text: "not recoverable".into(),
            debate_id: None,
            round: None,
            case_id: None,
            contest_id: None,
            step_id: None,
        }),
    );
    feed.send(FeedItem::Delta(chunk)).unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ));
    store
        .call(move |db| {
            let mut events = Vec::new();
            loop {
                let next = replay.catch_up(db, 1).unwrap();
                if next.is_empty() {
                    break;
                }
                assert_eq!(next.len(), 2);
                events.extend(next);
            }
            assert_eq!(events.len(), 10);
            assert!(
                events
                    .iter()
                    .all(|e| matches!(e.content, EnvelopePayload::VarChange(_)))
            );
            assert_eq!(replay.after.unwrap().get(), 10);
            Ok(())
        })
        .unwrap();
}
