//! Canonical message focus and selection, independent of visual wrapping.
use squad_protocol::MessageId;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageAction {
    Quote,
    Regenerate,
    DebateSolution,
    GrillPlan,
    CopyMessage,
    CopyCode,
}
impl MessageAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quote => "Quote",
            Self::Regenerate => "Regenerate",
            Self::DebateSolution => "Debate solution",
            Self::GrillPlan => "Grill plan",
            Self::CopyMessage => "Copy message",
            Self::CopyCode => "Copy code block",
        }
    }
}
/// Eligibility is supplied by the owning contract/reducer, not inferred from colour or text.
#[derive(Clone, Debug)]
pub struct SelectableMessage {
    pub id: MessageId,
    pub durable: bool,
    pub actions: Vec<MessageAction>,
}
#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    pub focused: Option<MessageId>,
    selected: BTreeSet<MessageId>,
}
impl SelectionState {
    pub fn selected(&self) -> impl Iterator<Item = MessageId> + '_ {
        self.selected.iter().copied()
    }
    pub fn is_selected(&self, id: MessageId) -> bool {
        self.selected.contains(&id)
    }
    pub fn toggle(&mut self, message: &SelectableMessage) -> bool {
        if !message.durable {
            return false;
        }
        if !self.selected.remove(&message.id) {
            self.selected.insert(message.id);
        }
        true
    }
    pub fn navigate(&mut self, messages: &[SelectableMessage], delta: isize) {
        if messages.is_empty() {
            self.focused = None;
            return;
        }
        let current = self
            .focused
            .and_then(|id| messages.iter().position(|m| m.id == id));
        let next = current.map_or(if delta < 0 { messages.len() - 1 } else { 0 }, |i| {
            i.saturating_add_signed(delta).min(messages.len() - 1)
        });
        self.focused = Some(messages[next].id);
    }
    pub fn eligible(
        &self,
        messages: &[SelectableMessage],
        action: MessageAction,
    ) -> Vec<MessageId> {
        messages
            .iter()
            .filter(|m| self.selected.contains(&m.id) && m.durable && m.actions.contains(&action))
            .map(|m| m.id)
            .collect()
    }
    /// Call on authoritative deletion/reset, not on paging a message out of memory.
    pub fn remove(&mut self, ids: &[MessageId]) {
        self.selected.retain(|id| !ids.contains(id));
        if self.focused.is_some_and(|id| ids.contains(&id)) {
            self.focused = None;
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
