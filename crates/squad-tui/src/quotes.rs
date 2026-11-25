//! Ordered, explicit attachments. Inspection never sends a request.
use squad_protocol::{EventId, parser::SubmissionMeta};
#[derive(Debug, Clone, Default)]
pub struct Quotes {
    pub ids: Vec<EventId>,
    pub reply_to: Vec<EventId>,
}
impl Quotes {
    pub fn toggle(&mut self, id: EventId) {
        if let Some(i) = self.ids.iter().position(|q| *q == id) {
            self.ids.remove(i);
        } else if self.ids.len() < 12 {
            self.ids.push(id);
        }
    }
    pub fn metadata(&self) -> SubmissionMeta {
        SubmissionMeta {
            quote: self.ids.clone(),
            reply_to: self.reply_to.clone(),
        }
    }
    pub fn clear(&mut self) {
        self.ids.clear();
        self.reply_to.clear();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotePreview {
    pub id: EventId,
    pub author: String,
    pub text: String,
    pub available: bool,
}
impl Quotes {
    pub fn previews(&self, context: &squad_protocol::parser::ParseContext) -> Vec<QuotePreview> {
        self.ids
            .iter()
            .map(|id| match context.record(*id) {
                Some(record) => QuotePreview {
                    id: *id,
                    author: record.author.to_string(),
                    text: record.content.clone(),
                    available: record.eligible && !record.superseded,
                },
                None => QuotePreview {
                    id: *id,
                    author: String::new(),
                    text: "Quote unavailable".into(),
                    available: false,
                },
            })
            .collect()
    }
}
