use squad_protocol::*;
use squad_tui::{
    actions,
    app::App,
    fx::{FeedOrigin, FxQueue, TerminalCaps},
    session::{FakeRoom, RoomPort},
    widgets::input::EditorAction,
};
use std::time::Duration;

#[tokio::test(start_paused = true)]
async fn rejection_and_stop_ack_preserve_committed_content_drafts_and_quotes_for_new_modes() {
    let room = FakeRoom::new();
    room.start_demo().await.unwrap();
    let snapshot = room.state().await;
    for kind in [
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        let mut app = App::new(
            TerminalCaps::resolve(
                |k| (k == "SQUAD_FX").then(|| "off".into()),
                false,
                (80, 24).into(),
            )
            .unwrap(),
        );
        app.session.restore(snapshot.clone());
        app.session.snapshot.context.pending_pick = None;
        app.session.snapshot.context.active_contest =
            Some(ContestState::Generic(GenericContestState {
                contest_id: ContestId::new(),
                kind,
                state_version: 1,
                status: Text::new("RUNNING").unwrap(),
                step_id: None,
                artifact_event_ids: vec![],
                terminal_reason: None,
            }));
        let draft = format!("@all ${} unfinished plan", kind.as_str());
        app.input
            .apply(EditorAction::Insert(draft.clone()))
            .unwrap();
        let quote = snapshot.context.records[0].event_id;
        app.quotes.toggle(quote);
        let before = app.session.snapshot.events.clone();
        app.acknowledge(RequestResult::Rejected {
            request_id: EventId::new(),
            error: ErrorPayload {
                code: ErrorCode::DAILY_MAX_REACHED,
                text: Text::new("Daily max reached; in-flight calls continue.").unwrap(),
                retryable: false,
                related_event_id: None,
                work_id: None,
                debate_id: None,
                case_id: None,
                earliest_cursor: None,
                retry_action: None,
                contest_id: None,
                provider_status: None,
                provider_message: None,
            },
        });
        assert_eq!(app.input.text(), draft);
        assert_eq!(app.quotes.ids, vec![quote]);
        assert_eq!(app.session.snapshot.events, before);
        app.queue(actions::stop());
        assert!(app.awaiting_ack);
        assert!(app.session.active_contest());
        assert_eq!(app.session.snapshot.events, before);
    }
}

#[tokio::test(start_paused = true)]
async fn replayed_scoreboards_replace_and_clear_waits_for_the_authoritative_event() {
    let room = FakeRoom::new();
    room.start_demo().await.unwrap();
    let mut app = App::new(TerminalCaps::resolve(|_| None, false, (80, 24).into()).unwrap());
    app.session.restore(room.state().await);
    let original = app.session.snapshot.scoreboard.clone();
    let event = app
        .session
        .snapshot
        .events
        .iter()
        .find(|e| matches!(e.content, EnvelopePayload::ScoresChanged(_)))
        .unwrap()
        .clone();
    let mut q = FxQueue::new();
    assert!(!app.session.reduce(
        FeedItem::Committed(event.clone()),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q
    ));
    let mut repeated = event;
    repeated.base.id = EventId::new();
    repeated.commit.seq = Cursor::new(app.session.snapshot.cursor.unwrap().get() + 1).unwrap();
    app.session.reduce(
        FeedItem::Committed(repeated),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q,
    );
    assert_eq!(app.session.snapshot.scoreboard, original);
    let spend = app.session.snapshot.daily_spend.spent_usd;
    app.queue(Request::Clear {
        request_id: EventId::new(),
    });
    assert!(!app.session.transcript.cards.is_empty());
    assert_eq!(app.session.snapshot.scoreboard, original);
    room.commit(
        Actor::System,
        EnvelopePayload::RoomCleared(RoomCleared {
            epoch: 2,
            previous_through: None,
        }),
    );
    let cleared = room
        .state()
        .await
        .events
        .into_iter()
        .find(|e| matches!(e.content, EnvelopePayload::RoomCleared(_)))
        .unwrap();
    app.session.reduce(
        FeedItem::Committed(cleared),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut q,
    );
    assert!(app.session.transcript.cards.is_empty());
    assert!(app.session.snapshot.scoreboard.is_empty());
    assert_eq!(app.session.snapshot.daily_spend.spent_usd, spend);
}
