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
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};
use squad_tui::effects::verdict::{Contender, Criterion, Level, Verdict, VerdictReveal};
use std::{
    io,
    time::{Duration, Instant},
};

fn canned() -> Verdict {
    Verdict {
        contenders: [
            Contender {
                nick: "claude".into(),
                hue: Color::Rgb(224, 130, 90),
                art: vec![
                    "  ____ _      _   _   _ ____  _____".into(),
                    " / ___| |    / \\ | | | |  _ \\| ____|".into(),
                    "| |___| |___/ _ \\| |_| | | | |  _|".into(),
                    " \\____|_____/_/ \\_\\___/|____/|_____|".into(),
                ],
                compact_art: vec![
                    "▛▀▘ ▌ ▄▀▄ ▌ ▌ ▛▀▄ ▛▀▘".into(),
                    "▙▄▖ ▙▄ ▌ ▌ ▙▄▌ ▙▄▀ ▙▄▖".into(),
                ],
            },
            Contender {
                nick: "gemini".into(),
                hue: Color::Rgb(91, 155, 255),
                art: vec!["G E M I N I".into()],
                compact_art: vec!["GEMINI".into()],
            },
        ],
        criteria: std::array::from_fn(|i| Criterion {
            name: ["fit", "feasibility", "originality", "survivedAttacks"][i].into(),
            scores: [[8, 7], [8, 9], [9, 7], [9, 8]][i],
        }),
        scale: 10,
        totals: [85, 78],
        winner: 0,
        round_tally: [2, 1],
        override_reason: None,
        reasoning: "A focused plan, with a credible path through the hardest objections.".into(),
        steal: "Gemini's staged rollout and clear rollback criteria.".into(),
    }
}

fn run() -> io::Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let data = canned();
    let mut start = Instant::now();
    let mut reveal = VerdictReveal::new(Level::Full, false, true, 42);
    loop {
        let elapsed = start.elapsed();
        terminal.draw(|f| {
            let area = f.area();
            f.render_widget(Block::default().borders(Borders::ALL).title(" SQUAD · broadcast night · q quit · r replay ").style(Style::default().fg(Color::Rgb(255,210,63))), area);
            let inner = Rect::new(area.x + 2, area.y + 2, area.width.saturating_sub(4), area.height.saturating_sub(4));
            if elapsed < Duration::from_millis(1200) {
                let n = (elapsed.as_millis() / 100).min(5) as usize;
                let logo: String = "SQUAD".chars().enumerate().map(|(i,c)| if i < n { c } else { '▓' }).collect();
                f.render_widget(Paragraph::new(format!("\n\n        {logo}\n\n        SIX MINDS. ONE ROOM.")).style(Style::default().add_modifier(Modifier::BOLD)), inner);
            } else if elapsed < Duration::from_secs(12) {
                let debate_ms = elapsed.as_millis() - 1200;
                let round = (debate_ms / 3600 + 1).min(3);
                let final_label = if round == 3 { "FINAL ROUND".into() } else { format!("ROUND {round}") };
                let body = format!("{final_label}\n\nclaude  W4 L2                  VS                  gemini  W3 L1\n\nTopic: How should we ship the smallest useful version?\n\nclaude: Start with one complete workflow and measure the result.\n\ngemini: Put the risky dependency behind a staged rollout.\n\nJudge · fit / feasibility / originality / survivedAttacks\n\nRound {round}: {} : {}\n\n{}", 8 + round % 2, 7 + round % 3, if debate_ms % 3600 < 1800 { "Arguments arriving ▌" } else { "Round committed · next round follows" });
                f.render_widget(Paragraph::new(body), inner);
            } else {
                let t = elapsed.saturating_sub(Duration::from_secs(12));
                reveal.render(f.buffer_mut(), area, inner, &[], t, &data);
            }
        })?;
        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            if elapsed >= Duration::from_secs(12) {
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
