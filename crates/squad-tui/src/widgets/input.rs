//! Grapheme editor independent of protocol payload types; submission yields a String.
use crate::text::{boundaries, byte_at_column, cursor_position, normalize_newlines, wrap};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{StatefulWidget, Widget},
};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorAction {
    Insert(String),
    Left(bool),
    Right(bool),
    Up(bool),
    Down(bool),
    Home(bool),
    End(bool),
    Backspace,
    Delete,
    SelectAll,
    HistoryPrevious,
    HistoryNext,
    Submit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputError {
    pub bytes: usize,
    pub max_bytes: usize,
}

/// Shared UTF-8 byte validation for typed text, paste, and submission.
pub fn validate_input(text: &str) -> Result<(), InputError> {
    let max_bytes = squad_protocol::constants::MAX_MESSAGE_BYTES;
    if text.len() > max_bytes {
        Err(InputError {
            bytes: text.len(),
            max_bytes,
        })
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct InputState {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
    history: Vec<String>,
    history_index: Option<usize>,
    draft: Option<(String, usize, Option<usize>)>,
    scroll: usize,
    width: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn selection(&self) -> Option<Range<usize>> {
        self.anchor
            .filter(|&a| a != self.cursor)
            .map(|a| a.min(self.cursor)..a.max(self.cursor))
    }
    pub fn desired_height(&self, width: u16) -> u16 {
        wrap(&self.text, usize::from(width.saturating_sub(2)).max(1))
            .len()
            .clamp(4, 8) as u16
    }
    fn move_to(&mut self, byte: usize, select: bool) {
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = byte;
    }
    fn replace(&mut self, range: Range<usize>, value: &str) -> Result<(), InputError> {
        let value = normalize_newlines(value);
        let mut next = self.text.clone();
        next.replace_range(range.clone(), &value);
        validate_input(&next)?;
        self.text = next;
        // Inserting a combining mark can join adjacent clusters. Snap forward.
        let target = range.start + value.len();
        self.cursor = boundaries(&self.text)
            .into_iter()
            .find(|&b| b >= target)
            .unwrap_or(self.text.len());
        self.anchor = None;
        Ok(())
    }
    pub fn apply(&mut self, action: EditorAction) -> Result<Option<String>, InputError> {
        let bounds = boundaries(&self.text);
        let previous = bounds
            .iter()
            .copied()
            .rev()
            .find(|&b| b < self.cursor)
            .unwrap_or(0);
        let next = bounds
            .iter()
            .copied()
            .find(|&b| b > self.cursor)
            .unwrap_or(self.text.len());
        match action {
            EditorAction::Insert(value) => {
                self.replace(self.selection().unwrap_or(self.cursor..self.cursor), &value)?
            }
            EditorAction::Backspace => {
                self.replace(self.selection().unwrap_or(previous..self.cursor), "")?
            }
            EditorAction::Delete => {
                self.replace(self.selection().unwrap_or(self.cursor..next), "")?
            }
            EditorAction::Left(select) => self.move_to(
                if !select {
                    self.selection().map_or(previous, |r| r.start)
                } else {
                    previous
                },
                select,
            ),
            EditorAction::Right(select) => self.move_to(
                if !select {
                    self.selection().map_or(next, |r| r.end)
                } else {
                    next
                },
                select,
            ),
            EditorAction::Home(select) => {
                let p = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1);
                self.move_to(p, select);
            }
            EditorAction::End(select) => {
                let p = self.text[self.cursor..]
                    .find('\n')
                    .map_or(self.text.len(), |i| self.cursor + i);
                self.move_to(p, select);
            }
            EditorAction::Up(select) | EditorAction::Down(select) => {
                let down = matches!(action, EditorAction::Down(_));
                let rows = wrap(&self.text, self.width.max(1));
                let (row, column) = cursor_position(&rows, self.cursor);
                let target = if down {
                    (row + 1).min(rows.len() - 1)
                } else {
                    row.saturating_sub(1)
                };
                self.move_to(byte_at_column(&rows[target], column), select);
            }
            EditorAction::SelectAll => {
                self.anchor = Some(0);
                self.cursor = self.text.len();
            }
            EditorAction::HistoryPrevious => {
                if self.history.is_empty() {
                    return Ok(None);
                }
                let index = match self.history_index {
                    Some(i) => i.saturating_sub(1),
                    None => {
                        self.draft = Some((self.text.clone(), self.cursor, self.anchor));
                        self.history.len() - 1
                    }
                };
                self.history_index = Some(index);
                self.text = self.history[index].clone();
                self.cursor = self.text.len();
                self.anchor = None;
            }
            EditorAction::HistoryNext => {
                if let Some(i) = self.history_index {
                    if i + 1 < self.history.len() {
                        self.history_index = Some(i + 1);
                        self.text = self.history[i + 1].clone();
                        self.cursor = self.text.len();
                        self.anchor = None;
                    } else {
                        self.history_index = None;
                        if let Some((text, cursor, anchor)) = self.draft.take() {
                            self.text = text;
                            self.cursor = cursor;
                            self.anchor = anchor;
                        }
                    }
                }
            }
            EditorAction::Submit => {
                validate_input(&self.text)?;
                if self.text.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(self.text.clone()));
            }
        }
        Ok(None)
    }
    /// Call only after the application accepts a human submission.
    pub fn accept_submission(&mut self) {
        if !self.text.is_empty() {
            self.history.push(std::mem::take(&mut self.text));
        }
        self.cursor = 0;
        self.anchor = None;
        self.history_index = None;
        self.draft = None;
        self.scroll = 0;
    }
    /// Undo `accept_submission` for a rejected draft: drop its history entry
    /// and put the text back with the cursor at the end.
    pub fn restore_submission(&mut self, draft: &str) {
        if self.history.last().is_some_and(|h| h == draft) {
            self.history.pop();
        }
        self.text = draft.to_string();
        self.cursor = self.text.len();
        self.anchor = None;
        self.history_index = None;
        self.draft = None;
        self.scroll = 0;
    }
    pub fn cursor_in(&mut self, area: Rect) -> Option<(u16, u16)> {
        if area.width <= 2 || area.height == 0 {
            return None;
        }
        self.width = usize::from(area.width - 2);
        let rows = wrap(&self.text, self.width);
        let (row, column) = cursor_position(&rows, self.cursor);
        let height = usize::from(area.height.min(8));
        let labels = rows.len() > height && height >= 3;
        let height = if labels { height - 2 } else { height };
        self.scroll = self.scroll.min(row);
        if row >= self.scroll + height {
            self.scroll = row + 1 - height;
        }
        self.scroll = self.scroll.min(rows.len().saturating_sub(height));
        Some((
            area.x + 2 + column.min(self.width - 1) as u16,
            area.y + u16::from(labels) + (row - self.scroll) as u16,
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InputWidget;
impl StatefulWidget for InputWidget {
    type State = InputState;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut InputState) {
        if state.cursor_in(area).is_none() {
            return;
        }
        ratatui::widgets::Clear.render(area, buf);
        let rows = wrap(&state.text, state.width);
        let selection = state.selection();
        let height = usize::from(area.height.min(8));
        let labels = rows.len() > height && height >= 3;
        let content_height = if labels { height - 2 } else { height };
        for (visible, row) in rows
            .iter()
            .skip(state.scroll)
            .take(content_height)
            .enumerate()
        {
            let y = area.y + u16::from(labels) + visible as u16;
            buf.set_stringn(
                area.x,
                y,
                if state.scroll + visible == 0 {
                    "> "
                } else {
                    "  "
                },
                2,
                Style::default(),
            );
            for cell in &row.cells {
                let style = if selection
                    .as_ref()
                    .is_some_and(|s| s.start < cell.source.end && cell.source.start < s.end)
                {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                if cell.column + cell.width <= state.width {
                    buf.set_stringn(
                        area.x + 2 + cell.column as u16,
                        y,
                        &cell.display,
                        cell.width,
                        style,
                    );
                }
            }
        }
        if state.text.is_empty() && area.width > 2 {
            buf.set_stringn(
                area.x + 2,
                area.y,
                "Ask the squad…",
                usize::from(area.width - 2),
                Style::default().add_modifier(Modifier::DIM),
            );
        }
        // Overflow labels occupy reserved rows and never cover text or the caret.
        for (show, label, y) in [
            (labels && state.scroll > 0, "more above", area.y),
            (
                labels && state.scroll + content_height < rows.len(),
                "more below",
                area.y + height as u16 - 1,
            ),
        ] {
            if show && area.width >= label.len() as u16 {
                buf.set_string(
                    area.right() - label.len() as u16,
                    y,
                    label,
                    Style::default().add_modifier(Modifier::DIM),
                );
            }
        }
    }
}
