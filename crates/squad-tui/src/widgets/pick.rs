//! Question-first selection presentation. No expiry, parsing, submission or local admission.
use super::tournament::paint::{Painter, heading};
use crate::fx::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};
use squad_protocol::{
    ContestAnswer, ContestId, ContestKind, ContestSelectionState, ContestSelectionStatus,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    Debate,
    Grill,
}
impl PickKind {
    pub fn banner(self) -> &'static str {
        match self {
            Self::Debate => "CHOOSE YOUR DEBATE BROS",
            Self::Grill => "CHOOSE WHO GETS GRILLED",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Debate => "Type two names, e.g. @grok @gemini · Esc to cancel",
            Self::Grill => "Type one name, e.g. @grok · Esc to cancel",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickStatus {
    Collecting,
    Pending,
    Accepted,
    Cancelled,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub nick: String,
    pub eligible: bool,
    pub status: String,
    pub answer: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickView {
    pub id: Option<ContestId>,
    pub kind: PickKind,
    pub status: PickStatus,
    pub question: String,
    pub candidates: Vec<Candidate>,
    /// Core-supplied validation feedback, displayed verbatim.
    pub feedback: Option<String>,
    pub restored: bool,
    pub scroll: u16,
}
impl PickView {
    pub fn new(kind: PickKind, question: impl Into<String>, candidates: Vec<Candidate>) -> Self {
        Self {
            id: None,
            kind,
            status: PickStatus::Collecting,
            question: question.into(),
            candidates,
            feedback: None,
            restored: false,
            scroll: 0,
        }
    }
    /// Restore ordinary answers in addressed order. Authoritative eligible nicks must
    /// come from pick_requested / the room snapshot, never guessed from answer text.
    pub fn from_selection(
        state: &ContestSelectionState,
        eligible: &[String],
        question: impl Into<String>,
    ) -> Option<Self> {
        let kind = match state.kind {
            ContestKind::Debate => PickKind::Debate,
            ContestKind::Grill => PickKind::Grill,
            _ => return None,
        };
        let candidates = state
            .addressed
            .iter()
            .map(|nick| {
                let answer: Option<&ContestAnswer> = state.answers.iter().find(|a| a.nick == *nick);
                Candidate {
                    nick: nick.to_string(),
                    eligible: eligible.iter().any(|n| n == nick.as_str()),
                    status: if answer.is_some() {
                        "answered"
                    } else if state.state == ContestSelectionStatus::CollectingAnswers {
                        "awaiting answer"
                    } else {
                        "did not answer"
                    }
                    .into(),
                    answer: answer.map_or_else(String::new, |a| a.text.clone()),
                }
            })
            .collect();
        let mut view = Self::new(kind, question, candidates);
        view.id = Some(state.contest_id);
        view.status = match state.state {
            ContestSelectionStatus::CollectingAnswers => PickStatus::Collecting,
            ContestSelectionStatus::AwaitingPick => PickStatus::Pending,
            ContestSelectionStatus::Failed => PickStatus::Failed,
            ContestSelectionStatus::Stopped => PickStatus::Cancelled,
        };
        view.restored = true;
        Some(view)
    }
    pub fn placeholder(&self) -> Option<&'static str> {
        (self.status == PickStatus::Pending).then(|| self.kind.hint())
    }
    pub fn wrong_pick_feedback(&self) -> String {
        let names = self
            .candidates
            .iter()
            .filter(|c| c.eligible)
            .map(|c| format!("@{}", c.nick))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Pick exactly {} of: {names}.",
            if self.kind == PickKind::Debate {
                "two"
            } else {
                "one"
            }
        )
    }
    pub fn ineligible_feedback(nick: &str) -> String {
        format!("@{nick} did not answer, so it cannot be picked.")
    }
    pub fn insufficient_feedback(answered: usize, addressed: usize) -> String {
        format!("Not enough answers to start. {answered} of {addressed} members answered.")
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickRects {
    pub banner: Rect,
    /// Nick, owned marker cell, and chrome row; answer bodies excluded.
    pub candidates: Vec<(String, Rect, Rect)>,
    pub answers: Vec<(String, Rect)>,
    pub status: Rect,
}
pub fn render(view: &PickView, area: Rect, buf: &mut Buffer, palette: &Palette) -> PickRects {
    let mut p = Painter::new(area, buf, view.scroll);
    let pending = view.status == PickStatus::Pending;
    let label = match view.status {
        PickStatus::Collecting => "COLLECTING_ANSWERS",
        PickStatus::Pending => view.kind.banner(),
        PickStatus::Accepted => "PICK ACCEPTED",
        PickStatus::Cancelled => "STOPPED · pick_cancelled",
        PickStatus::Failed => "FAILED",
    };
    let mut rects = PickRects {
        banner: p.text(label, heading(palette)),
        ..Default::default()
    };
    if pending {
        p.text(view.kind.hint(), Style::default());
    }
    p.text(&view.question, Style::default());
    rects.status = p.text(
        if pending {
            if view.restored {
                "pick pending · restored"
            } else {
                "pick pending"
            }
        } else {
            "Ordinary answers"
        },
        Style::default(),
    );
    for (i, c) in view.candidates.iter().enumerate() {
        let highlighted = pending && c.eligible;
        let style = Style::default()
            .fg(palette.member(&c.nick, i))
            .add_modifier(if highlighted {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let r = p.text(
            &format!(
                "{} @{} · {}{}",
                if highlighted { "▸" } else { " " },
                c.nick,
                c.status,
                if highlighted { " · eligible" } else { "" }
            ),
            style,
        );
        if highlighted && !r.is_empty() {
            rects
                .candidates
                .push((c.nick.clone(), Rect::new(r.x, r.y, 1, 1), r));
        }
        if !c.answer.is_empty() {
            let r = p.text(&c.answer, Style::default());
            if !r.is_empty() {
                rects.answers.push((c.nick.clone(), r));
            }
        }
    }
    if let Some(feedback) = &view.feedback {
        p.text(feedback, Style::default());
    }
    rects
}
