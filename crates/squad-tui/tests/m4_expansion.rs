use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    ui,
    widgets::{
        grill::{Exchange, GrillView, PlanVersion, Turn, TurnContent},
        tournament::{Entry, Ranking, Round, TournamentView},
    },
};
use std::time::Duration;

fn app() -> App {
    let caps = TerminalCaps::resolve(
        |k| (k == "SQUAD_FX").then(|| "off".into()),
        true,
        (100, 40).into(),
    )
    .unwrap();
    App::new(caps)
}
fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    terminal
        .draw(|f| ui::render(app, f, Duration::ZERO))
        .unwrap();
    let b = terminal.backend().buffer();
    (0..40)
        .map(|y| (0..100).map(|x| b[(x, y)].symbol()).collect::<String>().trim_end().to_owned())
        .collect::<Vec<_>>()
        .join("\n")
}
fn f7(app: &mut App) {
    app.handle(
        Event::Key(KeyEvent::new(KeyCode::F(7), KeyModifiers::NONE)),
        Duration::ZERO,
    );
}
fn identity(id: ContestId) -> MessageId {
    id.to_string().parse().unwrap()
}

#[test]
fn tournament_rankings_expand_through_the_presentation_cache() {
    let mut app = app();
    let id = ContestId::new();
    let mut view = TournamentView::new("Which database?", vec![]);
    view.state = "RUNNING".into();
    view.rounds = vec![Round {
        number: 1,
        remaining: 4,
        expanded: false,
        replay: None,
        entries: vec![Entry {
            nick: "claude".into(),
            text: "Postgres, obviously".into(),
            status: "complete".into(),
        }],
        ranking: vec![Ranking {
            nick: "claude".into(),
            score: 9,
            critique: "Sharp and well argued".into(),
            eliminated: false,
        }],
    }];
    app.session.tournaments.insert(id, view);
    app.session.snapshot.events.clear();
    let collapsed = draw(&mut app);
    assert!(!collapsed.contains("Sharp and well argued"), "{collapsed}");
    let rows = app.composed.transcript.total_rows();
    f7(&mut app);
    let expanded = draw(&mut app);
    assert!(expanded.contains("Sharp and well argued"), "{expanded}");
    assert!(app.composed.transcript.total_rows() > rows);
    let key = &app.composed.transcript.message(identity(id)).unwrap().presentation;
    assert!(key.rankings_expanded && key.expanded);
    assert!(app.composed.anchor.is_some(), "toggle keeps an anchor");
    f7(&mut app);
    assert!(!draw(&mut app).contains("Sharp and well argued"));
    assert!(
        !app.composed
            .transcript
            .message(identity(id))
            .unwrap()
            .presentation
            .rankings_expanded
    );
}

#[test]
fn grill_plans_and_appendix_expand_through_the_presentation_cache() {
    let mut app = app();
    let id = ContestId::new();
    let mut view = GrillView::new("claude", vec!["grok".into()]);
    view.state = "RUNNING".into();
    view.turns = vec![Turn {
        round: 1,
        griller: "grok".into(),
        angle: "risk".into(),
        status: "complete".into(),
        content: TurnContent::Questions(vec![Exchange {
            question: "What breaks first?".into(),
            answer: "The cache".into(),
            plan_changes: "add eviction".into(),
        }]),
    }];
    view.plans = vec![
        PlanVersion { version: 1, text: "OLD PLAN TEXT".into(), expanded: None },
        PlanVersion { version: 2, text: "NEW PLAN TEXT".into(), expanded: None },
    ];
    app.session.grills.insert(id, view);
    let collapsed = draw(&mut app);
    assert!(collapsed.contains("NEW PLAN TEXT"), "{collapsed}");
    assert!(!collapsed.contains("OLD PLAN TEXT"));
    f7(&mut app);
    let expanded = draw(&mut app);
    assert!(expanded.contains("OLD PLAN TEXT"), "{expanded}");
    let key = &app.composed.transcript.message(identity(id)).unwrap().presentation;
    assert!(key.appendix_expanded && key.rankings_expanded);
}
