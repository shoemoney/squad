use ratatui::{Terminal, backend::TestBackend};
use squad_tui::{
    app::App,
    event_loop::{self, FRAME_PERIOD, frame_deadline},
    fx::TerminalCaps,
    session::FakeRoom,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
#[test]
fn frame_scheduler_caps_every_source_and_disarms_idle() {
    assert_eq!(
        frame_deadline(Duration::ZERO, None, false, None, None),
        None
    );
    assert_eq!(
        frame_deadline(Duration::ZERO, Some(Duration::ZERO), true, None, None),
        Some(FRAME_PERIOD)
    );
    assert_eq!(
        frame_deadline(
            Duration::ZERO,
            None,
            false,
            Some(Duration::from_millis(50)),
            None
        ),
        Some(Duration::from_millis(50))
    );
    assert_eq!(
        frame_deadline(
            Duration::from_secs(5),
            Some(Duration::ZERO),
            false,
            None,
            None
        ),
        None
    );
}
#[tokio::test]
async fn idle_room_draws_once_and_stays_disarmed_for_five_seconds() {
    let mut app = App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap());
    let room = Arc::new(FakeRoom::new());
    let (tx, mut rx) = mpsc::channel(8);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(tx);
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    event_loop::run(&mut app, room, &mut terminal, &mut rx)
        .await
        .unwrap();
    assert_eq!(app.draws, 1);
}
#[tokio::test]
async fn ready_input_precedes_initial_draw() {
    let mut app = App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap());
    let (tx, mut rx) = mpsc::channel(8);
    drop(tx);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    event_loop::run(&mut app, Arc::new(FakeRoom::new()), &mut terminal, &mut rx)
        .await
        .unwrap();
    assert_eq!(app.draws, 0);
}

#[tokio::test]
async fn sustained_feed_draws_between_bounded_batches_and_quit_stays_responsive() {
    use squad_protocol::*;
    let room = Arc::new(FakeRoom::new());
    let mut app = App::new(
        TerminalCaps::resolve(|_| None, false, ratatui::layout::Size::new(80, 24)).unwrap(),
    );
    let sender = room.clone();
    let (tx, mut rx) = mpsc::channel(8);
    let producer = tokio::spawn(async move {
        let item = FeedItem::Presence(ProducerEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: EventId::new(),
                room_id: MainRoom::Main,
                correlation_id: EventId::new(),
                causation_id: None,
                sender: Actor::System,
                recipients: vec![Recipient::Me],
                created_at: std::time::SystemTime::now().into(),
            },
            content: EnvelopePayload::Presence(Presence {
                nick: Nick::new("claude").unwrap(),
                status: PresenceStatus::Idle,
                work_id: None,
                queue_depth: 0,
                error_code: None,
                observed_at: std::time::SystemTime::now().into(),
            }),
        });
        for _ in 0..30 {
            for _ in 0..512 {
                sender.emit(item.clone());
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        drop(tx);
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    event_loop::run(&mut app, room, &mut terminal, &mut rx)
        .await
        .unwrap();
    producer.await.unwrap();
    assert!(app.feed_items > 512, "feed was exercised");
    assert_eq!(app.max_feed_batch, 256);
    assert!(app.draws > 1, "sustained feed cannot starve drawing");
}

#[tokio::test]
async fn effects_finish_then_idle_timer_stays_disarmed() {
    use squad_tui::fx::{CatalogueEffect, FeedOrigin, FxCue, TimedCue};
    let mut app = App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap());
    app.fx.queue_mut().push(
        FeedOrigin::Live,
        TimedCue {
            event_id: squad_protocol::EventId::new(),
            at: Duration::ZERO,
            cue: FxCue::Catalogue {
                effect: CatalogueEffect::Chrome,
                target: "logo".into(),
                duration: Duration::from_millis(100),
            },
        },
    );
    let (tx, mut rx) = mpsc::channel(8);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5300)).await;
        drop(tx);
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    event_loop::run(&mut app, Arc::new(FakeRoom::new()), &mut terminal, &mut rx)
        .await
        .unwrap();
    assert!(
        (2..=12).contains(&app.draws),
        "draws should end with the 100ms effect, got {}",
        app.draws
    );
    assert!(app.fx.next_wake().is_none());
}

#[tokio::test]
async fn feed_lag_recovers_authoritative_state_without_losing_draft() {
    use squad_protocol::*;
    use squad_tui::widgets::input::EditorAction;
    let room = Arc::new(FakeRoom::new());
    let producer = room.clone();
    let mut app = App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap());
    app.input
        .apply(EditorAction::Insert("unfinished".into()))
        .unwrap();
    let (tx, mut rx) = mpsc::channel(8);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(2)).await;
        let item = FeedItem::Presence(ProducerEnvelope {
            base: EnvelopeBase {
                version: 1,
                id: EventId::new(),
                room_id: MainRoom::Main,
                correlation_id: EventId::new(),
                causation_id: None,
                sender: Actor::System,
                recipients: vec![Recipient::Me],
                created_at: std::time::SystemTime::now().into(),
            },
            content: EnvelopePayload::Presence(Presence {
                nick: Nick::new("claude").unwrap(),
                status: PresenceStatus::Idle,
                work_id: None,
                queue_depth: 0,
                error_code: None,
                observed_at: std::time::SystemTime::now().into(),
            }),
        });
        for _ in 0..6000 {
            producer.emit(item.clone());
        }
        producer.commit(
            Actor::System,
            EnvelopePayload::BudgetState(BudgetStateEvent {
                spent_usd: 3.,
                reserved_usd: 0.,
                limit_usd: 25.,
                estimated_usd: 0.,
                unresolved_attempts: 0,
                day: "2026-11-24".into(),
                unknown_calls: 0,
            }),
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(tx);
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    event_loop::run(&mut app, room, &mut terminal, &mut rx)
        .await
        .unwrap();
    assert_eq!(app.input.text(), "unfinished");
    assert_eq!(app.session.snapshot.daily_spend.spent_usd, 3.);
    assert_eq!(app.notice.as_deref(), Some("Room feed restored."));
}
