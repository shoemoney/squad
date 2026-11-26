#[path = "support/m6_room.rs"]
mod live;
use squad_protocol::*;

#[tokio::test]
async fn grill_feed_preserves_committed_payloads_and_order_without_private_context() {
    let f = live::Fixture::with_roster(3).await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/v1/chat/completions"))
        .respond_with(live::fake_openrouter::streaming("PASS", "stop"))
        .with_priority(10)
        .mount(&f.fake.server)
        .await;
    let room = f.open(OpenMode::Interactive).await;
    let mut feed = room.subscribe();
    room.submit(f.request("@member0 Design a bounded queue"))
        .await;
    f.finals(1).await;
    let source = room
        .page_events(None, 500)
        .await
        .unwrap()
        .into_iter()
        .find(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
        .unwrap();
    let EnvelopePayload::MessageFinal(plan) = &source.content else {
        unreachable!()
    };
    let context = parser::ParseContext {
        roster: f.config.agents.iter().map(|a| a.nick.clone()).collect(),
        records: vec![parser::RecordSummary {
            event_id: source.base.id,
            message_id: Some(plan.completed.generation.message_id),
            seq: source.commit.seq,
            kind: Text::new("message.final").unwrap(),
            author: source.base.sender.clone(),
            recipients: source.base.recipients.clone(),
            content: plan.content.clone(),
            in_reply_to_message_id: Some(plan.in_reply_to_message_id),
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: vec![],
            state_version: None,
            superseded: false,
            eligible: true,
        }],
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    let parsed = parse("@member0 $grill", &meta, &context).unwrap();
    let result = room
        .submit(
            Request::from_parsed_with_context(EventId::new(), parsed, &meta, &context, false)
                .unwrap(),
        )
        .await;
    assert!(
        matches!(result, RequestResult::Accepted { .. }),
        "{result:?}"
    );
    f.wait(|| f.scalar("SELECT COUNT(*) FROM contests WHERE kind='grill' AND active=0") == 1)
        .await;
    let history = room.page_events(None, 500).await.unwrap();
    let last = history.last().unwrap().commit.seq;
    let mut delivered = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let FeedItem::Committed(event) = feed.recv().await.unwrap() {
                let done = event.commit.seq == last;
                delivered.push(event);
                if done {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(delivered, history);
    for kind in ["grill.setup", "grill.plan", "grill.pass", "grill.state"] {
        assert!(
            delivered
                .iter()
                .any(|e| serde_json::to_value(&e.content).unwrap()["kind"] == kind),
            "{kind}"
        );
    }
    let wire = serde_json::to_string(&delivered).unwrap();
    for forbidden in ["labelMap", "label_map", "PERSONA_", "member_prompts"] {
        assert!(!wire.contains(forbidden));
    }
    room.shutdown().await.unwrap();
}
