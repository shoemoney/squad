//! Stable reply identities, attempt replacement and 50 ms token batching.
use squad_protocol::*;
use std::{collections::BTreeMap, time::Duration};
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
}
impl Default for Transcript {
    fn default() -> Self {
        Self {
            revision: 0,
            cards: BTreeMap::new(),
            anchor: None,
            follow: true,
            batch_deadline: None,
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
