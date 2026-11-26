use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    event_loop,
    fx::{CatalogueEffect, FeedOrigin, FxCue, TerminalCaps, TimedCue},
    session::{FakeRoom, RoomError, RoomFeed, RoomFuture, RoomPort, RoomSnapshot},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;

struct ObservedRoom {
    room: FakeRoom,
    submissions: AtomicUsize,
}
impl RoomPort for ObservedRoom {
    fn submit(&self, request: Request) -> RoomFuture<'_, RequestResult> {
        self.submissions.fetch_add(1, Ordering::SeqCst);
        self.room.submit(request)
    }
    fn subscribe(&self, after: Option<Cursor>) -> Result<RoomFeed, RoomError> {
        self.room.subscribe(after)
    }
    fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> RoomFuture<'_, Result<Vec<CommittedEnvelope>, RoomError>> {
        self.room.page_events(before, limit)
    }
    fn state(&self) -> RoomFuture<'_, RoomSnapshot> {
        self.room.state()
    }
    fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> RoomFuture<'_, Result<String, RoomError>> {
        self.room.export(scope, id)
    }
    fn shutdown(&self) -> RoomFuture<'_, Result<(), RoomError>> {
        self.room.shutdown()
    }
}

#[tokio::test]
async fn new_effects_do_not_gate_moves_barriers_or_change_deliveries_and_export() {
    for effect in [
        CatalogueEffect::Census,
        CatalogueEffect::Crosscheck,
        CatalogueEffect::Survey,
        CatalogueEffect::Probe,
    ] {
        let room = Arc::new(ObservedRoom {
            room: FakeRoom::new(),
            submissions: AtomicUsize::new(0),
        });
        room.room.commit(
            Actor::Me,
            EnvelopePayload::UserMessage(UserMessage {
                input: UserMessageInput {
                    message_id: MessageId::new(),
                    text: Text::new("@claude durable evidence").unwrap(),
                    quote: None,
                    reply_to: None,
                },
                body: "durable evidence".into(),
                resolved_quotes: vec![],
                debate_id: None,
                case_id: None,
                contest_id: None,
            }),
        );
        let before = room.export(ExportScope::Room, None).await.unwrap();
        assert!(before.contains("durable evidence"));
        let id = ContestId::new();
        let mut app = App::new(
            TerminalCaps::resolve(
                |k| match k {
                    "SQUAD_FX" => Some("full".into()),
                    "COLORTERM" => Some("truecolor".into()),
                    _ => None,
                },
                true,
                (80, 24).into(),
            )
            .unwrap(),
        );
        app.composed.started = true;
        app.tips_enabled = false;
        app.fx.queue_mut().push(
            FeedOrigin::Live,
            TimedCue {
                event_id: EventId::new(),
                at: Duration::ZERO,
                cue: FxCue::Catalogue {
                    effect,
                    target: id.to_string(),
                    duration: Duration::from_secs(5),
                },
            },
        );
        let (tx, mut rx) = mpsc::channel(8);
        let sender = room.clone();
        let producer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            for (ordinal, move_name) in [(1, ContestMove::Fanout), (2, ContestMove::Barrier)] {
                sender.room.commit(
                    Actor::System,
                    EnvelopePayload::ContestStep(ContestStep {
                        contest_id: id,
                        step_id: StepId::new(),
                        ordinal,
                        script_version: Text::new("fixture-v1").unwrap(),
                        move_name,
                        inputs: vec![],
                        assignments: vec![],
                    }),
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(tx);
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        event_loop::run(&mut app, room.clone(), &mut terminal, &mut rx)
            .await
            .unwrap();
        producer.await.unwrap();
        assert!(
            app.now() < Duration::from_secs(5),
            "feed processing waited for {effect:?}"
        );
        assert_eq!(
            app.session.contests[&id].steps[&2].move_name,
            ContestMove::Barrier
        );
        assert!(app.feed_items >= 2);
        assert_eq!(room.submissions.load(Ordering::SeqCst), 0);
        assert_eq!(room.export(ExportScope::Room, None).await.unwrap(), before);
    }
}
