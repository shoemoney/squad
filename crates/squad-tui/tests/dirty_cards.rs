use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::{FeedOrigin, FxQueue, TerminalCaps},
    session::{FakeRoom, RoomPort, Session},
    ui,
};
use std::time::Duration;

fn app() -> App {
    App::new(
        TerminalCaps::resolve(
            |k| (k == "SQUAD_FX").then(|| "off".into()),
            true,
            (140, 42).into(),
        )
        .unwrap(),
    )
}
fn draw(app: &mut App, width: u16) {
    Terminal::new(TestBackend::new(width, 42))
        .unwrap()
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
}
fn generation() -> GenerationIdentity {
    GenerationIdentity {
        message_id: MessageId::new(),
        work_id: WorkId::new(),
        trigger_event_id: EventId::new(),
        attempt: 1,
    }
}
fn base(g: &GenerationIdentity) -> EnvelopeBase {
    EnvelopeBase {
        version: 1,
        id: EventId::new(),
        room_id: MainRoom::Main,
        correlation_id: g.trigger_event_id,
        causation_id: None,
        sender: Actor::Member(Nick::new("claude").unwrap()),
        recipients: vec![Recipient::Me],
        created_at: std::time::SystemTime::now().into(),
    }
}
fn delta(session: &mut Session, g: &GenerationIdentity, chunk: u64, text: &str) {
    session.reduce(
        FeedItem::Delta(ProducerEnvelope {
            base: base(g),
            content: EnvelopePayload::Delta(Delta {
                generation: g.clone(),
                chunk_index: chunk,
                text: text.into(),
                debate_id: None,
                round: None,
                case_id: None,
                contest_id: None,
                step_id: None,
            }),
        }),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut FxQueue::new(),
    );
}
fn commit(session: &mut Session, g: &GenerationIdentity, content: EnvelopePayload) {
    let base = base(g);
    let seq = session.snapshot.events.len() as i64 + 1;
    session.reduce(
        FeedItem::Committed(CommittedEnvelope {
            commit: CommitFields {
                seq: Cursor::new(seq).unwrap(),
                accepted_at: base.created_at,
                commit_index: 0,
                commit_size: 1,
            },
            base,
            content,
        }),
        FeedOrigin::Live,
        Duration::ZERO,
        &mut FxQueue::new(),
    );
}
fn status(session: &mut Session, g: &GenerationIdentity, status: WorkStatus) {
    commit(
        session,
        g,
        EnvelopePayload::WorkState(WorkStateEvent {
            work_id: g.work_id,
            trigger_event_id: g.trigger_event_id,
            target: Target::Member(Nick::new("claude").unwrap()),
            context_id: ContextId("main".into()),
            attempt: g.attempt,
            status,
            error: None,
            debate_id: None,
            case_id: None,
            contest_id: None,
        }),
    );
}

#[test]
fn static_history_does_no_frame_bookkeeping_and_only_published_cards_are_prepared() {
    let mut app = app();
    for _ in 0..1000 {
        app.session.transcript.complete(
            MessageId::new(),
            Actor::Me,
            0,
            "cached **history** 世界".into(),
        );
    }
    let g = generation();
    delta(&mut app.session, &g, 0, "first");
    app.expire(Duration::from_millis(50));
    draw(&mut app, 140);
    let inspected = app.composed.prepared_cards;
    let work = app.session.transcript.work;
    for _ in 0..20 {
        app.expire(Duration::from_millis(60));
        draw(&mut app, 140);
    }
    assert_eq!(
        app.composed.prepared_cards, inspected,
        "unchanged history must do zero card lookups"
    );
    assert_eq!(
        app.session.transcript.work, work,
        "no repeated history insertion or flush"
    );
    let revision = app.session.transcript.revision;
    delta(&mut app.session, &g, 1, " pending");
    draw(&mut app, 140);
    assert_eq!(
        app.session.transcript.revision, revision,
        "pending transport is not published text"
    );
    assert_eq!(app.composed.prepared_cards, inspected);
    app.expire(Duration::from_millis(100));
    draw(&mut app, 140);
    assert_eq!(app.composed.prepared_cards, inspected + 1);
    assert_eq!(
        app.session.transcript.work.flushed_cards,
        work.flushed_cards + 1
    );
    assert_eq!(
        app.composed
            .transcript
            .message(g.message_id)
            .unwrap()
            .source,
        "first pending"
    );
    let inspected = app.composed.prepared_cards;
    draw(&mut app, 80);
    assert_eq!(
        app.composed.prepared_cards, inspected,
        "width reflows cached rows without rechecking cards"
    );
}

#[test]
fn terminal_retry_and_supersession_publish_without_waiting_for_a_text_flush() {
    for terminal in [WorkStatus::Failed, WorkStatus::Cancelled] {
        let mut app = app();
        let mut g = generation();
        delta(&mut app.session, &g, 0, "partial");
        draw(&mut app, 140);
        status(&mut app.session, &g, terminal.clone());
        draw(&mut app, 140);
        let model = app.composed.transcript.message(g.message_id).unwrap();
        assert_eq!(model.source, "partial");
        assert_eq!(
            model.status,
            if terminal == WorkStatus::Failed {
                "incomplete · failed"
            } else {
                "incomplete · cancelled"
            }
        );
        let revision = app.session.transcript.revision;
        delta(&mut app.session, &g, 1, "late");
        assert_eq!(app.session.transcript.revision, revision);
        assert!(!app.session.transcript.streaming());
        g.attempt += 1;
        delta(&mut app.session, &g, 0, "retried");
        draw(&mut app, 140);
        assert_eq!(
            app.composed
                .transcript
                .message(g.message_id)
                .unwrap()
                .status,
            "streaming"
        );
        app.session.transcript.flush(Duration::from_millis(50));
        app.session
            .transcript
            .complete(g.message_id, Actor::Me, g.attempt, "durable".into());
        draw(&mut app, 140);
        assert_eq!(
            app.composed
                .transcript
                .message(g.message_id)
                .unwrap()
                .status,
            "complete"
        );
        let replacement = generation();
        commit(
            &mut app.session,
            &g,
            EnvelopePayload::MessageSuperseded(MessageSuperseded {
                superseded_event_id: EventId::new(),
                superseded_message_id: g.message_id,
                member: Nick::new("claude").unwrap(),
                in_reply_to_message_id: MessageId::new(),
                replacement_work_id: replacement.work_id,
                replacement_message_id: replacement.message_id,
            }),
        );
        draw(&mut app, 140);
        assert_eq!(
            app.composed
                .transcript
                .message(g.message_id)
                .unwrap()
                .status,
            "superseded"
        );
        delta(&mut app.session, &replacement, 0, "replacement");
        draw(&mut app, 140);
        assert!(
            app.composed
                .transcript
                .message(g.message_id)
                .unwrap()
                .before_body[0]
                .to_string()
                .contains("@claude")
        );
    }
}

#[tokio::test(start_paused = true)]
async fn replay_paging_clear_and_restore_keep_anchors_and_unread_membership() {
    let room = FakeRoom::new();
    room.start_demo().await.unwrap();
    let snapshot = room.state().await;
    let mut app = app();
    app.session.restore(snapshot.clone());
    draw(&mut app, 140);
    app.page(true);
    draw(&mut app, 140);
    let anchor = app.composed.anchor.unwrap();
    let g = generation();
    delta(&mut app.session, &g, 0, "unread");
    delta(&mut app.session, &g, 1, " tail");
    assert_eq!(app.session.transcript.new_activity(), 1);
    status(&mut app.session, &g, WorkStatus::Cancelled);
    assert_eq!(app.session.transcript.new_activity(), 1);
    draw(&mut app, 80);
    assert_eq!(app.composed.anchor, Some(anchor));
    app.resume_live();
    assert_eq!(app.session.transcript.new_activity(), 0);
    app.page(true);
    delta(&mut app.session, &generation(), 0, "another");
    assert_eq!(app.session.transcript.new_activity(), 1);
    // Loading a durable older page is a restore; historical cards are never unread.
    app.session.page_older(&room).await.unwrap();
    draw(&mut app, 140);
    assert_eq!(
        app.session.transcript.new_activity(),
        0,
        "transient card absent from restored history is removed"
    );
    let inspected = app.composed.prepared_cards;
    app.expire(Duration::from_secs(1));
    draw(&mut app, 140);
    assert_eq!(app.composed.prepared_cards, inspected);
    commit(
        &mut app.session,
        &g,
        EnvelopePayload::RoomCleared(RoomCleared {
            epoch: 2,
            previous_through: None,
        }),
    );
    draw(&mut app, 140);
    assert_eq!(app.session.transcript.new_activity(), 0);
    assert_eq!(
        app.composed.transcript.len(),
        0,
        "clear invalidates even a same-size replacement"
    );
    app.session.restore(snapshot);
    draw(&mut app, 140);
    assert!(!app.composed.transcript.is_empty());
    assert_eq!(app.session.transcript.new_activity(), 0);
}
