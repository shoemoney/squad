use squad_protocol::*;
use squad_tui::{actions, quotes::Quotes, session::FakeRoom};
#[tokio::test]
async fn export_success_requires_destination_write_and_flush() {
    struct Fails;
    impl std::io::Write for Fails {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("flush failed"))
        }
    }
    let room = FakeRoom::new();
    let mut destination = vec![];
    actions::export_to(&room, ExportScope::Room, None, &mut destination)
        .await
        .unwrap();
    assert!(
        actions::export_to(&room, ExportScope::Room, None, &mut Fails)
            .await
            .is_err()
    );
}
#[test]
fn quote_selection_is_ordered_bounded_and_unavailable_is_explicit() {
    let mut quotes = Quotes::default();
    let first = EventId::new();
    quotes.toggle(first);
    for _ in 0..20 {
        quotes.toggle(EventId::new());
    }
    assert_eq!(quotes.ids.len(), 12);
    assert_eq!(quotes.metadata().quote[0], first);
    assert!(!quotes.previews(&Default::default())[0].available);
    quotes.toggle(first);
    assert_eq!(quotes.ids.len(), 11);
}

#[tokio::test]
async fn file_export_writes_core_text_and_preserves_existing_destination() {
    let room = FakeRoom::new();
    let message_id = MessageId::new();
    room.commit(
        Actor::Me,
        EnvelopePayload::UserMessage(UserMessage {
            input: UserMessageInput {
                message_id,
                text: Text::new("@claude export fixture").unwrap(),
                quote: None,
                reply_to: None,
            },
            body: "export fixture".into(),
            resolved_quotes: vec![],
            debate_id: None,
            case_id: None,
            contest_id: None,
        }),
    );
    let path = std::env::temp_dir().join(format!("squad-export-{}.md", EventId::new()));
    let job = actions::ExportJob {
        request_id: EventId::new(),
        scope: ExportScope::Room,
        id: None,
        path: path.clone(),
    };
    assert!(matches!(
        actions::export_file(&room, job.clone()).await,
        RequestResult::Internal(InternalResult::Text { .. })
    ));
    let original = std::fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&original).contains("export fixture"));
    assert!(matches!(
        actions::export_file(&room, job).await,
        RequestResult::Rejected { .. }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn disabled_scaffold_modes_are_absent_from_completion_and_help() {
    let ctx = squad_protocol::parser::ParseContext {
        roster: ["a", "b", "c", "d"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let examples = squad_tui::typeahead::examples(&ctx);
    for mode in [
        "forecast",
        "negotiate",
        "blow_it_up",
        "readback",
        "debug",
        "trace",
        "rank",
        "tune",
        "bass_ackwards_prd",
    ] {
        let token = format!("${mode}");
        assert!(!squad_tui::typeahead::MODES.iter().any(|(m, _)| *m == token));
        assert!(
            !squad_protocol::parser::help_examples()
                .iter()
                .any(|e| e.contains(&token))
        );
        assert!(!examples.iter().any(|e| e.contains(&token)));
    }
    for example in examples {
        squad_protocol::parse(&example, &Default::default(), &ctx).unwrap();
    }
}
