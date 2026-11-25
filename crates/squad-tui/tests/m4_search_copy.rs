use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{app::App, fx::TerminalCaps, ui};
use std::time::Duration;

fn app() -> App {
    let caps = TerminalCaps::resolve(
        |k| (k == "SQUAD_FX").then(|| "off".into()),
        true,
        (100, 30).into(),
    )
    .unwrap();
    App::new(caps)
}
fn mid(n: usize) -> MessageId {
    format!("01900000-0000-7000-8000-{n:012x}").parse().unwrap()
}
fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent::new(code, modifiers))
}
fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle(key(KeyCode::Char(c), KeyModifiers::NONE), Duration::ZERO);
    }
}
fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
    let b = terminal.backend().buffer();
    (0..30)
        .map(|y| {
            (0..100)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn seeded() -> App {
    let mut app = app();
    for n in 1..=30 {
        let text = if n % 10 == 0 {
            format!("needle in message {n}")
        } else {
            format!("filler message {n}\n\nmore filler")
        };
        app.session.transcript.complete(mid(n), Actor::Me, 0, text);
    }
    draw(&mut app);
    app
}
fn record(seq: i64, text: &str) -> parser::RecordSummary {
    parser::RecordSummary {
        event_id: EventId::new(),
        message_id: Some(MessageId::new()),
        seq: Cursor::new(seq).unwrap(),
        kind: Text::new("message.final").unwrap(),
        author: Actor::Me,
        recipients: vec![],
        content: text.into(),
        in_reply_to_message_id: None,
        contest_id: None,
        case_id: None,
        plan_event_id: None,
        grid_event_id: None,
        cell_ids: vec![],
        state_version: None,
        superseded: false,
        eligible: true,
    }
}

#[test]
fn ctrl_f_searches_navigates_wraps_and_reveals_matches() {
    let mut app = seeded();
    app.handle(
        key(KeyCode::Char('f'), KeyModifiers::CONTROL),
        Duration::ZERO,
    );
    assert!(app.search_open);
    typed(&mut app, "needle");
    assert_eq!(app.search.matches.len(), 3);
    assert_eq!(app.search.current, Some(0));
    assert!(!app.session.transcript.follow, "reveal leaves live follow");
    let screen = draw(&mut app);
    assert!(screen.contains("Find: needle · 1/3"), "{screen}");
    assert!(screen.contains("needle in message 10"), "{screen}");
    app.handle(key(KeyCode::Enter, KeyModifiers::NONE), Duration::ZERO);
    assert_eq!(app.search.current, Some(1));
    assert!(draw(&mut app).contains("needle in message 20"));
    app.handle(key(KeyCode::Enter, KeyModifiers::NONE), Duration::ZERO);
    app.handle(key(KeyCode::Enter, KeyModifiers::NONE), Duration::ZERO);
    assert_eq!(app.search.current, Some(0), "next wraps");
    app.handle(key(KeyCode::Up, KeyModifiers::NONE), Duration::ZERO);
    assert_eq!(app.search.current, Some(2), "prev wraps");
    assert!(draw(&mut app).contains("needle in message 30"));
    // Edits refresh matches; Esc closes and clears.
    app.handle(key(KeyCode::Backspace, KeyModifiers::NONE), Duration::ZERO);
    assert_eq!(app.search.query, "needl");
    app.handle(key(KeyCode::Esc, KeyModifiers::NONE), Duration::ZERO);
    assert!(!app.search_open);
    assert!(app.search.query.is_empty());
    assert!(!draw(&mut app).contains("Find:"));
}

#[test]
fn search_ignores_streaming_text_and_typing_stays_out_of_the_draft() {
    let mut app = seeded();
    app.session
        .transcript
        .cards
        .get_mut(&mid(5))
        .unwrap()
        .complete = false;
    app.session.transcript.cards.get_mut(&mid(5)).unwrap().text = "needle streaming".into();
    app.handle(
        key(KeyCode::Char('f'), KeyModifiers::CONTROL),
        Duration::ZERO,
    );
    typed(&mut app, "needle");
    assert_eq!(app.search.matches.len(), 3);
    assert!(app.input.text().is_empty());
}

#[test]
fn ctrl_y_copies_focused_and_selected_messages_with_unconfirmed_feedback() {
    let mut app = app();
    let (a, b) = (record(1, "first\x1b[31m body"), record(2, "second body"));
    app.session.snapshot.context.records = vec![b, a];
    app.handle(
        key(KeyCode::Char('y'), KeyModifiers::CONTROL),
        Duration::ZERO,
    );
    assert!(app.pending_copy.is_none());
    assert!(app.notice.as_deref().unwrap().contains("Nothing selected"));
    app.handle(key(KeyCode::F(6), KeyModifiers::NONE), Duration::ZERO);
    app.handle(key(KeyCode::Down, KeyModifiers::NONE), Duration::ZERO);
    app.handle(
        key(KeyCode::Char('y'), KeyModifiers::CONTROL),
        Duration::ZERO,
    );
    let request = app.take_copy().expect("copy request");
    assert!(request.sequence.starts_with("\x1b]52;c;"));
    assert!(request.sequence.ends_with('\x07'));
    assert!(app.notice.as_deref().unwrap().contains("not confirmed"));
    assert!(app.take_copy().is_none());
    app.handle(key(KeyCode::Char(' '), KeyModifiers::NONE), Duration::ZERO);
    app.handle(key(KeyCode::Down, KeyModifiers::NONE), Duration::ZERO);
    app.handle(key(KeyCode::Char(' '), KeyModifiers::NONE), Duration::ZERO);
    app.handle(
        key(KeyCode::Char('y'), KeyModifiers::CONTROL),
        Duration::ZERO,
    );
    let both = app.take_copy().unwrap().sequence;
    assert!(both.len() > request.sequence.len(), "two messages copied");
}

#[test]
fn unavailable_clipboard_points_at_export() {
    let mut app = app();
    app.clipboard_available = false;
    app.session.snapshot.context.records = vec![record(1, "only")];
    app.handle(key(KeyCode::F(6), KeyModifiers::NONE), Duration::ZERO);
    app.handle(key(KeyCode::Down, KeyModifiers::NONE), Duration::ZERO);
    app.handle(
        key(KeyCode::Char('y'), KeyModifiers::CONTROL),
        Duration::ZERO,
    );
    assert!(app.pending_copy.is_none());
    assert!(app.notice.as_deref().unwrap().contains("/export"));
}
