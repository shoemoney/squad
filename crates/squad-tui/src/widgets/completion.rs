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
}
impl CompletionState {
    pub fn at(raw: &str, caret: usize, ctx: &ParseContext, meta: &SubmissionMeta) -> Self {
        if !raw.is_char_boundary(caret) {
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
            Err(_) => prefix_tokens,
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
        let mut candidates = vec![];
        if prefix.starts_with('@') {
            let mut nicks = ctx.roster.clone();
            if let Some(pick) = &ctx.pending_pick {
                nicks.sort_by_key(|n| !pick.eligible.contains(n));
            }
            candidates.extend(nicks.iter().map(|n| format!("@{n}")));
            candidates.push("@all".into());
            if !meta.quote.is_empty()
                || ctx.active_contest.is_some()
                || ctx
                    .records
                    .iter()
                    .any(|r| r.kind.as_str() == "debate.verdict")
            {
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
                [
                    "$debate",
                    "$tournament",
                    "$grill",
                    "$census",
                    "$crosscheck",
                    "$survey",
                    "$probe",
                    "$ROUNDS",
                    "$ROUNDS=",
                    "$GRILL_ROUNDS",
                    "$GRILL_ROUNDS=",
                    "$CENSUS_PASSES",
                    "$CENSUS_PASSES=",
                ]
                .map(str::to_owned),
            );
        } else if prefix.starts_with('/') && start == 0 {
            candidates.extend(
                [
                    "/set",
                    "/stop",
                    "/judge",
                    "/regen",
                    "/clear",
                    "/who",
                    "/help",
                    "/export",
                    "/continue",
                    "/accept",
                ]
                .map(str::to_owned),
            );
        }
        candidates.retain(|s| s.starts_with(prefix));
        Self {
            items: candidates,
            selected: 0,
            replace: start..end,
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
    }
}
