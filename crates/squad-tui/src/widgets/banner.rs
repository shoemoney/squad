//! Static inline banners. Font selection and the width-keyed cache belong to core/fx.
use crate::fx::Palette;
pub use crate::fx::banner::BannerCache;
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_core::banner::{Banner, BannerMode, RenderedBanner};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct BannerView {
    pub kind: Banner,
    pub mode: BannerMode,
}
impl BannerView {
    pub fn new(kind: Banner, mode: BannerMode) -> Self {
        Self { kind, mode }
    }
    /// Recompute after a resize; callers can store this in their transcript row cache.
    pub fn measure(&self, width: u16, cache: &mut BannerCache) -> usize {
        cache.get(&self.kind.text(), self.mode, width).height()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BannerRects {
    pub area: Rect,
    pub height: usize,
}

pub fn render(view: &BannerView, area: Rect, buf: &mut Buffer, palette: &Palette) -> BannerRects {
    let art = squad_core::banner::render(&view.kind.text(), usize::from(area.width), view.mode);
    render_art(&art, area, buf, palette)
}
/// Use a cached core result for frame rendering, without regenerating FIGlet rows.
pub fn render_art(
    art: &RenderedBanner,
    area: Rect,
    buf: &mut Buffer,
    palette: &Palette,
) -> BannerRects {
    let mut canvas = Canvas::new(area, buf, 0);
    let rect = canvas.art(art, Style::default().fg(palette.chrome()));
    BannerRects {
        area: rect,
        height: art.height(),
    }
}

/// Shared widget-local flow layout. Logical row heights include off-screen content;
/// returned rectangles contain only visible cells and are safe effect targets.
pub(crate) struct Canvas<'a> {
    pub area: Rect,
    pub buf: &'a mut Buffer,
    pub row: usize,
    scroll: usize,
}
impl<'a> Canvas<'a> {
    pub fn new(area: Rect, buf: &'a mut Buffer, scroll: usize) -> Self {
        let area = area.intersection(buf.area);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].reset();
            }
        }
        Self {
            area,
            buf,
            row: 0,
            scroll,
        }
    }
    pub fn span(&self, start: usize, end: usize) -> Rect {
        let top = start
            .max(self.scroll)
            .min(self.scroll + usize::from(self.area.height));
        let bottom = end
            .max(top)
            .min(self.scroll + usize::from(self.area.height));
        Rect::new(
            self.area.x,
            self.area.y + (top - self.scroll) as u16,
            self.area.width,
            (bottom - top) as u16,
        )
    }
    pub fn line(&mut self, text: &str, style: Style) -> Rect {
        let start = self.row;
        if self.area.width == 0 {
            return self.span(start, start);
        }
        for line in flow_rows(text, usize::from(self.area.width)) {
            let rect = self.span(self.row, self.row + 1);
            if !rect.is_empty() {
                self.buf
                    .set_stringn(rect.x, rect.y, line, usize::from(rect.width), style);
            }
            self.row += 1;
        }
        self.span(start, self.row)
    }
    pub fn art(&mut self, art: &RenderedBanner, style: Style) -> Rect {
        let start = self.row;
        for line in &art.lines {
            let rect = self.span(self.row, self.row + 1);
            if !rect.is_empty() {
                self.buf
                    .set_stringn(rect.x, rect.y, line, usize::from(rect.width), style);
            }
            self.row += 1;
        }
        self.span(start, self.row)
    }
}

// Sanitize with the shared grapheme policy, then prefer word boundaries. Long
// identifiers still wrap by grapheme, so no identity or prose is discarded.
fn flow_rows(text: &str, width: usize) -> Vec<String> {
    let mut output = Vec::new();
    for source in crate::text::wrap(text, usize::MAX) {
        let text: String = source.cells.into_iter().map(|cell| cell.display).collect();
        let mut line = String::new();
        for word in text.split_word_bounds() {
            if !line.is_empty() && line.width() + word.width() > width {
                output.push(line.trim_end().to_owned());
                line.clear();
            }
            if line.is_empty() && word.chars().all(char::is_whitespace) {
                continue;
            }
            for grapheme in word.graphemes(true) {
                if !line.is_empty() && line.width() + grapheme.width() > width {
                    output.push(std::mem::take(&mut line));
                }
                line.push_str(grapheme);
            }
        }
        output.push(line.trim_end().to_owned());
    }
    output
}
