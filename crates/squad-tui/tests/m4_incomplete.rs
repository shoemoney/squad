use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    transcript::{CardState, ReplyCard},
    ui,
};
use std::time::Duration;

fn app() -> App {
    let caps = TerminalCaps::resolve(
        |k| (k == "SQUAD_FX").then(|| "off".into()),
        false,
        (80, 24).into(),
    )
    .unwrap();
    App::new(caps)
}
fn mid(n: usize) -> MessageId {
    format!("01900000-0000-7000-8000-{n:012x}").parse().unwrap()
}
fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
    let b = terminal.backend().buffer();
    (0..24)
        .map(|y| (0..80).map(|x| b[(x, y)].symbol()).collect::<String>().trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}
fn card(n: usize, author: &str, text: &str, terminal: Option<WorkStatus>) -> ReplyCard {
    ReplyCard {
        id: mid(n),
        work_id: None,
        terminal,
        author: Actor::Member(Nick::new(author).unwrap()),
        attempt: 1,
        text: text.into(),
        pending_text: String::new(),
        next_chunk: 0,
        complete: true,
    }
}

#[test]
fn card_state_distinguishes_streaming_incomplete_and_superseded() {
    let mut app = app();
    let t = &mut app.session.transcript;
    t.cards.insert(mid(1), card(1, "claude", "done", Some(WorkStatus::Completed)));
    t.cards.insert(mid(2), card(2, "grok", "part", Some(WorkStatus::Failed)));
    t.cards.insert(mid(3), card(3, "gemini", "part", Some(WorkStatus::Cancelled)));
    let mut live = card(4, "openai", "str", None);
    live.complete = false;
    t.cards.insert(mid(4), live);
    t.supersede(mid(1), mid(5));
    assert_eq!(t.state_of(mid(1)), Some(CardState::Superseded));
    assert_eq!(t.state_of(mid(2)), Some(CardState::Incomplete(false)));
    assert_eq!(t.state_of(mid(3)), Some(CardState::Incomplete(true)));
    assert_eq!(t.state_of(mid(4)), Some(CardState::Streaming));
    assert_eq!(t.state_of(mid(9)), None);
    t.clear();
    assert!(t.superseded.is_empty() && t.cards.is_empty());
}

#[test]
fn incomplete_and_superseded_replies_are_labelled_in_the_transcript() {
    let mut app = app();
    let t = &mut app.session.transcript;
    t.cards.insert(mid(1), card(1, "claude", "First attempt answer", Some(WorkStatus::Completed)));
    t.cards.insert(mid(2), card(2, "grok", "Half an ans", Some(WorkStatus::Failed)));
    t.cards.insert(mid(3), card(3, "gemini", "Stopped early", Some(WorkStatus::Cancelled)));
    t.cards.insert(mid(4), card(4, "claude", "Second attempt answer", Some(WorkStatus::Completed)));
    t.supersede(mid(1), mid(4));
    let screen = draw(&mut app);
    for needle in [
        "superseded",
        "Replaced by @claude",
        "incomplete · failed",
        "did not finish",
        "incomplete · cancelled",
        "cancelled before it finished",
        "Second attempt answer",
        "First attempt answer",
    ] {
        assert!(screen.contains(needle), "missing {needle}\n{screen}");
    }
    insta::assert_snapshot!("incomplete_and_superseded_80x24", screen);
}

#[test]
fn a_message_superseded_event_labels_the_old_reply() {
    let mut app = app();
    app.session
        .transcript
        .cards
        .insert(mid(1), card(1, "claude", "old", Some(WorkStatus::Completed)));
    let before = draw(&mut app);
    assert!(!before.contains("superseded"));
    app.session.transcript.cards.insert(mid(2), card(2, "claude", "new", Some(WorkStatus::Completed)));
    app.session.transcript.supersede(mid(1), mid(2));
    assert!(draw(&mut app).contains("Replaced by @claude"));
}
