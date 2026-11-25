#![allow(dead_code)]
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use squad_protocol::EventId;
use squad_tui::fx::*;
use std::time::Duration;
use tachyonfx::{Interpolation, fx};

pub const AREA: Rect = Rect::new(0, 0, 40, 10);
pub const BODY: Rect = Rect::new(2, 3, 36, 3);
pub fn id(n: u32) -> EventId {
    format!("00000000-0000-7000-8000-{n:012x}").parse().unwrap()
}
pub fn cue(n: u32, at: u64) -> TimedCue {
    TimedCue {
        event_id: id(n),
        at: Duration::from_millis(at),
        cue: FxCue::DailySpendChanged,
    }
}
pub fn caps(level: FxLevel) -> TerminalCaps {
    TerminalCaps::resolve(
        |key| match key {
            "SQUAD_FX" => Some(
                match level {
                    FxLevel::Full => "full",
                    FxLevel::Lite => "lite",
                    FxLevel::Off => "off",
                }
                .into(),
            ),
            "COLORTERM" => Some("truecolor".into()),
            _ => None,
        },
        true,
        Size::new(40, 10),
    )
    .unwrap()
}
pub fn manager(level: FxLevel) -> (ManualClock, FxManager<ManualClock>) {
    let clock = ManualClock::default();
    let manager = FxManager::new(clock.clone(), caps(level));
    (clock, manager)
}
/// Static widget fixture with real Ratatui diffing, explicit chrome and protected body.
pub fn draw(manager: &mut FxManager<ManualClock>, clock: &ManualClock) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();
    let mut context = FxContext {
        protected: vec![BODY],
        ..Default::default()
    };
    manager.pre(&mut context);
    terminal.draw(|frame| {
        frame.render_widget(Block::default().borders(Borders::ALL).title("SQUAD"),AREA);
        frame.render_widget(Paragraph::new("claude · completed").style(Style::default().fg(Color::Yellow)),Rect::new(2,1,36,1));
        frame.render_widget(Paragraph::new("Readable body: café, 中, 👩‍💻\nEvidence and citations stay put.\n$3.42 / $20.00 (~pending)").style(Style::default().fg(Color::Reset)),BODY);
        manager.post(frame.buffer_mut(),AREA,clock.now());
    }).unwrap();
    terminal.backend().buffer().clone()
}
pub fn render_at(cues: &[TimedCue], t: Duration) -> Buffer {
    render_at_level(cues, t, FxLevel::Full)
}
pub fn render_at_level(cues: &[TimedCue], t: Duration, level: FxLevel) -> Buffer {
    let (clock, mut manager) = manager(level);
    let mut ordered: Vec<_> = cues.iter().filter(|cue| cue.at <= t).collect();
    ordered.sort_by_key(|cue| cue.at);
    for cue in ordered {
        clock.set(cue.at);
        manager.queue_mut().push(FeedOrigin::Live, cue.clone());
        manager.pre(&mut FxContext {
            protected: vec![BODY],
            ..Default::default()
        });
        // A representative catalogue pass; catalogue modules own the treatment.
        let key = FxKey::Custom("fixture-accent".into());
        manager.add_pass(
            key.clone(),
            Pass::new(
                key,
                fx::fade_from_fg(Color::DarkGray, (280, Interpolation::CubicOut)),
                AREA,
                PassAccess::Unprotected,
            ),
            false,
        );
    }
    clock.set(t);
    draw(&mut manager, &clock)
}
