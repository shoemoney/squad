//! Real PTY transport and terminal restoration; the room is entirely scripted.
#[cfg(unix)]
#[test]
fn fake_room_terminal_input_and_restoration() {
    let result = std::process::Command::new("python3")
        .arg("-c")
        .arg(include_str!("m4b_pty_support/drive.py"))
        .arg(std::env::current_exe().unwrap())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
#[ignore = "PTY subprocess entry point, exercised by fake_room_terminal_input_and_restoration"]
async fn pty_child() {
    use crossterm::event::{self, Event};
    use ratatui::{Terminal, backend::CrosstermBackend};
    use squad_protocol::*;
    use squad_tui::{
        app::App,
        fx::TerminalCaps,
        session::{FakeRoom, RoomPort},
        terminal::TerminalGuard,
        ui,
    };
    use std::{
        io::{self, Write},
        time::Duration,
    };

    assert_eq!(std::env::var("SQUAD_PTY_CHILD").as_deref(), Ok("1"));
    let guard = TerminalGuard::enter_startup().unwrap();
    let mut app = App::new(
        TerminalCaps::resolve(
            |k| (k == "SQUAD_FX").then(|| "off".into()),
            false,
            (120, 40).into(),
        )
        .unwrap(),
    );
    app.tips_enabled = false;
    let room = FakeRoom::new();
    room.start_demo().await.unwrap();
    let state = room.state().await;
    let preflight = DebatePreflight {
        judge_only: false,
        debate_id: DebateId::new(),
        plan_id: PlanId::new(),
        expires_at: "2026-11-26T01:00:00Z".parse().unwrap(),
        estimate: DebateEstimate {
            estimated_usd: 3.,
            reserved_usd: 4.,
            remaining_usd: 20.,
            model_calls: 7,
        },
        judge_model: Some("fixture/judge".into()),
        warning: None,
        confirmation_required: true,
        debaters: ["claude".parse().unwrap(), "gemini".parse().unwrap()],
        rounds: 2,
        judge_requested: true,
        opening_round_required: false,
    };
    // The demo contains streamed turns and a committed verdict, with no network.
    assert!(!state.events.is_empty());
    app.session.restore(state);
    let streaming = FeedItem::Delta(ProducerEnvelope {
        base: EnvelopeBase {
            version: 1,
            id: EventId::new(),
            room_id: MainRoom::Main,
            correlation_id: EventId::new(),
            causation_id: None,
            sender: Actor::Member("claude".parse().unwrap()),
            recipients: vec![Recipient::Me],
            created_at: std::time::SystemTime::now().into(),
        },
        content: EnvelopePayload::Delta(Delta {
            generation: GenerationIdentity {
                message_id: MessageId::new(),
                work_id: WorkId::new(),
                trigger_event_id: EventId::new(),
                attempt: 1,
            },
            chunk_index: 0,
            text: "Streaming PTY evidence".into(),
            debate_id: None,
            round: None,
            case_id: None,
            contest_id: None,
            step_id: None,
        }),
    });
    room.emit(streaming.clone());
    app.session.reduce(
        streaming,
        squad_tui::fx::FeedOrigin::Live,
        Duration::ZERO,
        &mut squad_tui::fx::FxQueue::new(),
    );
    app.session.transcript.flush(Duration::from_millis(100));
    assert!(app.session.transcript.streaming());
    app.session.snapshot.context.active_contest = None;
    app.session.snapshot.context.pending_pick = None;
    app.session.pick = None;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout())).unwrap();
    let mut draw = |app: &mut App| {
        terminal
            .draw(|f| ui::render(app, f, Duration::ZERO))
            .unwrap();
    };
    draw(&mut app);
    let ready = |label: &str| {
        print!("\r\nPTY_READY_{label}\r\n");
        io::stdout().flush().unwrap();
    };
    let read = |app: &mut App| {
        assert!(event::poll(Duration::from_secs(5)).unwrap());
        app.handle(event::read().unwrap(), Duration::ZERO);
    };
    ready("paste");
    read(&mut app);
    assert_eq!(app.input.text(), "@claude literal\nsecond line");
    assert!(app.pending.is_none());
    ready("newline");
    read(&mut app); // Ctrl+J fallback
    assert!(app.input.text().ends_with('\n'));
    ready("altenter");
    read(&mut app);
    assert!(app.input.text().ends_with("\n\n"));
    let draft = app.input.text().to_owned();
    ready("search");
    read(&mut app); // Ctrl+F
    assert!(app.search_open);
    read(&mut app); // literal query
    assert_eq!(app.search.query, "a");
    read(&mut app); // Escape
    assert!(!app.search_open);
    assert_eq!(app.input.text(), draft);
    app.msg_focus = app
        .session
        .snapshot
        .context
        .records
        .iter()
        .find(|r| !r.content.is_empty())
        .map(|r| r.event_id);
    ready("copy");
    read(&mut app); // Ctrl+Y
    assert!(app.take_copy().is_some());
    app.session.snapshot.preflights = vec![PendingPreflight::Debate(preflight)];
    draw(&mut app);
    ready("preflight");
    read(&mut app); // Y
    assert!(matches!(app.pending, Some(Request::DebateConfirm { .. })));
    ready("resize");
    assert!(event::poll(Duration::from_secs(5)).unwrap());
    let resize = event::read().unwrap();
    assert!(matches!(resize, Event::Resize(80, 24)), "{resize:?}");
    app.handle(resize, Duration::ZERO);
    draw(&mut app);
    drop(terminal);
    guard.restore().unwrap();
    println!("PTY_COMPLETE");
}
