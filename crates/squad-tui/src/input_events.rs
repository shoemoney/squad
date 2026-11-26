//! Crossterm event translation. Release events never edit; repeated Enter never
//! submits. Paste is one literal insertion, never recursively parsed as keys.
use crate::widgets::input::EditorAction;
use crossterm::{
    event::{
        DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Default)]
pub struct InputEvents {
    pub kitty: bool,
}
impl InputEvents {
    pub fn translate(&self, event: Event) -> Option<EditorAction> {
        if let Event::Paste(text) = event {
            return Some(EditorAction::Insert(text));
        }
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Enter if alt || (shift && self.kitty) => {
                Some(EditorAction::Insert("\n".into()))
            }
            KeyCode::Char('j') if control => Some(EditorAction::Insert("\n".into())),
            KeyCode::Enter if key.kind == KeyEventKind::Press => Some(EditorAction::Submit),
            KeyCode::Enter => None,
            KeyCode::Char('a') if control => Some(EditorAction::SelectAll),
            KeyCode::Up if alt => Some(EditorAction::HistoryPrevious),
            KeyCode::Down if alt => Some(EditorAction::HistoryNext),
            KeyCode::Left => Some(EditorAction::Left(shift)),
            KeyCode::Right => Some(EditorAction::Right(shift)),
            KeyCode::Up => Some(EditorAction::Up(shift)),
            KeyCode::Down => Some(EditorAction::Down(shift)),
            KeyCode::Home => Some(EditorAction::Home(shift)),
            KeyCode::End => Some(EditorAction::End(shift)),
            KeyCode::Backspace => Some(EditorAction::Backspace),
            KeyCode::Delete => Some(EditorAction::Delete),
            KeyCode::Char(c) if !control && !alt => Some(EditorAction::Insert(c.to_string())),
            _ => None,
        }
    }
}

/// Own the keyboard stack entry and bracketed-paste mode for a terminal session.
/// Create after raw-mode setup, before the event reader starts. Crossterm's
/// capability query uses a two-second poll on Unix; errors use fallbacks.
/// Drop before terminal restoration. The writer must be the terminal writer.
pub struct KeyboardSession<W: Write> {
    writer: W,
    pub events: InputEvents,
    keyboard_pushed: bool,
    paste_enabled: bool,
}
impl<W: Write> KeyboardSession<W> {
    pub fn negotiate(writer: W) -> io::Result<Self> {
        let kitty = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
        Self::with_capability(writer, kitty)
    }
    /// Inject an already-probed capability (also allows deterministic tests).
    pub fn with_capability(mut writer: W, kitty: bool) -> io::Result<Self> {
        execute!(writer, EnableBracketedPaste)?;
        if kitty
            && let Err(error) = execute!(
                writer,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )
        {
            let _ = execute!(writer, DisableBracketedPaste);
            return Err(error);
        }
        Ok(Self {
            writer,
            events: InputEvents { kitty },
            keyboard_pushed: kitty,
            paste_enabled: true,
        })
    }
    pub fn restore(&mut self) -> io::Result<()> {
        if self.keyboard_pushed {
            execute!(self.writer, PopKeyboardEnhancementFlags)?;
            self.keyboard_pushed = false;
        }
        if self.paste_enabled {
            execute!(self.writer, DisableBracketedPaste)?;
            self.paste_enabled = false;
        }
        Ok(())
    }
}
impl<W: Write> Drop for KeyboardSession<W> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
