//! Prepared message caches and a visible-row-only transcript painter.
//!
//! Reducers prepare/reflow outside drawing. Token batching and durable paging
//! belong to `crate::transcript`; this widget never reads a feed or a clock.
use crate::{
    fx::{ColorTier, Palette},
    markdown::{MarkdownRenderer, sanitize, wrap_lines},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Widget,
};
use squad_protocol::{MessageId, UserMessage};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Changing any presentation choice invalidates the associated wrapped height.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresentationKey {
    pub revision: u64,
    pub expanded: bool,
    pub scorecard_expanded: bool,
    pub rankings_expanded: bool,
    pub appendix_expanded: bool,
    pub pick_revision: u64,
    pub banner_mode: u8,
    pub font_fallback: bool,
    pub markdown_style: u64,
    pub report_revision: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptMessage {
    pub id: MessageId,
    pub author: String,
    pub human: bool,
    pub version: String,
    pub slot: usize,
    pub source: String,
    pub attempt: u32,
    pub complete: bool,
    pub status: String,
    pub quotes: Vec<String>,
    pub presentation: PresentationKey,
    /// Prepared static banner, scorecard, or collapsed-report rows. Their full
    /// wrapped height participates in the same lookup and anchor calculations.
    pub before_body: Vec<Line<'static>>,
    /// Precomposed widget rows, already measured at the transcript body width.
    pub widget_rows: Vec<Line<'static>>,
    pub after_body: Vec<Line<'static>>,
}
impl TranscriptMessage {
    pub fn new(id: MessageId, author: impl Into<String>, source: impl Into<String>) -> Self {
        let author = author.into();
        let human = author == "me";
        Self {
            id,
            author,
            human,
            source: source.into(),
            version: String::new(),
            slot: 0,
            attempt: 1,
            complete: true,
            status: String::new(),
            quotes: vec![],
            presentation: PresentationKey::default(),
            before_body: vec![],
            widget_rows: vec![],
            after_body: vec![],
        }
    }
    pub fn from_user(message: &UserMessage, human_name: &str) -> Self {
        let mut view = Self::new(message.input.message_id, human_name, &message.body);
        view.human = true;
        view.quotes = message
            .resolved_quotes
            .iter()
            .map(|q| format!("{}: {}", q.author, q.text))
            .collect();
        view
    }
}
struct AnchorRows {
    source: String,
    width: usize,
    complete: bool,
    graphemes: usize,
    rows: Vec<usize>,
}
struct CachedMessage {
    model: TranscriptMessage,
    markdown: MarkdownRenderer,
    prefix: Vec<Line<'static>>,
    suffix: Vec<Line<'static>>,
    source_rows: Vec<usize>,
    anchor_cache: Vec<AnchorRows>,
    height: usize,
}
impl CachedMessage {
    fn prepare(&mut self, width: usize) {
        self.markdown.update(
            &format!("{}:{}", self.model.id, self.model.attempt),
            &self.model.source,
            self.model.complete,
            width,
        );
        let mut prefix = Vec::new();
        for quote in &self.model.quotes {
            prefix.extend(wrap_lines(
                &[Line::raw(format!("> {}", sanitize(quote)))],
                width,
            ));
        }
        prefix.extend(wrap_lines(&self.model.before_body, width));
        prefix.extend(self.model.widget_rows.clone());
        self.prefix = prefix;
        self.suffix = wrap_lines(&self.model.after_body, width);
        // Source grapheme anchors use block-local wrapping rather than global
        // row ratios, so append-only tails cannot shift earlier block anchors.
        self.source_rows.clear();
        let source = self.markdown.source();
        let mut base = 0;
        let mut previous_end = 0;
        for (index, block) in self.markdown.blocks().iter().enumerate() {
            let start = block.source_range.start;
            let end = block.source_range.end;
            base += source[previous_end..start].graphemes(true).count();
            previous_end = end;
            let raw = &source[start..end];
            let reusable = self.anchor_cache.get(index).is_some_and(|cached| {
                cached.source == raw
                    && cached.width == width
                    && cached.complete == self.model.complete
                    && cached.rows.len() == block.row_count()
            });
            if !reusable {
                let (mut rows, graphemes) = anchor_source_rows(raw, width.saturating_sub(4).max(1));
                // Use the already parsed/styled block's actual row count. A second
                // Markdown renderer per block repeated fence highlighting and
                // could disagree with completion's reference reconciliation.
                rows.resize(block.row_count(), graphemes);
                let cache = AnchorRows {
                    source: raw.to_owned(),
                    width,
                    complete: self.model.complete,
                    graphemes,
                    rows,
                };
                if index < self.anchor_cache.len() {
                    self.anchor_cache[index] = cache;
                } else {
                    self.anchor_cache.push(cache);
                }
            }
            let cached = &self.anchor_cache[index];
            self.source_rows
                .extend(cached.rows.iter().map(|row| base + row));
            base += cached.graphemes;
        }
        self.anchor_cache.truncate(self.markdown.blocks().len());
        base += source[previous_end..].graphemes(true).count();
        self.source_rows.resize(self.markdown.row_count(), base);
        self.height = self.header_rows()
            + self.prefix.len()
            + self.markdown.row_count()
            + self.suffix.len()
            + 1;
    }
    fn header_rows(&self) -> usize {
        usize::from(self.model.widget_rows.is_empty())
    }
    fn body_start(&self) -> usize {
        self.header_rows() + self.prefix.len()
    }
}
// The same greedy source-row policy as text::wrap, retaining only canonical
// grapheme offsets. Display strings/cell vectors are unnecessary for anchors.
fn anchor_source_rows(source: &str, width: usize) -> (Vec<usize>, usize) {
    let width = width.max(1);
    let mut rows = vec![0];
    let mut column = 0;
    let mut count = 0;
    for (index, grapheme) in source.graphemes(true).enumerate() {
        count = index + 1;
        if grapheme == "\n" || grapheme == "\r\n" {
            rows.push(count);
            column = 0;
            continue;
        }
        let control = grapheme != "\t" && grapheme.chars().any(char::is_control);
        let measured = if control { 1 } else { grapheme.width() };
        let mut cells = if grapheme == "\t" {
            4 - column % 4
        } else {
            measured
        };
        if column > 0 && column + cells > width {
            rows.push(index);
            column = 0;
            cells = if grapheme == "\t" { 4 } else { measured };
        }
        column += cells;
    }
    if column >= width {
        rows.push(count);
    }
    (rows, count)
}

/// Top-of-viewport identity and original-source grapheme, never a terminal byte/cell offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptAnchor {
    pub id: MessageId,
    pub source_grapheme: usize,
    pub row_in_header: Option<usize>,
}
#[derive(Default)]
pub struct TranscriptView {
    messages: Vec<CachedMessage>,
    indexes: HashMap<MessageId, usize>,
    heights: Vec<usize>,
    width: u16,
}
impl TranscriptView {
    pub fn new(width: u16) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }
    pub fn len(&self) -> usize {
        self.messages.len()
    }
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
    pub fn total_rows(&self) -> usize {
        self.prefix(self.messages.len())
    }
    pub fn cache_stats(&self, id: MessageId) -> Option<crate::markdown::CacheStats> {
        self.indexes
            .get(&id)
            .map(|i| self.messages[*i].markdown.stats())
    }
    pub fn message(&self, id: MessageId) -> Option<&TranscriptMessage> {
        self.indexes.get(&id).map(|i| &self.messages[*i].model)
    }
    /// Update in place: concurrent streams cannot reorder reserved cards. Call
    /// once per reducer batch; terminal outcomes may call immediately.
    pub fn upsert(&mut self, model: TranscriptMessage) {
        if let Some(&i) = self.indexes.get(&model.id) {
            if self.messages[i].model == model {
                return;
            }
            let old_height = self.messages[i].height;
            self.messages[i].model = model;
            self.messages[i].prepare(usize::from(self.width.saturating_sub(1)));
            let new_height = self.messages[i].height;
            let mut node = i + 1;
            while node < self.heights.len() {
                self.heights[node] = self.heights[node] - old_height + new_height;
                node += node.isolate_lowest_one();
            }
        } else {
            let i = self.messages.len();
            self.indexes.insert(model.id, i);
            let mut cached = CachedMessage {
                model,
                markdown: MarkdownRenderer::new(),
                prefix: vec![],
                suffix: vec![],
                source_rows: vec![],
                anchor_cache: vec![],
                height: 0,
            };
            cached.prepare(usize::from(self.width.saturating_sub(1)));
            self.messages.push(cached);
            // Append one Fenwick node, including the preceding covered range.
            if self.heights.is_empty() {
                self.heights.push(0);
            }
            let node = i + 1;
            let first = node - (node.isolate_lowest_one());
            let value = self.prefix(i) - self.prefix(first) + self.messages[i].height;
            self.heights.push(value);
        }
    }
    pub fn resize(&mut self, width: u16) {
        if self.width == width {
            return;
        }
        self.width = width;
        for message in &mut self.messages {
            message.prepare(usize::from(width.saturating_sub(1)));
        }
        self.reindex();
    }
    /// Prepending durable pages preserves caller-held canonical anchors.
    pub fn prepend(&mut self, models: impl IntoIterator<Item = TranscriptMessage>) {
        let mut preceding = TranscriptView::new(self.width);
        for model in models {
            if !self.indexes.contains_key(&model.id) {
                preceding.upsert(model);
            }
        }
        preceding.messages.append(&mut self.messages);
        self.messages = preceding.messages;
        self.reindex();
    }
    pub fn retain(&mut self, mut keep: impl FnMut(MessageId) -> bool) {
        let old = self.messages.len();
        self.messages.retain(|m| keep(m.model.id));
        if old != self.messages.len() {
            self.reindex();
        }
    }
    pub fn first_id(&self) -> Option<MessageId> {
        self.messages.first().map(|m| m.model.id)
    }
    pub fn start_row(&self, id: MessageId) -> Option<usize> {
        self.indexes.get(&id).map(|i| self.prefix(*i))
    }
    fn reindex(&mut self) {
        self.indexes.clear();
        self.heights = vec![0; self.messages.len() + 1];
        for (i, message) in self.messages.iter().enumerate() {
            self.indexes.insert(message.model.id, i);
            let mut node = i + 1;
            while node < self.heights.len() {
                self.heights[node] += message.height;
                node += node.isolate_lowest_one();
            }
        }
    }
    fn prefix(&self, mut end: usize) -> usize {
        let mut sum = 0;
        while end > 0 {
            sum += self.heights[end];
            end &= end - 1;
        }
        sum
    }
    /// Binary lifting over cumulative heights, O(log messages).
    fn locate(&self, row: usize) -> (usize, usize) {
        let mut i = 0;
        let mut sum = 0;
        let mut step = self.messages.len().checked_next_power_of_two().unwrap_or(0);
        while step > 0 {
            let next = i + step;
            if next < self.heights.len() && sum + self.heights[next] <= row {
                sum += self.heights[next];
                i = next;
            }
            step >>= 1;
        }
        (i, row.saturating_sub(sum))
    }
    pub fn anchor_at(&self, row: usize) -> Option<TranscriptAnchor> {
        let (i, local) = self.locate(row.min(self.total_rows().saturating_sub(1)));
        let message = self.messages.get(i)?;
        let body = local.saturating_sub(message.body_start());
        Some(TranscriptAnchor {
            id: message.model.id,
            source_grapheme: message
                .source_rows
                .get(body)
                .copied()
                .unwrap_or_else(|| message.markdown.source().graphemes(true).count()),
            row_in_header: (local < message.body_start()).then_some(local),
        })
    }
    /// Resolve a retained source anchor to the nearest containing visual row.
    /// Block-local raw wrapping approximates offsets around Markdown delimiters;
    /// exact rich-text provenance requires source offsets from MarkdownRenderer.
    /// Keep the original anchor across reflows instead of recapturing rounded rows.
    pub fn row_for_anchor(&self, anchor: TranscriptAnchor) -> Option<usize> {
        let &i = self.indexes.get(&anchor.id)?;
        let message = &self.messages[i];
        let local = anchor.row_in_header.map_or_else(
            || {
                message.body_start()
                    + message
                        .source_rows
                        .partition_point(|g| *g <= anchor.source_grapheme)
                        .saturating_sub(1)
            },
            |r| r.min(message.body_start().saturating_sub(1)),
        );
        Some(self.prefix(i) + local)
    }
}
#[derive(Clone, Debug, Default)]
pub struct TranscriptRects {
    pub cards: Vec<(MessageId, Rect)>,
    pub protected: Vec<Rect>,
    pub rails: Vec<Rect>,
    pub cursors: Vec<Rect>,
    pub seals: Vec<Rect>,
    pub rows_drawn: usize,
    pub messages_visited: usize,
}
/// Paint exactly the clipped visible rows, with zero overscan. No parsing,
/// highlighting, source cloning, or history-wide iteration occurs here.
pub fn render(
    view: &TranscriptView,
    top: usize,
    area: Rect,
    buf: &mut Buffer,
    palette: &Palette,
) -> TranscriptRects {
    let area = area.intersection(buf.area);
    let mut geometry = TranscriptRects::default();
    if area.is_empty() {
        return geometry;
    }
    let (mut index, mut local) = view.locate(top);
    let mut y = area.y;
    while y < area.bottom() && index < view.messages.len() {
        let message = &view.messages[index];
        geometry.messages_visited += 1;
        let take = (message.height - local).min(usize::from(area.bottom() - y));
        geometry.cards.push((
            message.model.id,
            Rect::new(area.x, y, area.width, take as u16),
        ));
        let color = palette.member(
            if message.model.human {
                "me"
            } else {
                &message.model.author
            },
            message.model.slot,
        );
        let body_start = message.body_start();
        let body_end = body_start + message.markdown.row_count();
        let visible_body_start = local.max(body_start);
        let visible_body_end = (local + take).min(body_end);
        let body = message.markdown.visible_rows(
            visible_body_start.saturating_sub(body_start)
                ..visible_body_end.saturating_sub(body_start),
        );
        for offset in 0..take {
            let row = local + offset;
            let row_y = y + offset as u16;
            let rail = Rect::new(area.x, row_y, 1, 1);
            buf.set_string(
                area.x,
                row_y,
                if row + 1 == message.height {
                    " "
                } else {
                    "│"
                },
                Style::default().fg(color),
            );
            geometry.rails.push(rail);
            if !message.model.complete && row + 1 == body_end {
                buf.set_string(area.x, row_y, "▌", Style::default().fg(color));
            }
            let content = Rect::new(area.x + 1, row_y, area.width.saturating_sub(1), 1);
            if row == 0 && message.header_rows() > 0 {
                let header = format!(
                    "{}{}{}",
                    sanitize(&message.model.author),
                    if message.model.version.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", sanitize(&message.model.version))
                    },
                    if message.model.status.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", sanitize(&message.model.status))
                    }
                );
                Line::styled(
                    header,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )
                .render(content, buf);
            } else if row < body_start {
                message.prefix[row - message.header_rows()]
                    .clone()
                    .render(content, buf);
            } else if row < body_end {
                let mut line = body.lines[row - visible_body_start].clone();
                if palette.tier == ColorTier::NoColor {
                    line.style.fg = None;
                    line.style.bg = None;
                    for span in &mut line.spans {
                        span.style.fg = None;
                        span.style.bg = None;
                    }
                }
                line.render(content, buf);
            } else if row - body_end < message.suffix.len() {
                message.suffix[row - body_end].clone().render(content, buf);
            }
            let mut protected_content = content;
            if !message.model.complete && row + 1 == body_end && content.width > 1 {
                let end = (content.x..content.right())
                    .rfind(|x| buf[(*x, row_y)].symbol() != " ")
                    .map_or(content.x, |x| {
                        x + unicode_width::UnicodeWidthStr::width(buf[(x, row_y)].symbol()) as u16
                    });
                if end < content.right() {
                    geometry.cursors.push(Rect::new(end, row_y, 1, 1));
                    protected_content.width = end - content.x;
                }
            }
            geometry.protected.push(protected_content);
            geometry.rows_drawn += 1;
        }
        y += take as u16;
        index += 1;
        local = 0;
    }
    geometry
}

#[cfg(test)]
mod anchor_tests {
    use super::anchor_source_rows;
    use unicode_segmentation::UnicodeSegmentation;

    #[test]
    fn lightweight_anchor_rows_match_source_layout_for_unicode_tabs_and_controls() {
        for source in [
            "",
            "1234",
            "a\tb\r\n世界 👩‍💻e\u{301}\n",
            "a\u{1b}b\u{200d}c",
            "\t\t\n\nlongwordwithoutbreaks",
        ] {
            for width in 1..20 {
                let expected = crate::text::wrap(source, width)
                    .iter()
                    .map(|row| source[..row.source_start].graphemes(true).count())
                    .collect::<Vec<_>>();
                let (actual, count) = anchor_source_rows(source, width);
                assert_eq!(actual, expected, "source={source:?} width={width}");
                assert_eq!(count, source.graphemes(true).count());
            }
        }
    }
}
