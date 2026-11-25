//! Effects diagnostics are opt-in and contain no prompt or response data.
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
#[derive(Debug, Clone, Default)]
pub struct DebugView {
    pub tag: Option<String>,
    pub draws: u64,
    pub feed_items: u64,
}
pub fn render(view: &DebugView, area: Rect, buffer: &mut Buffer) {
    let tag = view.tag.as_deref().unwrap_or("Debug · Esc Close");
    buffer.set_stringn(
        area.x,
        area.y,
        format!("{tag} · draws {} · feed {}", view.draws, view.feed_items),
        usize::from(area.width),
        Style::default(),
    );
}
