use ratatui::{Terminal, backend::TestBackend};
use squad_protocol::*;
use squad_tui::{
    app::App,
    fx::TerminalCaps,
    session::{FakeRoom, RoomPort},
    ui,
};
use std::time::Duration;

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::render(app, frame, Duration::ZERO))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    // UUIDs are presentation identities; keep their width but normalize entropy.
    let mut i = 0;
    while i + 36 <= text.len() {
        if text.is_char_boundary(i + 36) && text[i..i + 36].parse::<EventId>().is_ok() {
            text.replace_range(i..i + 36, "00000000-0000-7000-8000-000000000001");
            i += 36;
        } else {
            i += text[i..].chars().next().unwrap().len_utf8();
        }
    }
    text
}
#[tokio::test(start_paused = true)]
async fn composed_screens() {
    let room = FakeRoom::new();
    room.start_demo().await.unwrap();
    let snapshot = room.state().await;
    for (width, height) in [(80, 24), (140, 42)] {
        for stage in ["answers", "debate", "verdict", "pick"] {
            let caps = TerminalCaps::resolve(
                |key| match key {
                    "SQUAD_FX" => Some("off".into()),
                    "COLORTERM" => Some("truecolor".into()),
                    _ => None,
                },
                true,
                (width, height).into(),
            )
            .unwrap();
            let mut app = App::new(caps);
            let mut state = snapshot.clone();
            let end=match stage {
                "answers"=>state.events.iter().position(|e|matches!(e.content,EnvelopePayload::DebateState(_))).unwrap(),
                "debate"=>state.events.iter().position(|e|matches!(&e.content,EnvelopePayload::DebateState(s) if s.state==DebateStatus::Judging)).unwrap(),
                "verdict"=>state.events.iter().position(|e|matches!(e.content,EnvelopePayload::ContestCollecting(_))).unwrap(),
                _=>state.events.len(),
            };
            state.events.truncate(end);
            state.scoreboard = state
                .events
                .iter()
                .rev()
                .find_map(|e| {
                    if let EnvelopePayload::ScoresChanged(s) = &e.content {
                        Some(s.scores.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            state.trophies.clear();
            if let Some(b) = state.events.iter().rev().find_map(|e| {
                if let EnvelopePayload::BudgetState(b) = &e.content {
                    Some(b)
                } else {
                    None
                }
            }) {
                state.daily_spend = squad_tui::session::DailySpendView {
                    day: b.day.clone(),
                    spent_usd: b.spent_usd,
                    reserved_usd: b.reserved_usd,
                    limit_usd: b.limit_usd,
                    unknown_calls: b.unknown_calls,
                };
            }

            if stage != "pick" {
                state.context.active_contest = None;
                state.context.pending_pick = None;
            }
            app.session.restore(state);
            let output = screen(&mut app, width, height);
            insta::assert_snapshot!(format!("composed_{stage}_{width}x{height}"), output);
            assert!(app.fx.queue_mut().is_empty());
        }
    }
}

#[tokio::test(start_paused = true)]
async fn duplicate_finals_replay_and_inline_turns() {
    let room = FakeRoom::new();
    let started = tokio::time::Instant::now();
    room.start_demo().await.unwrap();
    let duration = started.elapsed();
    assert!(
        (Duration::from_secs(60)..=Duration::from_secs(90)).contains(&duration),
        "showcase duration {duration:?}"
    );
    let mut state = room.state().await;
    let duplicate = state
        .events
        .iter()
        .find(|e| matches!(e.content, EnvelopePayload::MessageFinal(_)))
        .unwrap()
        .clone();
    let mut alternate = duplicate.clone();
    alternate.base.id = EventId::new();
    state.events.push(alternate);
    let caps = TerminalCaps::resolve(
        |key| {
            if key == "SQUAD_FX" {
                Some("off".into())
            } else {
                None
            }
        },
        true,
        (140, 42).into(),
    )
    .unwrap();
    let mut app = App::new(caps);
    app.session.restore(state);
    screen(&mut app, 140, 42);
    assert_eq!(app.session.transcript.cards.len(), 16);
    // Six ordinary answers, the user question, one debate, one verdict, and grill answer.
    assert_eq!(app.composed.transcript.len(), 11);
    assert!(app.fx.queue_mut().is_empty());
    let before = app.session.transcript.cards.len();
    let mut q = squad_tui::fx::FxQueue::new();
    assert!(!app.session.reduce(
        FeedItem::Committed(duplicate),
        squad_tui::fx::FeedOrigin::Live,
        Duration::ZERO,
        &mut q
    ));
    assert_eq!(app.session.transcript.cards.len(), before);
    assert!(q.is_empty());
}
