//! Caret-aware completion using protocol scanner protection and UTF-8 spans.
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Clear, Widget},
};
use squad_protocol::parser::{
    ParseContext, SubmissionMeta,
    lexer::{TokenKind, tokenize},
};
use std::ops::Range;
#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    pub items: Vec<String>,
    pub selected: usize,
    pub replace: Range<usize>,
    pub descriptions: Vec<String>,
}
impl CompletionState {
    pub fn at(raw: &str, caret: usize, ctx: &ParseContext, meta: &SubmissionMeta) -> Self {
        Self::with_starters(raw, caret, ctx, meta, false)
    }
    pub fn with_starters(
        raw: &str,
        caret: usize,
        ctx: &ParseContext,
        meta: &SubmissionMeta,
        starters: bool,
    ) -> Self {
        if caret > raw.len() || !raw.is_char_boundary(caret) {
            return Self::default();
        }
        // An unfinished code span is still protected. The shared lexer reports its span.
        let prefix_tokens = match tokenize(&raw[..caret]) {
            Ok(tokens) => tokens,
            Err(_) => return Self::default(),
        };
        let tokens = match tokenize(raw) {
            Ok(tokens) => tokens,
            Err(e) if e.span.start < caret && caret <= e.span.end => return Self::default(),
            Err(_) => prefix_tokens.clone(),
        };
        if tokens.iter().any(|t| {
            matches!(t.kind, TokenKind::Protected | TokenKind::Escape)
                && t.span.start < caret
                && caret <= t.span.end
        }) {
            return Self::default();
        }
        let token = tokens.iter().find(|t| {
            t.span.start < caret
                && caret <= t.span.end
                && matches!(
                    t.kind,
                    TokenKind::Mention(_)
                        | TokenKind::Exclusion(_)
                        | TokenKind::Command(_)
                        | TokenKind::Variable(_)
                        | TokenKind::Assignment { .. }
                )
        });
        let start = token.map_or_else(
            || {
                raw[..caret]
                    .rfind(|c: char| c.is_whitespace() || "([{".contains(c))
                    .map_or(0, |i| i + 1)
            },
            |t| t.span.start,
        );
        let end = token.map_or_else(
            || {
                raw[caret..]
                    .find(char::is_whitespace)
                    .map_or(raw.len(), |i| caret + i)
            },
            |t| t.span.end,
        );
        let prefix = &raw[start..caret];
        let prior = tokenize(&raw[..start]).unwrap_or_default();
        let slash_line = raw.trim_start().starts_with('/');
        let has_mentions = prior
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Mention(_)));
        let has_all = prior
            .iter()
            .any(|t| matches!(&t.kind, TokenKind::Mention(n) if n == "all"));
        let mode = prior.iter().find_map(|t| match &t.kind {
            TokenKind::Command(name) => Some(name.as_str()),
            _ => None,
        });
        // Standalone judging needs a quote and cannot mix member recipients.
        let judge_allowed = !slash_line
            && !prior
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Mention(n) if n == "judge"))
            && (mode == Some("debate")
                || mode.is_none()
                    && !meta.quote.is_empty()
                    && !prior
                        .iter()
                        .any(|t| matches!(&t.kind, TokenKind::Mention(_))));
        let mut candidates = vec![];
        if prefix.starts_with('@') {
            let mut nicks = if has_all { vec![] } else { ctx.roster.clone() };
            if let Some(pick) = &ctx.pending_pick {
                nicks.sort_by_key(|n| !pick.eligible.contains(n));
            }
            candidates.extend(nicks.iter().map(|n| format!("@{n}")));
            if !ctx.roster.is_empty() && !slash_line && !has_mentions {
                candidates.push("@all".into());
            }
            if judge_allowed {
                candidates.push("@judge".into());
            }
        } else if prefix.starts_with('-') {
            let before = &raw[..start];
            let prior = tokenize(before).unwrap_or_default();
            let all = prior
                .iter()
                .rposition(|t| matches!(&t.kind, TokenKind::Mention(n) if n == "all"));
            if let Some(all) = all
                && prior[all + 1..].iter().all(|t| {
                    matches!(t.kind, TokenKind::Exclusion(_))
                        || before[t.span.start..t.span.end].trim().is_empty()
                })
            {
                candidates.extend(
                    ctx.roster
                        .iter()
                        .filter(|n| {
                            !prior.iter().any(
                                |t| matches!(&t.kind, TokenKind::Exclusion(x) if x == n.as_str()),
                            )
                        })
                        .map(|n| format!("-{n}")),
                );
            }
        } else if prefix.starts_with('$') {
            candidates.extend(
                crate::typeahead::MODES
                    .iter()
                    .filter(|(token, _)| crate::typeahead::enabled(token))
                    .map(|(name, _)| name.to_string()),
            );
            // Preserve variable completion for an explicit uppercase prefix.
            if prefix
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_uppercase())
            {
                candidates.extend(
                    [
                        "$ROUNDS",
                        "$ROUNDS=",
                        "$GRILL_ROUNDS",
                        "$GRILL_ROUNDS=",
                        "$CENSUS_PASSES",
                        "$CENSUS_PASSES=",
                    ]
                    .map(str::to_string),
                );
            }
        } else if prefix.starts_with('.') && start == 0 {
            candidates.extend(
                crate::typeahead::SYSTEM_COMMANDS
                    .iter()
                    .map(|(name, _)| name.to_string()),
            );
        } else if prefix.starts_with('/') && start == 0 {
            candidates.extend(
                squad_protocol::parser::SLASH_COMMANDS
                    .iter()
                    .map(|(name, _)| name.to_string()),
            );
        }
        if starters && candidates.is_empty() && !prefix.starts_with(['@', '$', '/', '-']) {
            let has_mode = prior
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Command(_)));
            if !ctx.roster.is_empty() && !has_mentions && !slash_line {
                candidates.push("@all".into());
            }
            candidates.extend(
                ctx.roster
                    .iter()
                    .filter(|nick| {
                        !has_all && !prior.iter().any(|t| {
                            matches!(&t.kind, TokenKind::Mention(name) if name == nick.as_str())
                        })
                    })
                    .map(|n| format!("@{n}")),
            );
            if judge_allowed {
                candidates.push("@judge".into());
            }
            if !has_mode && !slash_line {
                candidates.extend(
                    crate::typeahead::MODES
                        .iter()
                        .filter(|(token, _)| crate::typeahead::enabled(token))
                        .map(|(name, _)| name.to_string()),
                );
            }
            if start == 0 {
                candidates.extend(
                    squad_protocol::parser::SLASH_COMMANDS
                        .iter()
                        .map(|(name, _)| name.to_string()),
                );
            }
        }
        let query = prefix.to_lowercase();
        candidates.retain(|s| crate::typeahead::rank(s, &query).is_some());
        candidates.sort_by_key(|s| crate::typeahead::rank(s, &query));
        Self {
            items: candidates,
            selected: 0,
            replace: start..end,
            descriptions: vec![],
        }
    }
    pub fn move_selection(&mut self, down: bool) {
        if !self.items.is_empty() {
            self.selected = if down {
                (self.selected + 1) % self.items.len()
            } else {
                (self.selected + self.items.len() - 1) % self.items.len()
            };
        }
    }
    pub fn replacement(&self) -> Option<String> {
        self.items.get(self.selected).map(|s| format!("{s} "))
    }
}
pub fn render(view: &CompletionState, area: Rect, buf: &mut Buffer) {
    Clear.render(area, buf);
    let first = view.selected.saturating_sub(5);
    for (row, item) in view
        .items
        .iter()
        .enumerate()
        .skip(first)
        .take(6.min(usize::from(area.height)))
    {
        let style = if row == view.selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        buf.set_stringn(
            area.x,
            area.y + (row - first) as u16,
            item,
            usize::from(area.width),
            style,
        );
        let offset = item.chars().count() as u16 + 2;
        if offset < area.width
            && let Some(description) = view.descriptions.get(row)
        {
            buf.set_stringn(
                area.x + offset,
                area.y + (row - first) as u16,
                description,
                usize::from(area.width - offset),
                style.add_modifier(Modifier::DIM),
            );
        }
    }
}
