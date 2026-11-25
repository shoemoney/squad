//! Incremental, protocol-independent Markdown presentation.
//!
//! Call [`MarkdownRenderer::update`] before drawing. Drawing can then request only
//! [`MarkdownRenderer::visible_rows`]. A key identifies one message attempt; use a
//! different key for a retry/replacement. Width includes the two-cell side padding.

mod highlight;

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use self::highlight::FenceHighlighter;

/// Top-level structural identity, independent of protocol and terminal widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading,
    Code { language: String },
    List,
    Quote,
    Table,
    Rule,
    Other,
}

/// A parsed block with separate styled-source and width-dependent row caches.
#[derive(Clone, Debug)]
pub struct MarkdownBlock {
    pub kind: BlockKind,
    /// UTF-8 offsets in the sanitized, LF-normalized source, never display cells.
    pub source_range: Range<usize>,
    pub finalized: bool,
    code: Option<String>,
    styled: Vec<Line<'static>>,
    wrapped: Vec<Line<'static>>,
}

impl MarkdownBlock {
    /// Unwrapped code for copying, without the fence label or visual padding.
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

/// Counters useful for verifying that streaming and resizing reuse cached work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub parse_passes: usize,
    pub parsed_bytes: usize,
    pub wrapped_blocks: usize,
    pub highlighted_lines: usize,
}

/// One message's presentation cache. Own one instance per independently streamed reply.
#[derive(Default)]
pub struct MarkdownRenderer {
    key: String,
    source: String,
    complete: bool,
    width: usize,
    blocks: Vec<MarkdownBlock>,
    fence: FenceHighlighter,
    stats: CacheStats,
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse only the revisable tail on append. Replacement invalidates all state;
    /// durable completion reconciles the whole message once (including references).
    /// Highlighting happens here, never in `visible_rows`.
    pub fn update(&mut self, attempt_key: &str, source: &str, complete: bool, width: usize) {
        let source = sanitize(source);
        let replaced = self.key != attempt_key
            || !source.starts_with(&self.source)
            || (self.complete && (source != self.source || !complete));
        if replaced {
            self.blocks.clear();
            self.fence.clear();
            self.source.clear();
            self.complete = false;
        }
        let changed = source != self.source || complete != self.complete;
        let resized = self.width != width;
        self.key = attempt_key.to_owned();
        self.width = width;
        if changed {
            let start = if complete {
                // Reference definitions can resolve links in earlier blocks.
                self.blocks.clear();
                0
            } else {
                self.blocks.retain(|block| block.finalized);
                self.blocks.last().map_or(0, |block| block.source_range.end)
            };
            self.stats.parse_passes += 1;
            self.stats.parsed_bytes += source.len() - start;
            let mut parsed = parse_blocks(&source[start..], start, &mut self.fence);
            // A parser can temporarily emit an incomplete table row as a new
            // paragraph, then absorb it into the table on the next append. Keep
            // the whole contiguous group revisable until a blank-line boundary.
            let finalized_count = parsed
                .windows(2)
                .enumerate()
                .filter_map(|(index, pair)| {
                    let prefix = &source[..pair[1].source_range.start];
                    prefix
                        .lines()
                        .next_back()
                        .is_some_and(|line| line.trim().is_empty())
                        .then_some(index + 1)
                })
                .next_back()
                .unwrap_or(0);
            for (index, block) in parsed.iter_mut().enumerate() {
                block.finalized = complete || index < finalized_count;
                if !resized {
                    block.wrapped = wrap_lines_policy(
                        &block.styled,
                        width,
                        !matches!(block.kind, BlockKind::Code { .. }),
                    );
                    self.stats.wrapped_blocks += 1;
                }
            }
            self.blocks.extend(parsed);
            self.source = source;
            self.complete = complete;
        }
        if resized {
            for block in &mut self.blocks {
                block.wrapped = wrap_lines_policy(
                    &block.styled,
                    width,
                    !matches!(block.kind, BlockKind::Code { .. }),
                );
                self.stats.wrapped_blocks += 1;
            }
        }
        self.stats.highlighted_lines = self.fence.highlighted_lines();
    }

    /// Perform CPU work on Tokio's blocking pool. Spawn this future from the
    /// presentation worker and publish the returned cache when ready; the input
    /// loop and drawing can continue using their previous presentation.
    pub async fn prepare(
        mut self,
        attempt_key: String,
        source: String,
        complete: bool,
        width: usize,
    ) -> Result<Self, tokio::task::JoinError> {
        tokio::task::spawn_blocking(move || {
            self.update(&attempt_key, &source, complete, width);
            self
        })
        .await
    }

    /// Sanitized source remains independent of terminal wrapping.
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    pub fn row_count(&self) -> usize {
        self.blocks.iter().map(|block| block.wrapped.len()).sum()
    }

    /// Clone only requested cached rows. Out-of-range/empty ranges yield empty text.
    pub fn visible_rows(&self, rows: Range<usize>) -> Text<'static> {
        let mut offset = 0;
        let mut lines = Vec::new();
        for block in &self.blocks {
            let end = offset + block.wrapped.len();
            if offset >= rows.end {
                break;
            }
            if end > rows.start {
                let start = rows.start.saturating_sub(offset);
                let stop = rows.end.saturating_sub(offset).min(block.wrapped.len());
                if start < stop {
                    lines.extend_from_slice(&block.wrapped[start..stop]);
                }
            }
            offset = end;
        }
        Text::from(lines)
    }
}

/// Remove ECMA-48 escape sequences and terminal controls before producing spans.
/// Keep tabs/newlines for layout and normalize CRLF. Safe for metadata as well.
pub fn sanitize(input: &str) -> String {
    let mut chars = input.chars().peekable();
    let mut output = String::with_capacity(input.len());
    while let Some(ch) = chars.next() {
        match ch {
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => output.push('\n'),
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    let mut escape = false;
                    for c in chars.by_ref() {
                        if c == '\u{9c}' || c == '\u{7}' || (escape && c == '\\') {
                            break;
                        }
                        escape = c == '\u{1b}';
                    }
                }
                Some(c) if (' '..='/').contains(&c) => {
                    for c in chars.by_ref() {
                        if ('0'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => {
                let mut escape = false;
                for c in chars.by_ref() {
                    if c == '\u{9c}' || c == '\u{7}' || (escape && c == '\\') {
                        break;
                    }
                    escape = c == '\u{1b}';
                }
            }
            '\u{9b}' => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            '\n' | '\t' => output.push(ch),
            c if c.is_control() => {}
            c => output.push(c),
        }
    }
    output
}

fn parse_blocks(source: &str, base: usize, fence: &mut FenceHighlighter) -> Vec<MarkdownBlock> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut blocks = Vec::new();
    let mut events = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        if events.is_empty() {
            start = range.start;
        }
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
        // Entities can decode to control characters after source sanitization.
        let event = match event {
            Event::Text(text) => Event::Text(sanitize(&text).into()),
            Event::Html(text) => Event::Html(sanitize(&text).into()),
            Event::InlineHtml(text) => Event::InlineHtml(sanitize(&text).into()),
            Event::Code(text) => Event::Code(sanitize(&text).into()),
            event => event,
        };
        events.push(event);
        if depth == 0 {
            let kind = match &events[0] {
                Event::Start(Tag::Paragraph) => BlockKind::Paragraph,
                Event::Start(Tag::Heading { .. }) => BlockKind::Heading,
                Event::Start(Tag::CodeBlock(kind)) => BlockKind::Code {
                    language: match kind {
                        CodeBlockKind::Fenced(info) => info.to_string(),
                        _ => String::new(),
                    },
                },
                Event::Start(Tag::List(_)) => BlockKind::List,
                Event::Start(Tag::BlockQuote(_)) => BlockKind::Quote,
                Event::Start(Tag::Table(_)) => BlockKind::Table,
                Event::Rule => BlockKind::Rule,
                _ => BlockKind::Other,
            };
            let mut code_source = None;
            let styled = if let BlockKind::Code { language } = &kind {
                let code: String = events
                    .iter()
                    .filter_map(|event| match event {
                        Event::Text(s) => Some(s.as_ref()),
                        _ => None,
                    })
                    .collect();
                let mut lines = vec![Line::styled(
                    format!("```{language}"),
                    Style::default().fg(Color::DarkGray),
                )];
                lines.extend(fence.render((base + start, 0), language, &code));
                code_source = Some(code);
                lines
            } else {
                render_events(&events, base + start, fence)
            };
            blocks.push(MarkdownBlock {
                kind,
                source_range: base + start..base + range.end,
                finalized: false,
                code: code_source,
                styled,
                wrapped: Vec::new(),
            });
            events.clear();
        }
    }
    blocks
}

fn render_events(
    events: &[Event<'_>],
    offset: usize,
    fence: &mut FenceHighlighter,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut styles = vec![Style::default()];
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote_depth = 0;
    let mut cell = 0;
    let mut table_head = false;
    let flush = |lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>| {
        if !spans.is_empty() {
            lines.push(Line::from(std::mem::take(spans)));
        }
    };
    let mut events = events.iter().enumerate();
    while let Some((index, event)) = events.next() {
        if let Event::Start(Tag::CodeBlock(kind)) = event {
            flush(&mut lines, &mut spans);
            let language = match kind {
                CodeBlockKind::Fenced(info) => info.as_ref(),
                _ => "",
            };
            let mut code = String::new();
            for (_, event) in events.by_ref() {
                match event {
                    Event::Text(text) => code.push_str(text),
                    Event::End(TagEnd::CodeBlock) => break,
                    _ => {}
                }
            }
            let mut code_lines = vec![Line::styled(
                format!("```{language}"),
                Style::default().fg(Color::DarkGray),
            )];
            code_lines.extend(fence.render((offset, index), language, &code));
            for mut line in code_lines {
                if quote_depth > 0 {
                    line.spans.insert(
                        0,
                        Span::styled(
                            "│ ".repeat(quote_depth),
                            Style::default().fg(Color::DarkGray),
                        ),
                    );
                }
                lines.push(line);
            }
            continue;
        }
        let style = *styles.last().unwrap_or(&Style::default());
        match event {
            Event::Start(tag) => match tag {
                Tag::Emphasis => styles.push(style.add_modifier(Modifier::ITALIC)),
                Tag::Strong | Tag::Heading { .. } => {
                    styles.push(style.add_modifier(Modifier::BOLD))
                }
                Tag::Strikethrough => styles.push(style.add_modifier(Modifier::CROSSED_OUT)),
                Tag::Link { .. } => {
                    styles.push(style.fg(Color::Cyan).add_modifier(Modifier::UNDERLINED))
                }
                Tag::BlockQuote(_) => {
                    flush(&mut lines, &mut spans);
                    quote_depth += 1;
                }
                Tag::List(first) => {
                    flush(&mut lines, &mut spans);
                    lists.push(*first);
                }
                Tag::Item => {
                    flush(&mut lines, &mut spans);
                    if quote_depth > 0 {
                        spans.push(Span::styled(
                            "│ ".repeat(quote_depth),
                            Style::default().fg(Color::DarkGray),
                        ));
                    }
                    spans.push(Span::raw("  ".repeat(lists.len().saturating_sub(1))));
                    let marker = match lists.last_mut() {
                        Some(Some(number)) => {
                            let marker = format!("{number}. ");
                            *number += 1;
                            marker
                        }
                        _ => "• ".to_owned(),
                    };
                    spans.push(Span::raw(marker));
                }
                Tag::TableHead => {
                    table_head = true;
                    cell = 0;
                }
                Tag::TableRow => {
                    flush(&mut lines, &mut spans);
                    cell = 0;
                }
                Tag::TableCell => {
                    if cell > 0 {
                        spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
                    }
                    cell += 1;
                    styles.push(if table_head {
                        style.add_modifier(Modifier::BOLD)
                    } else {
                        style
                    });
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Heading(_)
                | TagEnd::Link
                | TagEnd::TableCell => {
                    styles.pop();
                }
                TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow => {
                    flush(&mut lines, &mut spans)
                }
                TagEnd::TableHead => {
                    flush(&mut lines, &mut spans);
                    table_head = false;
                }
                TagEnd::List(_) => {
                    flush(&mut lines, &mut spans);
                    lists.pop();
                }
                TagEnd::BlockQuote(_) => {
                    flush(&mut lines, &mut spans);
                    quote_depth -= 1;
                }
                _ => {}
            },
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                if spans.is_empty() && quote_depth > 0 {
                    spans.push(Span::styled(
                        "│ ".repeat(quote_depth),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                spans.push(Span::styled(text.to_string(), style));
            }
            Event::Code(code) => {
                if spans.is_empty() && quote_depth > 0 {
                    spans.push(Span::styled(
                        "│ ".repeat(quote_depth),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                spans.push(Span::styled(code.to_string(), style.fg(Color::Yellow)));
            }
            Event::SoftBreak | Event::HardBreak => {
                lines.push(Line::from(std::mem::take(&mut spans)));
            }
            Event::Rule => spans.push(Span::styled("────", Style::default().fg(Color::DarkGray))),
            Event::TaskListMarker(done) => {
                spans.push(Span::raw(if *done { "[x] " } else { "[ ] " }))
            }
            _ => {}
        }
    }
    flush(&mut lines, &mut spans);
    lines
}

/// Greedy grapheme wrapping, shared by prose and code. Styling boundaries cannot
/// split an extended grapheme. Tabs use four-cell stops at the content origin.
pub fn wrap_lines(lines: &[Line<'_>], width: usize) -> Vec<Line<'static>> {
    wrap_lines_policy(lines, width, true)
}
fn wrap_lines_policy(lines: &[Line<'_>], width: usize, prose: bool) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let padding = if width >= 5 { 2 } else { 0 };
    let available = width - 2 * padding;
    let mut output = Vec::new();
    for line in lines {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let mut row = Vec::new();
        let mut cells = 0;
        let mut source_cells = 0;
        let mut soft_wrap = false;
        let mut span_index = 0;
        let mut span_end = line.spans.first().map_or(0, |span| span.content.len());
        let words: std::collections::HashMap<usize, usize> = text
            .split_word_bound_indices()
            .filter(|(_, word)| !word.chars().all(char::is_whitespace))
            .map(|(offset, word)| (offset, word.width()))
            .collect();
        for (offset, grapheme) in text.grapheme_indices(true) {
            if let Some(word_width) = words.get(&offset)
                && *word_width <= available
                && cells + word_width > available
                && !row.is_empty()
            {
                if prose {
                    trim_delimiters(&mut row);
                }
                output.push(padded(std::mem::take(&mut row), padding));
                cells = 0;
                soft_wrap = true;
            }
            while offset >= span_end && span_index + 1 < line.spans.len() {
                span_index += 1;
                span_end += line.spans[span_index].content.len();
            }
            let style = line
                .spans
                .get(span_index)
                .map_or(line.style, |span| line.style.patch(span.style));
            if grapheme == "\n" || grapheme == "\r\n" {
                output.push(padded(std::mem::take(&mut row), padding));
                cells = 0;
                source_cells = 0;
                soft_wrap = false;
                continue;
            }
            let expanded;
            let grapheme = if grapheme == "\t" {
                expanded = " ".repeat(4 - source_cells % 4);
                expanded.as_str()
            } else {
                grapheme
            };
            // Expanded tab spaces may wrap individually; original graphemes never do.
            let parts: Vec<&str> = if grapheme.starts_with(' ') {
                grapheme.graphemes(true).collect()
            } else {
                vec![grapheme]
            };
            for part in parts {
                let size = UnicodeWidthStr::width(part);
                if cells + size > available && !row.is_empty() {
                    if prose {
                        trim_delimiters(&mut row);
                    }
                    output.push(padded(std::mem::take(&mut row), padding));
                    cells = 0;
                    soft_wrap = true;
                }
                if prose && soft_wrap && row.is_empty() && crate::text::is_wrap_delimiter(part) {
                    continue;
                }
                row.push(Span::styled(part.to_owned(), style));
                cells += size;
                source_cells += size;
            }
        }
        output.push(padded(row, padding));
    }
    output
}

fn padded(mut spans: Vec<Span<'static>>, padding: usize) -> Line<'static> {
    if padding > 0 {
        spans.insert(0, Span::raw(" ".repeat(padding)));
        spans.push(Span::raw(" ".repeat(padding)));
    }
    Line::from(spans)
}

fn trim_delimiters(row: &mut Vec<Span<'static>>) {
    while row
        .last()
        .is_some_and(|s| crate::text::is_wrap_delimiter(&s.content))
    {
        row.pop();
    }
}
