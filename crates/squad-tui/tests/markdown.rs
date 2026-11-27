use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use squad_tui::markdown::{BlockKind, MarkdownRenderer, sanitize, wrap_lines};
use unicode_width::UnicodeWidthStr;

fn rows(renderer: &MarkdownRenderer) -> Vec<String> {
    renderer
        .visible_rows(0..usize::MAX)
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

#[test]
fn streamed_tail_can_become_a_table_or_setext_heading() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update("attempt", "Intro\n\nName | Value\n", false, 40);
    assert_eq!(renderer.blocks().len(), 2);
    assert!(renderer.blocks()[0].finalized);
    assert!(!renderer.blocks()[1].finalized);
    renderer.update(
        "attempt",
        "Intro\n\nName | Value\n--- | ---\nA | B\n",
        false,
        40,
    );
    assert_eq!(renderer.blocks()[1].kind, BlockKind::Table);
    assert!(rows(&renderer).iter().any(|row| row.contains("A │ B")));
    renderer.update("new", "Title\n", false, 40);
    renderer.update("new", "Title\n====\n", false, 40);
    assert_eq!(renderer.blocks()[0].kind, BlockKind::Heading);
}

#[test]
fn append_reuses_final_blocks_and_resize_only_rewraps() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", "cached paragraph\n\nlive", false, 24);
    let before = renderer.stats();
    let prefix_end = renderer.blocks()[0].source_range.end;
    let source = "cached paragraph\n\nlive tail";
    renderer.update("one", source, false, 24);
    assert_eq!(
        renderer.stats().parsed_bytes - before.parsed_bytes,
        source.len() - prefix_end
    );
    assert_eq!(renderer.stats().wrapped_blocks - before.wrapped_blocks, 1);
    let before = renderer.stats();
    renderer.update("one", source, false, 12);
    assert_eq!(renderer.stats().parse_passes, before.parse_passes);
    assert_eq!(renderer.stats().highlighted_lines, before.highlighted_lines);
    assert!(
        rows(&renderer)
            .iter()
            .all(|row| UnicodeWidthStr::width(row.as_str()) <= 12)
    );
    renderer.update("one", source, true, 12);
    assert!(renderer.blocks().iter().all(|block| block.finalized));
    let before = renderer.stats();
    renderer.update("one", source, true, 12);
    assert_eq!(renderer.stats(), before);
    renderer.update("two", "replacement", false, 12);
    assert_eq!(renderer.blocks().len(), 1);
    assert!(!renderer.blocks()[0].finalized);
    assert!(!rows(&renderer).join("\n").contains("cached"));
}

#[test]
fn every_stream_prefix_matches_fresh_parsing() {
    let source = "# Title\n\nHello **bold** and `code`.\n\n> quoted\n> text\n\n1. one\n2. two\n   - nested\n\n| A | B |\n| - | - |\n| 界 | 👩‍💻 |\n\n```rust\n/* comment\ncontinued */\nfn main() {}\n```\n\nend";
    let mut incremental = MarkdownRenderer::new();
    for end in source
        .char_indices()
        .map(|(index, _)| index)
        .chain([source.len()])
    {
        incremental.update("same", &source[..end], false, 30);
        let mut fresh = MarkdownRenderer::new();
        fresh.update("same", &source[..end], false, 30);
        assert_eq!(
            incremental.visible_rows(0..usize::MAX),
            fresh.visible_rows(0..usize::MAX),
            "prefix {end}"
        );
    }
}

#[test]
fn open_code_state_is_incremental_and_partial_line_revisable() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", "```rust\n/* start\n", false, 80);
    let before = renderer.stats().highlighted_lines;
    renderer.update("one", "```rust\n/* start\nstill", false, 80);
    assert_eq!(renderer.stats().highlighted_lines - before, 1);
    let before = renderer.stats().highlighted_lines;
    renderer.update(
        "one",
        "```rust\n/* start\nstill comment */\nlet x = 3;",
        false,
        80,
    );
    assert_eq!(renderer.stats().highlighted_lines - before, 2);
    let mut fresh = MarkdownRenderer::new();
    fresh.update(
        "one",
        "```rust\n/* start\nstill comment */\nlet x = 3;",
        false,
        80,
    );
    assert_eq!(renderer.visible_rows(0..100), fresh.visible_rows(0..100));
    renderer.update("two", "```no_such_language\nabc\n", false, 80);
    assert!(rows(&renderer)[0].contains("no_such_language"));
    let text = renderer.visible_rows(1..2);
    assert!(
        text.lines[0]
            .spans
            .iter()
            .filter(|span| span.content.contains("abc"))
            .all(|span| span.style.fg == Some(Color::Gray))
    );
}

#[test]
fn wraps_cells_graphemes_tabs_and_explicit_newlines() {
    let lines = wrap_lines(&[Line::from(Span::raw("e\u{301}界👩‍💻\tX\nlast"))], 10);
    let content: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    assert_eq!(content, ["  e\u{301}界👩‍💻  ", "  X  ", "  last  "]);
    assert!(
        content
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 10)
    );
    let split = Line::from(vec![
        Span::raw("e"),
        Span::styled("\u{301}", Style::default().add_modifier(Modifier::BOLD)),
    ]);
    assert_eq!(wrap_lines(&[split], 5)[0].spans[1].content, "e\u{301}");
    assert!(wrap_lines(&[Line::raw("anything")], 0).is_empty());
}

#[test]
fn visible_rows_are_bounded_and_drawing_does_no_work() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", "first\n\nsecond\n\nthird", true, 20);
    let before = renderer.stats();
    assert_eq!(renderer.visible_rows(1..2).lines.len(), 1);
    assert!(renderer.visible_rows(20..30).lines.is_empty());
    assert!(
        renderer
            .visible_rows(std::ops::Range { start: 2, end: 1 })
            .lines
            .is_empty()
    );
    assert_eq!(renderer.stats(), before);
}

#[test]
fn controls_are_removed_before_markdown_or_metadata_display() {
    assert_eq!(
        sanitize("a\x1b[31mb\x1b[0m\x1b]8;;https://evil\x07c\x1b]8;;\x1b\\\r\nx\0\x08\t!"),
        "abc\nx\t!"
    );
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", "```text\n\x1b[2Jhello\n```", true, 20);
    assert!(!rows(&renderer).join("").contains('\x1b'));
}

#[test]
fn completion_resolves_earlier_reference_links_once() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", "[link][id]\n\nlive", false, 40);
    renderer.update(
        "one",
        "[link][id]\n\nlive\n\n[id]: https://example.org",
        true,
        40,
    );
    assert!(rows(&renderer)[0].contains("link"));
    assert!(!rows(&renderer)[0].contains("[id]"));
    assert!(
        renderer.visible_rows(0..1).lines[0]
            .spans
            .iter()
            .any(|span| span.style.add_modifier.contains(Modifier::UNDERLINED))
    );
}

#[test]
fn structural_markdown_snapshot() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", "# Heading\n\nPlain **bold** *italic* ~~gone~~ and `inline`.\n\n> quoted text\n\n- [x] done\n- next\n  - nested\n\nA | B\n-- | --\n界 | 👩‍💻\n\n```unknown\na\tb\n```", true, 32);
    insta::assert_debug_snapshot!("markdown_structures", rows(&renderer));
}

#[tokio::test]
async fn prepares_off_the_input_and_drawing_task() {
    let renderer = MarkdownRenderer::new()
        .prepare(
            "attempt".into(),
            "```rust\nfn main() {}\n```".into(),
            true,
            40,
        )
        .await
        .unwrap();
    assert_eq!(renderer.blocks()[0].code(), Some("fn main() {}\n"));
    assert!(renderer.row_count() > 0);
}

#[test]
fn nested_fences_are_highlighted_and_entity_controls_cannot_escape() {
    let mut renderer = MarkdownRenderer::new();
    renderer.update(
        "attempt",
        "> ```rust\n> fn main() {}\n> ```\n\n&#27;[31mtext",
        true,
        40,
    );
    let text = renderer.visible_rows(0..100);
    assert!(rows(&renderer)[0].contains("│ ```rust"));
    assert!(
        text.lines[1]
            .spans
            .iter()
            .any(|span| matches!(span.style.fg, Some(Color::Rgb(_, _, _))))
    );
    assert!(!rows(&renderer).join("").chars().any(|c| c.is_control()));
    assert_eq!(sanitize("a\u{9d}hidden\u{9c}b\u{90}hidden\x1b\\c"), "abc");
}

#[test]
fn prose_wraps_at_words_across_style_spans() {
    use ratatui::style::{Color, Style};
    let lines = wrap_lines(
        &[Line::from(vec![
            Span::raw("keep "),
            Span::styled("respon", Style::default().fg(Color::Red)),
            Span::styled("siveness", Style::default().fg(Color::Cyan)),
            Span::raw(" visible"),
        ])],
        20,
    );
    let text = lines
        .iter()
        .map(|line| line.to_string().trim().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(text, vec!["keep", "responsiveness", "visible"]);
    assert_eq!(
        lines[1]
            .spans
            .iter()
            .find(|s| s.content == "r")
            .unwrap()
            .style
            .fg,
        Some(Color::Red)
    );
    assert_eq!(
        lines[1]
            .spans
            .iter()
            .find(|s| s.content == "v")
            .unwrap()
            .style
            .fg,
        Some(Color::Cyan)
    );
}

#[test]
fn soft_wrap_consumes_prose_delimiters_but_preserves_code_indentation() {
    let rows = wrap_lines(&[Line::raw("  one two three four five")], 12);
    assert!(rows[0].to_string().starts_with("    one"));
    for row in &rows[1..] {
        let text = row.to_string();
        assert!(!text[2..].starts_with(' '), "stray delimiter: {text:?}");
    }
    let mut code = MarkdownRenderer::new();
    code.update("code", "```text\n    one  two\n```", true, 10);
    let text = code
        .visible_rows(0..code.row_count())
        .lines
        .iter()
        .map(ToString::to_string)
        .collect::<String>();
    assert!(text.contains("      "));
}

#[test]
fn completed_fences_reuse_rows_on_resize_and_rebuild_on_replacement() {
    let source =
        "```rust\n/* multi\nline */ let x = 42;\n```\n\n[link][ref]\n\n[ref]: https://example.com";
    let mut renderer = MarkdownRenderer::new();
    renderer.update("one", source, false, 80);
    renderer.update("one", source, true, 80);
    let stats = renderer.stats();
    renderer.update("one", source, true, 24);
    assert_eq!(renderer.stats().parse_passes, stats.parse_passes);
    assert_eq!(renderer.stats().highlighted_lines, stats.highlighted_lines);
    let mut fresh = MarkdownRenderer::new();
    fresh.update("one", source, true, 24);
    assert_eq!(
        renderer.visible_rows(0..usize::MAX),
        fresh.visible_rows(0..usize::MAX)
    );
    renderer.update("two", "```rust\nlet x = 7;\n```", true, 24);
    fresh.update("two", "```rust\nlet x = 7;\n```", true, 24);
    assert_eq!(
        renderer.visible_rows(0..usize::MAX),
        fresh.visible_rows(0..usize::MAX)
    );
}
