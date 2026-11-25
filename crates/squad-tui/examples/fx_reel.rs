//! Record with: cargo run -p squad-tui --example fx_reel
//! Canned data only. Space/any key skips the verdict; q quits, r restarts.
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders},
};
use squad_tui::fx::verdict::{Level, VerdictReveal};
use std::{
    io,
    time::{Duration, Instant},
};

#[path = "fx_reel/catalogue.rs"]
mod catalogue;

fn run() -> io::Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut start = Instant::now();
    let mut reveal = VerdictReveal::new(Level::Full, false, true, 42);
    loop {
        let elapsed = start.elapsed();
        terminal.draw(|f| {
            let area = f.area();
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" SQUAD · broadcast night · q quit · r replay ")
                    .style(Style::default().fg(Color::Rgb(255, 210, 63))),
                area,
            );
            let inner = Rect::new(
                area.x + 2,
                area.y + 2,
                area.width.saturating_sub(4),
                area.height.saturating_sub(4),
            );
            catalogue::showcase(f.buffer_mut(), inner, elapsed, &reveal);
        })?;
        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            if elapsed >= Duration::from_millis(17700) {
                let _ = reveal.skip_on_key(key);
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('r') => {
                    start = Instant::now();
                    reveal = VerdictReveal::new(Level::Full, false, true, 42);
                }
                _ if elapsed < Duration::from_millis(1200) => {
                    start = Instant::now() - Duration::from_millis(1200);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
        disable_raw_mode()?;
        return Err(error);
    }
    let result = run();
    let screen_result = execute!(io::stdout(), LeaveAlternateScreen);
    let raw_result = disable_raw_mode();
    result.and(screen_result).and(raw_result)
}
