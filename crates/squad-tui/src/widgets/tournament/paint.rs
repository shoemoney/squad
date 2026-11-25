//! Shared bounded static contest painter; geometry is in viewport coordinates.
use crate::fx::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

pub(crate) fn heading(palette: &Palette) -> Style {
    Style::default()
        .fg(palette.chrome())
        .add_modifier(Modifier::BOLD)
}

pub(crate) struct Painter<'a> {
    area: Rect,
    buf: &'a mut Buffer,
    row: usize,
    scroll: usize,
}
impl<'a> Painter<'a> {
    pub fn new(area: Rect, buf: &'a mut Buffer, scroll: u16) -> Self {
        let area = area.intersection(buf.area);
        buf.set_style(area, Style::reset());
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].set_symbol(" ");
            }
        }
        Self {
            area,
            buf,
            row: 0,
            scroll: usize::from(scroll),
        }
    }
    pub fn text(&mut self, text: &str, style: Style) -> Rect {
        let start = self.row;
        for line in wrapped(text, self.area.width) {
            if self.row >= self.scroll && self.row - self.scroll < usize::from(self.area.height) {
                let y = self.area.y + (self.row - self.scroll) as u16;
                self.buf
                    .set_stringn(self.area.x, y, line, usize::from(self.area.width), style);
            }
            self.row += 1;
        }
        let top = start.max(self.scroll);
        let bottom = self.row.min(self.scroll + usize::from(self.area.height));
        if top >= bottom {
            return Rect::default();
        }
        Rect::new(
            self.area.x,
            self.area.y + (top - self.scroll) as u16,
            self.area.width,
            (bottom - top) as u16,
        )
    }
}

// Word boundaries keep hints and prose readable; shared grapheme layout handles
// long tokens, tabs and terminal-control sanitization without splitting glyphs.
fn wrapped(text: &str, width: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;
    let width = usize::from(width).max(1);
    let mut lines = Vec::new();
    for source in crate::text::wrap(text, usize::MAX) {
        let clean = source
            .cells
            .iter()
            .map(|c| c.display.as_str())
            .collect::<String>();
        let mut line = String::new();
        for word in clean.split_whitespace() {
            if !line.is_empty() && line.width() + 1 + word.width() > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            if word.width() > width {
                let parts = crate::text::wrap(word, width);
                let count = parts.iter().filter(|r| !r.cells.is_empty()).count();
                for (i, part) in parts
                    .into_iter()
                    .filter(|r| !r.cells.is_empty())
                    .enumerate()
                {
                    let value = part
                        .cells
                        .iter()
                        .map(|c| c.display.as_str())
                        .collect::<String>();
                    if i + 1 == count {
                        line = value;
                    } else {
                        lines.push(value);
                    }
                }
            } else {
                line.push_str(word);
            }
        }
        lines.push(line);
    }
    lines
}
