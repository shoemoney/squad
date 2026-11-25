//! Startup capability resolution. Probing uses the TUI's input owner, never a second reader.
use super::{
    clock::{Clock, SystemClock},
    level::{FxLevel, InvalidFxLevel},
};
use ratatui::layout::Size;
use std::{
    io::{self, Write},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTier {
    TrueColor,
    Indexed256,
    Ansi16,
    NoColor,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCaps {
    pub color: ColorTier,
    pub synchronized_output: bool,
    pub ssh: bool,
    pub tmux: bool,
    pub size: Size,
    pub level: FxLevel,
    pub debug: bool,
}
impl TerminalCaps {
    /// Pure resolver: tests need not mutate the process environment.
    pub fn resolve(
        env: impl Fn(&str) -> Option<String>,
        synchronized_output: bool,
        size: Size,
    ) -> Result<Self, InvalidFxLevel> {
        let term = env("TERM").unwrap_or_default().to_ascii_lowercase();
        let colorterm = env("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        let color = if env("NO_COLOR").is_some() || term == "dumb" {
            ColorTier::NoColor
        } else if colorterm == "truecolor" || colorterm == "24bit" || term.ends_with("-direct") {
            ColorTier::TrueColor
        } else if term.contains("256color") {
            ColorTier::Indexed256
        } else {
            ColorTier::Ansi16
        };
        let ssh = env("SSH_TTY").is_some();
        Ok(Self {
            color,
            synchronized_output,
            ssh,
            tmux: env("TMUX").is_some(),
            size,
            level: FxLevel::resolve(env("SQUAD_FX").as_deref(), ssh, synchronized_output)?,
            debug: env("SQUAD_FX_DEBUG").as_deref() == Some("1"),
        })
    }
    pub fn detect(synchronized_output: bool) -> Result<Self, InvalidFxLevel> {
        let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
        Self::resolve(
            |key| std::env::var(key).ok(),
            synchronized_output,
            Size::new(width, height),
        )
    }
    pub fn full_buffer_passes(&self) -> bool {
        self.synchronized_output && self.level == FxLevel::Full
    }
}

pub const SYNC_QUERY: &[u8] = b"\x1b[?2026$p";
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(100);
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncProbeResult {
    pub supported: bool,
    pub pending_input: Vec<u8>,
}

/// `read` is supplied by the terminal input owner and MUST return within its timeout.
/// Unrelated bytes (including keys typed during startup) are returned for normal handling.
/// Invoke in raw mode before starting crossterm's event decoder.
pub fn probe_sync_output(
    writer: &mut impl Write,
    read: impl FnMut(Duration) -> io::Result<Vec<u8>>,
) -> io::Result<SyncProbeResult> {
    probe_with_clock(writer, read, &SystemClock::default())
}
pub fn probe_with_clock(
    writer: &mut impl Write,
    mut read: impl FnMut(Duration) -> io::Result<Vec<u8>>,
    clock: &impl Clock,
) -> io::Result<SyncProbeResult> {
    writer.write_all(SYNC_QUERY)?;
    writer.flush()?;
    let start = clock.now();
    let mut input = Vec::new();
    loop {
        if let Some((start, end, status)) = find_response(&input) {
            input.drain(start..end);
            return Ok(SyncProbeResult {
                supported: matches!(status, b'1' | b'2'),
                pending_input: input,
            });
        }
        let remaining = PROBE_TIMEOUT.saturating_sub(clock.now().saturating_sub(start));
        if remaining.is_zero() {
            break;
        }
        match read(remaining) {
            Ok(bytes) if bytes.is_empty() => break,
            Ok(bytes) => input.extend(bytes),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(SyncProbeResult {
        supported: false,
        pending_input: input,
    })
}
fn find_response(input: &[u8]) -> Option<(usize, usize, u8)> {
    let prefix = b"\x1b[?2026;";
    input
        .windows(prefix.len() + 3)
        .enumerate()
        .find_map(|(i, bytes)| {
            (bytes.starts_with(prefix)
                && bytes[prefix.len() + 1..] == *b"$y"
                && matches!(bytes[prefix.len()], b'0'..=b'4'))
            .then_some((i, i + prefix.len() + 3, bytes[prefix.len()]))
        })
}
