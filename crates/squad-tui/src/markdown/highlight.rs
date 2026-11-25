//! Incremental fence syntax state; complete lines are committed, partial lines revisable.
use std::{collections::HashMap, sync::OnceLock};

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Theme, ThemeSet},
    parsing::SyntaxSet,
};

fn assets() -> &'static (SyntaxSet, Theme) {
    static ASSETS: OnceLock<(SyntaxSet, Theme)> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let syntax = SyntaxSet::load_defaults_newlines();
        let mut themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .remove("base16-ocean.dark")
            .unwrap_or_default();
        (syntax, theme)
    })
}

#[derive(Default)]
pub(super) struct FenceHighlighter {
    fences: HashMap<(usize, usize), Fence>,
    highlighted_lines: usize,
}

struct Fence {
    language: String,
    committed: String,
    lines: Vec<Line<'static>>,
    highlighter: Option<HighlightLines<'static>>,
}

impl FenceHighlighter {
    pub fn clear(&mut self) {
        self.fences.clear();
    }
    pub fn highlighted_lines(&self) -> usize {
        self.highlighted_lines
    }

    pub fn render(
        &mut self,
        offset: (usize, usize),
        language: &str,
        code: &str,
    ) -> Vec<Line<'static>> {
        let (syntaxes, theme) = assets();
        let language = language.split_whitespace().next().unwrap_or("");
        let reset = self
            .fences
            .get(&offset)
            .is_none_or(|fence| fence.language != language || !code.starts_with(&fence.committed));
        if reset {
            let syntax = syntaxes.find_syntax_by_token(language);
            self.fences.insert(
                offset,
                Fence {
                    language: language.to_owned(),
                    committed: String::new(),
                    lines: Vec::new(),
                    highlighter: syntax.map(|syntax| HighlightLines::new(syntax, theme)),
                },
            );
        }
        let fence = self.fences.get_mut(&offset).expect("fence inserted above");
        let rest = &code[fence.committed.len()..];
        let complete_end = rest.rfind('\n').map_or(0, |index| index + 1);
        for line in rest[..complete_end].split_inclusive('\n') {
            fence
                .lines
                .push(highlight(fence.highlighter.as_mut(), line, syntaxes));
            fence.committed.push_str(line);
            self.highlighted_lines += 1;
        }
        let mut lines = fence.lines.clone();
        let partial = &rest[complete_end..];
        if !partial.is_empty() {
            // Fork the committed parser state: a partial string/comment may change
            // interpretation when the next token arrives.
            let mut preview = fence.highlighter.take().map(|highlighter| {
                let (highlight, parse) = highlighter.state();
                fence.highlighter = Some(HighlightLines::from_state(
                    theme,
                    highlight.clone(),
                    parse.clone(),
                ));
                HighlightLines::from_state(theme, highlight, parse)
            });
            lines.push(highlight(preview.as_mut(), partial, syntaxes));
            self.highlighted_lines += 1;
        }
        lines
    }
}

fn highlight(
    highlighter: Option<&mut HighlightLines<'static>>,
    line: &str,
    syntaxes: &SyntaxSet,
) -> Line<'static> {
    if let Some(highlighter) = highlighter
        && let Ok(tokens) = highlighter.highlight_line(line, syntaxes)
    {
        return Line::from(
            tokens
                .into_iter()
                .filter_map(|(style, text)| {
                    let text = text.trim_end_matches('\n');
                    if text.is_empty() {
                        return None;
                    }
                    let mut modifier = Modifier::empty();
                    if style.font_style.contains(FontStyle::BOLD) {
                        modifier |= Modifier::BOLD;
                    }
                    if style.font_style.contains(FontStyle::ITALIC) {
                        modifier |= Modifier::ITALIC;
                    }
                    if style.font_style.contains(FontStyle::UNDERLINE) {
                        modifier |= Modifier::UNDERLINED;
                    }
                    Some(Span::styled(
                        text.to_owned(),
                        Style::default()
                            .fg(Color::Rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            ))
                            .add_modifier(modifier),
                    ))
                })
                .collect::<Vec<_>>(),
        );
    }
    Line::styled(
        line.trim_end_matches('\n').to_owned(),
        Style::default().fg(Color::Gray),
    )
}
