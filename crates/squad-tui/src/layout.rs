//! Pure viewport allocation. Rectangles include widget chrome.
use ratatui::layout::Rect;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewport {
    Wide,
    Narrow,
    TooSmall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportLayout {
    pub viewport: Viewport,
    pub status: Rect,
    /// Always-visible accounting header (also the compact fallback).
    pub spend: Rect,
    pub participants: Rect,
    pub divider: Rect,
    pub transcript: Rect,
    pub input: Rect,
    pub help: Rect,
}

/// Allocate a four-row draft; use `with_input_rows` after measuring the editor.
pub fn layout(area: Rect) -> ViewportLayout {
    with_input_rows(area, 4)
}

/// Input growth takes space only from the transcript and participant rail.
/// At undersized dimensions all interactive regions are empty; callers render
/// the too-small screen and retain their existing interaction state.
pub fn with_input_rows(area: Rect, input_rows: u16) -> ViewportLayout {
    let empty = Rect::new(area.x, area.y, 0, 0);
    let row = |offset, height| Rect::new(area.x, area.y + offset, area.width, height);
    let mut result = ViewportLayout {
        viewport: Viewport::TooSmall,
        status: row(0, u16::from(area.height > 0)),
        spend: row(u16::from(area.height > 0), u16::from(area.height > 1)),
        participants: empty,
        divider: empty,
        transcript: empty,
        input: empty,
        help: empty,
    };
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return result;
    }
    let rail = if area.width >= 120 { 20 } else { 16 };
    result.viewport = if area.width >= 120 {
        Viewport::Wide
    } else {
        Viewport::Narrow
    };
    let input_rows = input_rows.clamp(4, 8);
    let body_height = area.height - 3 - input_rows;
    result.participants = Rect::new(area.x, area.y + 2, rail, body_height);
    result.divider = Rect::new(area.x + rail, area.y + 2, 1, body_height);
    result.transcript = Rect::new(
        area.x + rail + 1,
        area.y + 2,
        area.width - rail - 1,
        body_height,
    );
    result.input = row(area.height - 1 - input_rows, input_rows);
    result.help = row(area.height - 1, 1);
    result
}

/// Reserve an independently scrolling rail's last row for the spend widget
/// only when the caller's measured participant content leaves room for it.
/// Otherwise the accounting header remains the spend widget's destination.
pub fn spend_destination(layout: &ViewportLayout, participant_rows: u16) -> Rect {
    let rail = layout.participants;
    if rail.height > participant_rows {
        Rect::new(rail.x, rail.bottom() - 1, rail.width, 1)
    } else {
        layout.spend
    }
}
