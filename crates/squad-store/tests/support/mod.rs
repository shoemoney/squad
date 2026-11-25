use squad_protocol::*;
use squad_store::Store;
pub fn event(store: &Store) -> CommittedEnvelope {
    let id = EventId::new();
    store
        .commit(id, "fixture".into(), id.to_string(), move |tx, group| {
            group.push(
                tx,
                ProducerEnvelope {
                    base: EnvelopeBase {
                        version: 1,
                        id,
                        room_id: MainRoom::Main,
                        correlation_id: id,
                        causation_id: None,
                        sender: Actor::System,
                        recipients: vec![Recipient::Me],
                        created_at: group.accepted_at,
                    },
                    content: EnvelopePayload::SystemNotice(SystemNotice {
                        code: Text::new("FIXTURE").unwrap(),
                        text: Text::new("retained material").unwrap(),
                        work_id: None,
                        debate_id: None,
                        case_id: None,
                        contest_id: None,
                    }),
                },
            )?;
            Ok(group.accepted(id))
        })
        .unwrap()
        .events
        .remove(0)
}
