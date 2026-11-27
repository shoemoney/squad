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
    pub epoch: u64,
    pub cards: BTreeMap<MessageId, ReplyCard>,
    pub anchor: Option<(MessageId, usize)>,
    pub follow: bool,
    pub batch_deadline: Option<Duration>,
    /// Old reply -> its replacement, from `message.superseded`.
    pub superseded: BTreeMap<MessageId, MessageId>,
    // At most one publication entry per retained identity, so memory is bounded
    // by history rather than token count. Independent views can keep a cursor.
    publications: BTreeMap<u64, MessageId>,
    versions: BTreeMap<MessageId, u64>,
    pending: BTreeSet<MessageId>,
    live: BTreeSet<MessageId>,
    pub work: TranscriptWork,

    unseen: BTreeSet<MessageId>,
}
/// Work counters for history bookkeeping regressions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptWork {
    pub publications: usize,
    pub activity_insertions: usize,
    pub flushed_cards: usize,
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
            epoch: 0,
            cards: BTreeMap::new(),
            anchor: None,
            follow: true,
            batch_deadline: None,
            superseded: BTreeMap::new(),
            publications: BTreeMap::new(),
            versions: BTreeMap::new(),
            pending: BTreeSet::new(),
            live: BTreeSet::new(),
            work: TranscriptWork::default(),
            unseen: BTreeSet::new(),
        }
    }
}
impl Transcript {
    pub fn complete(&mut self, id: MessageId, author: Actor, attempt: u32, text: String) {
        if self.cards.get(&id).is_some_and(|c| c.attempt > attempt) {
            return;
        }
        let inserted = !self.cards.contains_key(&id);
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
        self.pending.remove(&id);
        self.live.remove(&id);
        self.note_insertion(id, inserted);
        self.publish(id);
    }
    /// Publish a changed presentation, including state/attempt-only changes.
    pub fn publish(&mut self, id: MessageId) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("presentation revision exhausted");
        if let Some(old) = self.versions.insert(id, self.revision) {
            self.publications.remove(&old);
        }
        self.publications.insert(self.revision, id);
        self.work.publications += 1;
    }
    pub fn changed_since(&self, revision: u64) -> impl Iterator<Item = MessageId> + '_ {
        use std::ops::Bound::{Excluded, Unbounded};
        self.publications
            .range((Excluded(revision), Unbounded))
            .map(|(_, id)| *id)
    }
    fn note_insertion(&mut self, id: MessageId, inserted: bool) {
        if inserted {
            self.work.activity_insertions += 1;
            if !self.follow {
                self.unseen.insert(id);
            }
            // A replacement arriving later can change the old card's byline.
            let old = self
                .superseded
                .iter()
                .filter_map(|(old, replacement)| (*replacement == id).then_some(*old))
                .collect::<Vec<_>>();
            for old in old {
                self.publish(old);
            }
        }
    }
    /// The reducer has accepted a delta. Pending text is not yet presentation.
    pub fn pending_delta(&mut self, id: MessageId, inserted: bool, replaced: bool) {
        self.note_insertion(id, inserted);
        self.pending.insert(id);
        self.live.insert(id);
        if inserted || replaced {
            self.publish(id);
        }
    }
    pub fn finish(&mut self, id: MessageId) {
        self.pending.remove(&id);
        self.live.remove(&id);
        self.publish(id);
    }
    pub fn flush(&mut self, now: Duration) -> bool {
        if self.batch_deadline.is_none_or(|at| at > now) {
            return false;
        }
        self.batch_deadline = None;
        let pending = std::mem::take(&mut self.pending);
        for id in pending {
            if let Some(card) = self.cards.get_mut(&id) {
                card.text.push_str(&std::mem::take(&mut card.pending_text));
                self.work.flushed_cards += 1;
                self.publish(id);
            }
        }
        true
    }
    /// Records that `old` was replaced by `replacement`; the old card stays visible and labelled.
    pub fn supersede(&mut self, old: MessageId, replacement: MessageId) {
        if self.superseded.insert(old, replacement) != Some(replacement) {
            self.publish(old);
        }
    }
    pub fn state_of(&self, id: MessageId) -> Option<CardState> {
        let card = self.cards.get(&id)?;
        Some(card.state(self.superseded.contains_key(&id)))
    }
    /// Messages that appeared while following was suspended.
    pub fn new_activity(&self) -> usize {
        self.unseen.len()
    }
    /// Following clears only unread identities; no retained-history traversal.
    pub fn sync_activity(&mut self) {
        if self.follow {
            self.unseen.clear();
        }
    }
    pub fn pause_follow(&mut self) {
        self.sync_activity();
        self.follow = false;
    }
    pub(crate) fn unread_ids(&self) -> BTreeSet<MessageId> {
        self.unseen.clone()
    }
    pub(crate) fn restore_unread(&mut self, unread: BTreeSet<MessageId>) {
        self.unseen = unread
            .into_iter()
            .filter(|id| self.cards.contains_key(id))
            .collect();
    }
    pub fn resume_follow(&mut self) {
        self.follow = true;
        self.sync_activity();
    }
    pub fn streaming(&self) -> bool {
        !self.live.is_empty()
    }
    pub fn clear(&mut self) {
        *self = Self {
            epoch: self.epoch.wrapping_add(1),
            follow: true,
            ..Self::default()
        };
    }
}

impl ReplyCard {
    pub fn state(&self, superseded: bool) -> CardState {
        if superseded {
            CardState::Superseded
        } else if !self.complete {
            CardState::Streaming
        } else {
            match self.terminal {
                Some(WorkStatus::Failed) => CardState::Incomplete(false),
                Some(WorkStatus::Cancelled) => CardState::Incomplete(true),
                _ => CardState::Complete,
            }
        }
    }
}
