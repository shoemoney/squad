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

/// Shared redacted diagnostics for the F12 pane. Only counters and short codes are
/// accepted; free text is reduced to its error code so no prompt, response, key or
/// provider message can reach the screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    lines: Vec<String>,
}
impl Diagnostics {
    pub fn counter(&mut self, name: &str, value: u64) -> &mut Self {
        self.lines.push(format!("{} {value}", clean(name)));
        self
    }
    pub fn error(
        &mut self,
        code: &squad_protocol::ErrorCode,
        provider_status: Option<u16>,
    ) -> &mut Self {
        self.lines.push(match provider_status {
            Some(s) => format!("error {code:?} · provider {s}"),
            None => format!("error {code:?}"),
        });
        self
    }
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}
fn clean(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect()
}
/// Draw the diagnostics below the one-line tag, clipped to `area`.
pub fn render_diagnostics(diagnostics: &Diagnostics, area: Rect, buffer: &mut Buffer) {
    for (row, line) in diagnostics
        .lines()
        .iter()
        .enumerate()
        .take(usize::from(area.height))
    {
        buffer.set_stringn(
            area.x,
            area.y + row as u16,
            line,
            usize::from(area.width),
            Style::default(),
        );
    }
}
