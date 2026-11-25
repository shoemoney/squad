//! Pure startup and undersized-terminal presentation; no lifecycle side effects.
use crate::layout::{MIN_HEIGHT, MIN_WIDTH};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Clear, Paragraph, Widget, Wrap},
};
use std::path::Path;

fn paragraph(area: Rect, buffer: &mut Buffer, text: &str) {
    // Remove control characters from caller-provided paths/status labels.
    let safe: String = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    Paragraph::new(safe)
        .wrap(Wrap { trim: false })
        .render(area, buffer);
}

pub fn missing_key(area: Rect, buffer: &mut Buffer, env_path: &Path) {
    Clear.render(area, buffer);
    paragraph(
        area,
        buffer,
        &format!(
            "Squad — missing API key\n\nOPENROUTER_API_KEY is required. Add it to .env or the process environment.\n\nSelected .env: {}\nRun squad doctor to check configuration.\n\nCtrl+C / q: Exit",
            env_path.display()
        ),
    );
}

/// The caller restores the terminal and exits after presenting this diagnostic.
pub fn room_locked(area: Rect, buffer: &mut Buffer, pid: u32) {
    Clear.render(area, buffer);
    paragraph(
        area,
        buffer,
        &format!(
            "Room main is already open in another squad process (pid {pid}).\n\nCtrl+C / q: Exit"
        ),
    );
}

/// Spend is already formatted from authoritative state by the budget widget.
/// It is placed first, so even a one-row viewport retains accounting.
pub fn terminal_too_small(area: Rect, buffer: &mut Buffer, spend: &str) {
    Clear.render(area, buffer);
    paragraph(
        area,
        buffer,
        &format!(
            "{spend}\nTerminal too small: {}×{}. Required: {MIN_WIDTH}×{MIN_HEIGHT}.\nResize to continue. Your draft and room activity are preserved.\nCtrl+C: Exit",
            area.width, area.height
        ),
    );
}
