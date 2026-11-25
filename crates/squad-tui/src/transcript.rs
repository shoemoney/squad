//! Stable reply identities, attempt replacement and 50 ms token batching.
use squad_protocol::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
#[derive(Debug, Clone)]
pub struct ReplyCard {
    pub id: MessageId,
    pub work_id: Option<WorkId>,
    pub terminal: Option<WorkStatus>,
    pub author: Actor,
    pub attempt: u32,
    pub text: String,
    pub pending_text: String,
    pub next_chunk: u64,
    pub complete: bool,
}
#[derive(Debug)]
pub struct Transcript {
    pub revision: u64,
    pub cards: BTreeMap<MessageId, ReplyCard>,
    pub anchor: Option<(MessageId, usize)>,
    pub follow: bool,
    pub batch_deadline: Option<Duration>,
    /// Old reply -> its replacement, from `message.superseded`.
    pub superseded: BTreeMap<MessageId, MessageId>,
    seen: BTreeSet<MessageId>,
    unseen: BTreeSet<MessageId>,
}
/// How a reply card is presented: terminal state is never inferred from colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardState {
    Streaming,
    Complete,
    /// Failed or cancelled with partial output kept.
    Incomplete(bool),
    Superseded,
}
impl CardState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Complete => "complete",
            Self::Incomplete(false) => "incomplete · failed",
            Self::Incomplete(true) => "incomplete · cancelled",
            Self::Superseded => "superseded",
        }
    }
}
impl Default for Transcript {
    fn default() -> Self {
        Self {
            revision: 0,
            cards: BTreeMap::new(),
            anchor: None,
            follow: true,
            batch_deadline: None,
            superseded: BTreeMap::new(),
            seen: BTreeSet::new(),
            unseen: BTreeSet::new(),
        }
    }
}
impl Transcript {
    pub fn complete(&mut self, id: MessageId, author: Actor, attempt: u32, text: String) {
        if self.cards.get(&id).is_some_and(|c| c.attempt > attempt) {
            return;
        }
        self.revision = self.revision.wrapping_add(1);
        self.cards.insert(
            id,
            ReplyCard {
                id,
                work_id: self.cards.get(&id).and_then(|c| c.work_id),
                terminal: Some(WorkStatus::Completed),
                author,
                attempt,
                text,
                pending_text: String::new(),
                next_chunk: 0,
                complete: true,
            },
        );
    }
    pub fn flush(&mut self, now: Duration) -> bool {
        if self.batch_deadline.is_none_or(|at| at > now) {
            return false;
        }
        self.batch_deadline = None;
        self.revision = self.revision.wrapping_add(1);
        for card in self.cards.values_mut() {
            card.text.push_str(&std::mem::take(&mut card.pending_text));
        }
        true
    }
    /// Records that `old` was replaced by `replacement`; the old card stays visible and labelled.
    pub fn supersede(&mut self, old: MessageId, replacement: MessageId) {
        if self.superseded.insert(old, replacement) != Some(replacement) {
            self.revision = self.revision.wrapping_add(1);
        }
    }
    pub fn state_of(&self, id: MessageId) -> Option<CardState> {
        let card = self.cards.get(&id)?;
        Some(if self.superseded.contains_key(&id) {
            CardState::Superseded
        } else if !card.complete {
            CardState::Streaming
        } else {
            match &card.terminal {
                Some(WorkStatus::Failed) => CardState::Incomplete(false),
                Some(WorkStatus::Cancelled) => CardState::Incomplete(true),
                _ => CardState::Complete,
            }
        })
    }
    /// Messages that appeared while following was suspended.
    pub fn new_activity(&self) -> usize {
        self.unseen.len()
    }
    /// Call once per frame/tick. While following, everything counts as seen.
    pub fn sync_activity(&mut self) {
        if self.follow {
            self.unseen.clear();
            self.seen.extend(self.cards.keys().copied());
            return;
        }
        for id in self.cards.keys() {
            if self.seen.insert(*id) {
                self.unseen.insert(*id);
            }
        }
    }
    pub fn resume_follow(&mut self) {
        self.follow = true;
        self.sync_activity();
    }
    pub fn streaming(&self) -> bool {
        self.cards.values().any(|c| !c.complete)
    }
    pub fn clear(&mut self) {
        *self = Self {
            follow: true,
            ..Self::default()
        };
    }
}
