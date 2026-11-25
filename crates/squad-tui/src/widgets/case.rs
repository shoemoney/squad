//! Standalone judge rulings and verdict-linked conversations, without winner cards.
use super::banner::Canvas;
use crate::fx::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use squad_protocol::{CaseFollowup, CaseMaterial, CaseOpened, CaseRuling, EventId};

#[derive(Clone, Debug)]
pub enum CaseEntry {
    Ruling(CaseRuling),
    Followup { event: CaseFollowup, text: String },
}
#[derive(Clone, Debug)]
pub struct CaseView {
    pub opened: CaseOpened,
    pub original_verdict: Option<EventId>,
    pub entries: Vec<CaseEntry>,
    pub collapsed: bool,
    pub scroll: usize,
    /// Verbatim core warning, including concrete-model conflict resolution.
    pub warning: Option<String>,
    pub failure: Option<String>,
}
impl From<&CaseOpened> for CaseView {
    fn from(opened: &CaseOpened) -> Self {
        Self {
            opened: opened.clone(),
            original_verdict: None,
            entries: vec![],
            collapsed: false,
            scroll: 0,
            warning: None,
            failure: None,
        }
    }
}
impl CaseView {
    pub fn add_ruling(&mut self, ruling: &CaseRuling) -> bool {
        if ruling.case_id != self.opened.case_id {
            return false;
        }
        if let Some(entry) = self.entries.iter_mut().find(|entry| matches!(entry, CaseEntry::Ruling(r) if r.judge_invocation_id == ruling.judge_invocation_id)) {
            *entry = CaseEntry::Ruling(ruling.clone());
        } else { self.entries.push(CaseEntry::Ruling(ruling.clone())); }
        true
    }
    /// Follow-up text is resolved by the room from the event's user_event_id.
    pub fn add_followup(&mut self, event: &CaseFollowup, text: &str) -> bool {
        if event.case_id != self.opened.case_id {
            return false;
        }
        if !self.entries.iter().any(|entry| matches!(entry, CaseEntry::Followup { event: existing, .. } if existing.user_event_id == event.user_event_id)) {
            self.entries.push(CaseEntry::Followup { event: event.clone(), text: text.into() });
        }
        true
    }
}
#[derive(Clone, Debug, Default)]
pub struct CaseRects {
    pub heading: Rect,
    pub material: Vec<Rect>,
    pub rulings: Vec<Rect>,
    pub followups: Vec<Rect>,
    pub content_height: usize,
}
pub fn render(view: &CaseView, area: Rect, buf: &mut Buffer, palette: &Palette) -> CaseRects {
    let mut c = Canvas::new(area, buf, view.scroll);
    let mut out = CaseRects::default();
    let accent = Style::default()
        .fg(palette.chrome())
        .add_modifier(Modifier::BOLD);
    let body = Style::default();
    out.heading = c.line(
        &format!(
            "{} Judging · @judge · {}",
            if view.collapsed { "▸" } else { "▾" },
            view.opened.judge_model
        ),
        accent,
    );
    if view.collapsed {
        out.content_height = c.row;
        return out;
    }
    if let Some(warning) = &view.warning {
        c.line(warning, body);
    }
    if view.original_verdict.is_some() {
        c.line("Follow-up to accepted verdict", body);
    }
    match &view.opened.material {
        CaseMaterial::Tournament { material } => {
            c.line(
                &format!(
                    "Tournament · round {} · {}",
                    material.round, material.prompt
                ),
                body,
            );
        }
        CaseMaterial::Standalone { messages, question } => {
            c.line(&format!("Question: {question}"), accent);
            for quote in messages {
                out.material
                    .push(c.line(&format!("{}: {}", quote.author, quote.text), body));
            }
        }
        CaseMaterial::Debate { setup, .. } => {
            c.line(&format!("Debate · {}", setup.topic_text), body);
        }
    }
    for entry in &view.entries {
        match entry {
            CaseEntry::Ruling(ruling) => {
                c.line(
                    &format!(
                        "@judge · {} · {}",
                        ruling.completed.model,
                        super::debate::finish_label(&ruling.completed.finish_reason)
                    ),
                    accent,
                );
                out.rulings.push(c.line(ruling.content.as_str(), body));
            }
            CaseEntry::Followup { event, text } => {
                c.line("Follow-up", accent);
                out.followups.push(c.line(text, body));
                if !view.entries.iter().any(|e| matches!(e, CaseEntry::Ruling(r) if r.judge_invocation_id == event.judge_invocation_id)) { c.line("@judge · awaiting follow-up ruling", body); }
            }
        }
    }
    if view.entries.is_empty() {
        c.line("@judge · awaiting ruling", body);
    }
    if let Some(error) = &view.failure {
        c.line(&format!("Judging failed: {error}"), body);
    }
    if view.original_verdict.is_some()
        || view
            .entries
            .iter()
            .any(|entry| matches!(entry, CaseEntry::Ruling(_)))
    {
        c.line("[Follow up] · quotes this verdict to @judge only", body);
    }
    out.content_height = c.row;
    out
}
