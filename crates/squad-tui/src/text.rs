//! Shared grapheme layout. Width policy: Cargo.lock's unicode-width 0.2.3,
//! non-CJK widths (ambiguous characters occupy one cell), four-cell tab stops.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub fn boundaries(text: &str) -> Vec<usize> {
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellGrapheme {
    /// Byte range in the original source, including both bytes of CRLF.
    pub source: Range<usize>,
    pub display: String,
    pub column: usize,
    pub width: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VisualRow {
    pub source_start: usize,
    pub source_end: usize,
    pub cells: Vec<CellGrapheme>,
}

/// Greedy wrapping preserves every source grapheme. A grapheme wider than the
/// viewport occupies its own row; renderers must clip it rather than split it.
pub fn wrap(text: &str, width: usize) -> Vec<VisualRow> {
    let width = width.max(1);
    let mut rows = vec![VisualRow::default()];
    let mut column = 0;
    for (start, grapheme) in text.grapheme_indices(true) {
        let end = start + grapheme.len();
        if grapheme == "\n" || grapheme == "\r\n" {
            rows.last_mut().unwrap().source_end = start;
            rows.push(VisualRow {
                source_start: end,
                source_end: end,
                cells: vec![],
            });
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
            rows.push(VisualRow {
                source_start: start,
                source_end: start,
                cells: vec![],
            });
            column = 0;
            cells = if grapheme == "\t" { 4 } else { measured };
        }
        let display = if grapheme == "\t" {
            " ".repeat(cells)
        } else if control {
            "�".into()
        } else {
            grapheme.into()
        };
        let row = rows.last_mut().unwrap();
        row.cells.push(CellGrapheme {
            source: start..end,
            display,
            column,
            width: cells,
        });
        row.source_end = end;
        column += cells;
    }
    // Give the caret its own row when the last row is exactly full.
    if column >= width {
        rows.push(VisualRow {
            source_start: text.len(),
            source_end: text.len(),
            cells: vec![],
        });
    }
    rows
}

pub fn cursor_position(rows: &[VisualRow], byte: usize) -> (usize, usize) {
    let row_index = rows
        .iter()
        .rposition(|row| row.source_start <= byte)
        .unwrap_or(0);
    let column = rows[row_index]
        .cells
        .iter()
        .take_while(|cell| cell.source.start < byte)
        .map(|cell| cell.width)
        .sum();
    (row_index, column)
}

pub fn byte_at_column(row: &VisualRow, column: usize) -> usize {
    row.cells
        .iter()
        .find(|cell| cell.column + cell.width > column)
        .map_or(row.source_end, |cell| cell.source.start)
}

/// Delimiter whitespace may be consumed at prose soft wraps. Source/editor and
/// code layout deliberately keep every grapheme through `wrap` instead.
pub fn is_wrap_delimiter(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() && c != '\n' && c != '\r')
}
