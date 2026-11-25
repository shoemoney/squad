use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{app::App, fx::TerminalCaps, ui};
use std::time::Duration;

pub fn mid(n: usize) -> MessageId {
    format!("01900000-0000-7000-8000-{n:012x}").parse().unwrap()
}
pub fn app() -> App {
    let caps = TerminalCaps::resolve(
        |k| (k == "SQUAD_FX").then(|| "off".into()),
        true,
        (100, 30).into(),
    )
    .unwrap();
    App::new(caps)
}
pub fn draw(app: &mut App) -> String {
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
pub fn fill(app: &mut App, from: usize, to: usize) {
    for n in from..=to {
        app.session.transcript.complete(
            mid(n),
            Actor::Me,
            0,
            format!("message number {n}\n\nsecond paragraph {n}"),
        );
    }
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn wheel(kind: MouseEventKind) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column: 50,
        row: 10,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn page_up_suspends_following_and_counts_new_activity() {
    let mut app = app();
    fill(&mut app, 1, 40);
    draw(&mut app);
    assert!(app.session.transcript.follow);
    assert_eq!(app.session.transcript.new_activity(), 0);
    app.handle(key(KeyCode::PageUp), Duration::ZERO);
    assert!(!app.session.transcript.follow);
    let before = draw(&mut app);
    assert!(before.contains("↓ back to live"));
    assert!(!before.contains(" new ·"));
    fill(&mut app, 41, 43);
    let after = draw(&mut app);
    assert_eq!(app.session.transcript.new_activity(), 3);
    assert!(after.contains("↓ 3 new · back to live"), "{after}");
    // The viewport did not jump while new messages arrived below it.
    let anchor = app.composed.anchor;
    draw(&mut app);
    assert_eq!(app.composed.anchor, anchor);
}

#[test]
fn page_down_to_the_bottom_resumes_live_and_clears_the_count() {
    let mut app = app();
    fill(&mut app, 1, 40);
    draw(&mut app);
    app.handle(key(KeyCode::PageUp), Duration::ZERO);
    draw(&mut app);
    fill(&mut app, 41, 42);
    draw(&mut app);
    assert_eq!(app.session.transcript.new_activity(), 2);
    for _ in 0..40 {
        app.handle(key(KeyCode::PageDown), Duration::ZERO);
        draw(&mut app);
        if app.session.transcript.follow {
            break;
        }
    }
    assert!(app.session.transcript.follow);
    assert_eq!(app.session.transcript.new_activity(), 0);
    assert!(!draw(&mut app).contains("back to live"));
}

#[test]
fn pages_are_about_one_viewport_and_never_scroll_past_the_top() {
    let mut app = app();
    fill(&mut app, 1, 40);
    draw(&mut app);
    let height = app.composed.viewport.height as isize;
    let start = app.composed.top;
    app.handle(key(KeyCode::PageUp), Duration::ZERO);
    draw(&mut app);
    let moved = start as isize - app.composed.top as isize;
    // Source anchors snap to block boundaries, so allow a row or two of slack.
    assert!(
        (height - 3..=height).contains(&moved),
        "moved {moved} of {height}"
    );
    for _ in 0..60 {
        app.handle(key(KeyCode::PageUp), Duration::ZERO);
        draw(&mut app);
    }
    assert_eq!(app.composed.top, 0);
}

#[test]
fn mouse_wheel_scrolls_three_rows_and_wheel_down_to_bottom_resumes() {
    let mut app = app();
    fill(&mut app, 1, 40);
    draw(&mut app);
    let start = app.composed.top;
    app.handle(wheel(MouseEventKind::ScrollUp), Duration::ZERO);
    draw(&mut app);
    assert!(!app.session.transcript.follow);
    assert_eq!(app.composed.top, start - 3);
    fill(&mut app, 41, 41);
    draw(&mut app);
    assert_eq!(app.session.transcript.new_activity(), 1);
    for _ in 0..10 {
        app.handle(wheel(MouseEventKind::ScrollDown), Duration::ZERO);
        draw(&mut app);
    }
    assert!(app.session.transcript.follow);
    assert_eq!(app.session.transcript.new_activity(), 0);
}

#[test]
fn end_key_clears_activity() {
    let mut app = app();
    fill(&mut app, 1, 40);
    draw(&mut app);
    app.handle(key(KeyCode::PageUp), Duration::ZERO);
    fill(&mut app, 41, 42);
    draw(&mut app);
    assert_eq!(app.session.transcript.new_activity(), 2);
    app.handle(key(KeyCode::End), Duration::ZERO);
    draw(&mut app);
    assert_eq!(app.session.transcript.new_activity(), 0);
    assert!(app.session.transcript.follow);
}
