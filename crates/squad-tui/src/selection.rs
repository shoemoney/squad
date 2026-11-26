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

/// Display facts for a selectable message, supplied by the transcript owner.
#[derive(Clone, Debug)]
pub struct MessagePreview {
    pub id: MessageId,
    /// Committed source event; message identity is not a submission reference.
    pub event_id: squad_protocol::EventId,
    pub author: String,
    pub text: String,
}
fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        format!("{}…", flat.chars().take(max - 1).collect::<String>())
    }
}
/// Why a debate-solution selection cannot be submitted yet. The parser stays the
/// authority on wording; these only gate the preview's "ready" state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SolutionGap {
    NeedTwo { selected: usize },
    SameAuthor,
    Unknown(MessageId),
}
#[derive(Clone, Debug)]
pub struct SolutionPreview {
    pub event_id: squad_protocol::EventId,
    pub author: String,
    pub text: String,
    pub id: MessageId,
}
/// Two explicit solutions from different members plus the topic, previewed
/// before submission. The client never substitutes another source.
#[derive(Clone, Debug)]
pub struct DebateSolutionSelection {
    pub topic: String,
    pub solutions: Vec<SolutionPreview>,
    pub gap: Option<SolutionGap>,
}
impl DebateSolutionSelection {
    pub fn resolve(
        state: &SelectionState,
        messages: &[SelectableMessage],
        previews: &[MessagePreview],
        topic: &str,
    ) -> Self {
        let ids = state.eligible(messages, MessageAction::DebateSolution);
        let mut solutions = Vec::new();
        let mut gap = None;
        for id in &ids {
            match previews.iter().find(|p| p.id == *id) {
                Some(p) => solutions.push(SolutionPreview {
                    event_id: p.event_id,
                    author: p.author.clone(),
                    text: clip(&p.text, 160),
                    id: *id,
                }),
                None => gap = gap.or(Some(SolutionGap::Unknown(*id))),
            }
        }
        if gap.is_none() {
            if solutions.len() != 2 {
                gap = Some(SolutionGap::NeedTwo {
                    selected: solutions.len(),
                });
            } else if solutions[0].author == solutions[1].author {
                gap = Some(SolutionGap::SameAuthor);
            }
        }
        Self {
            topic: clip(topic, 160),
            solutions,
            gap,
        }
    }
    pub fn ready(&self) -> bool {
        self.gap.is_none()
    }
    /// `replyTo` carries only debate solution references.
    pub fn meta(&self) -> Option<squad_protocol::parser::SubmissionMeta> {
        self.ready()
            .then(|| squad_protocol::parser::SubmissionMeta {
                quote: vec![],
                reply_to: self.solutions.iter().map(|s| s.event_id).collect(),
            })
    }
    pub fn preview_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("Topic: {}", self.topic)];
        lines.extend(
            self.solutions
                .iter()
                .map(|s| format!("{} — {}", s.author, s.text)),
        );
        lines
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanSourceGap {
    Missing,
    Ambiguous(usize),
}
/// Leader-plan source for `$grill`: exactly one eligible message among the
/// selected and quoted ones, shown with its preview before submission.
#[derive(Clone, Debug)]
pub struct GrillPlanSelection {
    pub source: Option<SolutionPreview>,
    pub gap: Option<PlanSourceGap>,
}
impl GrillPlanSelection {
    pub fn resolve(
        state: &SelectionState,
        messages: &[SelectableMessage],
        quoted: &[MessageId],
        previews: &[MessagePreview],
    ) -> Self {
        let mut ids = state.eligible(messages, MessageAction::GrillPlan);
        for q in quoted {
            if !ids.contains(q)
                && messages.iter().any(|m| {
                    m.id == *q && m.durable && m.actions.contains(&MessageAction::GrillPlan)
                })
            {
                ids.push(*q);
            }
        }
        match ids.as_slice() {
            [] => Self {
                source: None,
                gap: Some(PlanSourceGap::Missing),
            },
            [one] => Self {
                source: previews
                    .iter()
                    .find(|p| p.id == *one)
                    .map(|p| SolutionPreview {
                        event_id: p.event_id,
                        author: p.author.clone(),
                        text: clip(&p.text, 160),
                        id: *one,
                    }),
                gap: None,
            },
            many => Self {
                source: None,
                gap: Some(PlanSourceGap::Ambiguous(many.len())),
            },
        }
    }
    pub fn ready(&self) -> bool {
        self.gap.is_none() && self.source.is_some()
    }
    /// The explicit source travels as the single quoted message.
    pub fn meta(&self) -> Option<squad_protocol::parser::SubmissionMeta> {
        self.source
            .as_ref()
            .map(|s| squad_protocol::parser::SubmissionMeta {
                quote: vec![s.event_id],
                reply_to: vec![],
            })
    }
}
