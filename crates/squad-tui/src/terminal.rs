//! Terminal ownership and failure-safe restoration.
use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use std::{
    io::{self, IsTerminal, Write},
    sync::{
        Once,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub const CAPABILITY_TIMEOUT: Duration = Duration::from_millis(100);
pub const SYNC_QUERY: &[u8] = b"\x1b[?2026$p";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub keyboard_enhancement: bool,
    pub synchronized_output: bool,
}

/// Query DEC mode 2026 through an input owner that can return raw control
/// replies. `receive` must honor the supplied timeout and retain unrelated
/// input for the event loop. This must run before crossterm's event reader:
/// its parser does not expose DECRPM replies. Unknown/unsupported replies,
/// transport failures and silence all select ordinary diffed drawing.
pub fn query_synchronized_output<W: Write>(
    writer: &mut W,
    mut receive: impl FnMut(Duration) -> io::Result<Option<Vec<u8>>>,
) -> bool {
    if writer
        .write_all(SYNC_QUERY)
        .and_then(|()| writer.flush())
        .is_err()
    {
        return false;
    }
    let deadline = Instant::now() + CAPABILITY_TIMEOUT;
    let mut reply = Vec::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receive(remaining) {
            Ok(Some(bytes)) if !bytes.is_empty() => {
                reply.extend(bytes);
                if reply
                    .windows(11)
                    .any(|w| w == b"\x1b[?2026;1$y" || w == b"\x1b[?2026;2$y")
                {
                    return true;
                }
                if reply.len() > 1024 {
                    return false;
                }
            }
            _ => return false,
        }
    }
    false
}

const OWNED: u64 = 1;
const RAW: u64 = 2;
const MODES: u64 = 4;
const KEYBOARD: u64 = 8;
const RESTORING: u64 = 16;
static STATE: AtomicU64 = AtomicU64::new(0);
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
static PANIC_HOOK: Once = Once::new();

fn cleanup(owner: Option<u64>) -> io::Result<()> {
    let state = match STATE.try_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
        if state == 0 || state & RESTORING != 0 || owner.is_some_and(|id| state >> 8 != id) {
            None
        } else {
            Some((state & !255) | RESTORING)
        }
    }) {
        Ok(state) => state,
        Err(_) => return Ok(()),
    };
    let mut output = io::stdout();
    let mut error = None;
    // Each restoration is attempted even when an earlier command fails.
    macro_rules! restore {
        ($command:expr) => {
            if let Err(e) = execute!(output, $command) {
                error.get_or_insert(e);
            }
        };
    }
    if state & MODES != 0 {
        restore!(EndSynchronizedUpdate);
        if state & KEYBOARD != 0 {
            restore!(PopKeyboardEnhancementFlags);
        }
        restore!(DisableMouseCapture);
        restore!(DisableBracketedPaste);
        restore!(Show);
        restore!(LeaveAlternateScreen);
    }
    if state & RAW != 0
        && let Err(e) = terminal::disable_raw_mode()
    {
        error.get_or_insert(e);
    }
    STATE.store(0, Ordering::SeqCst);
    error.map_or(Ok(()), Err)
}

/// One terminal session per process. Drop restores modes on all return paths;
/// the chained panic hook restores them before the panic diagnostic is printed.
#[derive(Debug)]
pub struct TerminalGuard {
    session: u64,
    capabilities: TerminalCapabilities,
    keyboard_probed: bool,
}

impl TerminalGuard {
    /// Conservative startup when no raw-response transport is available.
    pub fn enter() -> io::Result<Self> {
        Self::enter_inner(None::<fn(Duration) -> io::Result<Option<Vec<u8>>>>, true)
    }

    pub fn enter_with_probe(
        receive: impl FnMut(Duration) -> io::Result<Option<Vec<u8>>>,
    ) -> io::Result<Self> {
        Self::enter_inner(Some(receive), true)
    }

    /// Immediate conservative setup: no blocking capability queries. The
    /// startup screen must be visible before any potentially slow work.
    pub fn enter_startup() -> io::Result<Self> {
        Self::enter_inner(None::<fn(Duration) -> io::Result<Option<Vec<u8>>>>, false)
    }

    fn enter_inner(
        receive: Option<impl FnMut(Duration) -> io::Result<Option<Vec<u8>>>>,
        probe_keyboard: bool,
    ) -> io::Result<Self> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Squad requires a terminal",
            ));
        }
        let session = NEXT_SESSION.fetch_add(1, Ordering::SeqCst);
        STATE
            .compare_exchange(
                0,
                (session << 8) | OWNED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "terminal session already active",
                )
            })?;
        PANIC_HOOK.call_once(|| {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = cleanup(None);
                previous(info);
            }));
        });
        let mut guard = Self {
            session,
            capabilities: TerminalCapabilities::default(),
            keyboard_probed: false,
        };
        terminal::enable_raw_mode()?;
        STATE.fetch_or(RAW | MODES, Ordering::SeqCst);
        let mut output = io::stdout();
        execute!(
            output,
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            Hide
        )?;
        guard.capabilities.synchronized_output =
            receive.is_some_and(|receive| query_synchronized_output(&mut output, receive));
        if probe_keyboard {
            guard.negotiate_keyboard()?;
        }
        Ok(guard)
    }

    /// Deferred until a frame is visible; run before the event reader starts.
    pub fn negotiate_keyboard(&mut self) -> io::Result<()> {
        if self.keyboard_probed {
            return Ok(());
        }
        self.keyboard_probed = true;
        self.capabilities.keyboard_enhancement =
            terminal::supports_keyboard_enhancement().unwrap_or(false);
        if self.capabilities.keyboard_enhancement {
            STATE.fetch_or(KEYBOARD, Ordering::SeqCst);
            execute!(
                io::stdout(),
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
        }
        Ok(())
    }

    pub fn capabilities(&self) -> TerminalCapabilities {
        self.capabilities
    }

    /// Report cleanup failures when explicitly leaving; Drop remains a fallback.
    pub fn restore(self) -> io::Result<()> {
        cleanup(Some(self.session))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = cleanup(Some(self.session));
    }
}

struct SynchronizedOutput<'a, W: Write> {
    writer: &'a mut W,
    enabled: bool,
}
impl<W: Write> SynchronizedOutput<'_, W> {
    fn finish(&mut self) -> io::Result<()> {
        if self.enabled {
            execute!(self.writer, EndSynchronizedUpdate)?;
            self.enabled = false;
        }
        Ok(())
    }
}
impl<W: Write> Drop for SynchronizedOutput<'_, W> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Bound a complete draw (including flush) with DEC 2026. End is attempted
/// even if begin/draw fails or the drawing closure unwinds.
pub fn synchronized_output<W: Write, T>(
    writer: &mut W,
    supported: bool,
    draw: impl FnOnce(&mut W) -> io::Result<T>,
) -> io::Result<T> {
    let mut frame = SynchronizedOutput {
        writer,
        enabled: supported,
    };
    if supported {
        execute!(frame.writer, BeginSynchronizedUpdate)?;
    }
    let result = draw(frame.writer);
    let end = frame.finish();
    match result {
        Err(error) => Err(error),
        Ok(value) => end.map(|()| value),
    }
}
