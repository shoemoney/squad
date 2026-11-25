//! Full application rehearsal: cargo run -p squad-tui --example demo
use squad_tui::{
    app::App, event_loop, fx::TerminalCaps, input_events::InputEvents, session::FakeRoom,
    terminal::TerminalGuard,
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let guard = TerminalGuard::enter()?;
    let caps = TerminalCaps::detect(guard.capabilities().synchronized_output)?;
    let mut app = App::new(caps);
    app.input_events = InputEvents {
        kitty: guard.capabilities().keyboard_enhancement,
    };
    // Demo starts in transcript focus, where q quits; F6 focuses the editor.
    app.focus_input = false;
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;
    let (tx, mut input) = mpsc::channel(128);
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let reader = std::thread::spawn(move || {
        while !reader_stop.load(Ordering::Relaxed) {
            match crossterm::event::poll(Duration::from_millis(30)) {
                Ok(true) => {
                    if tx.blocking_send(crossterm::event::read()).is_err() {
                        break;
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    break;
                }
            }
        }
    });
    let room = Arc::new(FakeRoom::new());
    let script = room.start_demo();
    let result = event_loop::run(&mut app, room, &mut terminal, &mut input).await;
    stop.store(true, Ordering::Relaxed);
    drop(input);
    let _ = reader.join();
    script.abort();
    let _ = script.await;
    drop(terminal);
    guard.restore()?;
    result
}
