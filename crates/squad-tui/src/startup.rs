//! Immediate startup presentation. This owns input until the room is ready;
//! the regular TUI then takes the same terminal without a blank transition.
use crate::terminal::TerminalGuard;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect, text::Line, widgets::Paragraph};
use squad_protocol::config::SquadConfig;
use std::{
    future::Future,
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::UnboundedReceiver;

pub type StartupTerminal = Terminal<CrosstermBackend<io::Stdout>>;

#[derive(Clone, Debug)]
pub struct SeatProgress {
    pub nick: String,
    pub result: Result<String, String>,
}

struct StartupSeat {
    nick: String,
    configured: String,
    result: Option<Result<String, String>>,
}

pub struct StartupScreen {
    seats: Vec<StartupSeat>,
}
impl StartupScreen {
    pub fn new(config: &SquadConfig) -> Self {
        Self {
            seats: config
                .agents
                .iter()
                .map(|a| StartupSeat {
                    nick: a.nick.to_string(),
                    configured: a.model.clone(),
                    result: None,
                })
                .chain(std::iter::once(StartupSeat {
                    nick: "judge".into(),
                    configured: config.judge.model.clone(),
                    result: None,
                }))
                .collect(),
        }
    }
    pub fn update(&mut self, progress: SeatProgress) {
        if let Some(seat) = self
            .seats
            .iter_mut()
            .find(|seat| seat.nick == progress.nick)
        {
            seat.result = Some(progress.result);
        }
    }
    pub fn render(&self, area: Rect, buffer: &mut ratatui::buffer::Buffer, elapsed: Duration) {
        use ratatui::widgets::Widget;
        let mode = if usize::from(area.height) >= self.seats.len() + 10 {
            squad_core::banner::BannerMode::Compact
        } else {
            squad_core::banner::BannerMode::Off
        };
        let banner = squad_core::banner::render("SQUAD", usize::from(area.width), mode);
        let mut lines: Vec<Line<'_>> = banner.lines.into_iter().map(Line::from).collect();
        lines.extend([
            Line::from(""),
            Line::from(format!(
                " Resolving {} models…  {:.1}s",
                self.seats.len(),
                elapsed.as_secs_f32()
            )),
            Line::from(""),
        ]);
        for StartupSeat {
            nick,
            configured,
            result,
        } in &self.seats
        {
            lines.push(Line::from(match result {
                Some(Ok(slug)) => format!(" ✓ {nick} → {slug}"),
                Some(Err(reason)) => {
                    // Provider diagnostics contain no response text; keep the
                    // checklist readable even when the full reason is long.
                    let short: String = reason.chars().take(72).collect();
                    format!(" ✗ {nick} → {short}")
                }
                None => format!(" ⏳ {nick} → {configured}"),
            }));
        }
        let content = Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        Paragraph::new(lines).render(content, buffer);
        if area.height > 0 {
            Paragraph::new(" Ctrl+C to quit")
                .render(Rect::new(area.x, area.bottom() - 1, area.width, 1), buffer);
        }
    }
}

/// Terminal ownership begins synchronously, before the async worker pool and
/// before polling the room-opening future.
pub struct StartupSession {
    guard: TerminalGuard,
    terminal: StartupTerminal,
    screen: StartupScreen,
    started: Instant,
}
impl StartupSession {
    pub fn begin(config: &SquadConfig) -> io::Result<Self> {
        let guard = TerminalGuard::enter_startup()?;
        let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        let screen = StartupScreen::new(config);
        let started = Instant::now();
        terminal
            .draw(|frame| screen.render(frame.area(), frame.buffer_mut(), started.elapsed()))?;
        Ok(Self {
            guard,
            terminal,
            screen,
            started,
        })
    }
}

/// Draw before polling `opening`, including its synchronous DB initialization.
/// Dropping the future on interrupt cancels in-flight HTTP requests; admitted
/// attempts remain unknown in the existing durable accounting ledger.
pub async fn wait<T>(
    session: StartupSession,
    opening: impl Future<Output = T>,
    mut progress: UnboundedReceiver<SeatProgress>,
) -> io::Result<Option<(T, TerminalGuard, StartupTerminal)>> {
    let StartupSession {
        guard,
        mut terminal,
        mut screen,
        started,
    } = session;
    tokio::pin!(opening);
    let mut tick = tokio::time::interval(Duration::from_millis(30));
    let mut progress_open = true;
    loop {
        tokio::select! {
            biased;
            _ = tick.tick() => {
                for _ in 0..32 {
                    if !event::poll(Duration::ZERO)? { break; }
                    if let Event::Key(key) = event::read()?
                        && key.kind != KeyEventKind::Release
                        && key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL) {
                        return Ok(None);
                    }
                }
                terminal.draw(|frame| screen.render(frame.area(), frame.buffer_mut(), started.elapsed()))?;
            }
            update = progress.recv(), if progress_open => {
                match update { Some(update) => screen.update(update), None => progress_open = false }
                terminal.draw(|frame| screen.render(frame.area(), frame.buffer_mut(), started.elapsed()))?;
            }
            signal = tokio::signal::ctrl_c() => { signal?; return Ok(None); },
            result = &mut opening => return Ok(Some((result, guard, terminal))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_checklist() {
        let mut screen = StartupScreen {
            seats: vec![
                StartupSeat {
                    nick: "claude".into(),
                    configured: "~anthropic/latest".into(),
                    result: None,
                },
                StartupSeat {
                    nick: "openai".into(),
                    configured: "~openai/latest".into(),
                    result: None,
                },
                StartupSeat {
                    nick: "judge".into(),
                    configured: "~fixture/judge".into(),
                    result: None,
                },
            ],
        };
        screen.update(SeatProgress {
            nick: "openai".into(),
            result: Ok("openai/resolved".into()),
        });
        screen.update(SeatProgress {
            nick: "judge".into(),
            result: Err("Probe dispatch refused".into()),
        });
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(90, 18)).unwrap();
        terminal
            .draw(|frame| {
                screen.render(
                    frame.area(),
                    frame.buffer_mut(),
                    Duration::from_millis(1250),
                )
            })
            .unwrap();
        insta::assert_snapshot!(terminal.backend());
    }
}
