//! Literal search in durable source, with explicit incomplete-history coverage.
use crate::markdown::sanitize;
use squad_protocol::{Cursor, MessageId};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct SearchDocument {
    pub id: MessageId,
    pub source: String,
    pub durable: bool,
    pub collapsed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub id: MessageId,
    pub bytes: Range<usize>,
    pub graphemes: Range<usize>,
    pub reveal: bool,
}
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current: Option<usize>,
    pub older_unsearched: bool,
    pub before: Option<Cursor>,
}
impl SearchState {
    /// Replaces matches for a refreshed loaded window; repeated pages never duplicate hits.
    pub fn refresh(
        &mut self,
        documents: &[SearchDocument],
        older_unsearched: bool,
        before: Option<Cursor>,
    ) {
        let previous = self.current.and_then(|i| self.matches.get(i)).cloned();
        self.matches.clear();
        self.older_unsearched = older_unsearched;
        self.before = before;
        if !self.query.is_empty() {
            for doc in documents.iter().filter(|doc| doc.durable) {
                let source = sanitize(&doc.source);
                let boundaries: Vec<usize> = source
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .chain(std::iter::once(source.len()))
                    .collect();
                for (start, matched) in source.match_indices(&self.query) {
                    let end = start + matched.len();
                    self.matches.push(SearchMatch {
                        id: doc.id,
                        bytes: start..end,
                        graphemes: boundaries
                            .partition_point(|b| *b <= start)
                            .saturating_sub(1)
                            ..boundaries.partition_point(|b| *b < end),
                        reveal: doc.collapsed,
                    });
                }
            }
        }
        self.current = previous
            .and_then(|old| {
                self.matches
                    .iter()
                    .position(|m| m.id == old.id && m.bytes == old.bytes)
            })
            .or(if self.matches.is_empty() {
                None
            } else {
                Some(0)
            });
    }
    pub fn navigate(&mut self, backwards: bool) -> Option<&SearchMatch> {
        if self.matches.is_empty() {
            self.current = None;
            return None;
        }
        let n = self.matches.len();
        let i = self
            .current
            .map_or(if backwards { 0 } else { n - 1 }, |i| i.min(n - 1));
        let next = if backwards {
            (i + n - 1) % n
        } else {
            (i + 1) % n
        };
        self.current = Some(next);
        self.matches.get(next)
    }
    pub fn status(&self) -> String {
        format!(
            "Find: {} · {}/{}{}",
            sanitize(&self.query),
            self.current.map_or(0, |i| i + 1),
            self.matches.len(),
            if self.older_unsearched {
                " · older history unsearched"
            } else {
                ""
            }
        )
    }
    pub fn needs_page(&self) -> bool {
        !self.query.is_empty() && self.older_unsearched
    }
}
