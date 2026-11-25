//! Shared-floor grill transcript. All counters and completion reasons are core-owned.
use super::tournament::paint::{Painter, heading};
use crate::fx::Palette;
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::{ContestId, ContestKind, GenericContestState};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exchange {
    pub question: String,
    pub answer: String,
    pub plan_changes: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnContent {
    Questions(Vec<Exchange>),
    /// The exact committed pass text is retained, without reinterpretation.
    Pass(String),
    Skipped(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub round: u32,
    pub griller: String,
    pub angle: String,
    pub status: String,
    pub content: TurnContent,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanVersion {
    pub version: u32,
    pub text: String,
    /// None uses the default: newest expanded, all earlier versions collapsed.
    pub expanded: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrillView {
    pub id: Option<ContestId>,
    pub leader: String,
    pub grillers: Vec<String>,
    pub focus: String,
    pub state: String,
    pub reason: Option<String>,
    pub total_rounds: u32,
    pub current_round: u32,
    pub current_turn: String,
    pub questions_asked: u32,
    pub passed: u32,
    pub turns: Vec<Turn>,
    /// Committed chronological versions. Rebuilding/failure never clears these.
    pub plans: Vec<PlanVersion>,
    pub final_plan: Option<String>,
    pub appendix_expanded: bool,
    pub expanded: bool,
    pub scroll: u16,
}
impl GrillView {
    pub fn new(leader: impl Into<String>, grillers: Vec<String>) -> Self {
        Self {
            leader: leader.into(),
            grillers,
            expanded: true,
            ..Default::default()
        }
    }
    pub fn apply_state(&mut self, state: &GenericContestState) -> bool {
        if state.kind != ContestKind::Grill || self.id.is_some_and(|id| id != state.contest_id) {
            return false;
        }
        self.id = Some(state.contest_id);
        self.state = state.status.to_string();
        self.reason = state.terminal_reason.as_ref().map(ToString::to_string);
        true
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrillRects {
    pub banner: Rect,
    pub rounds: Vec<(u32, Rect)>,
    pub turns: Vec<(u32, String, Rect)>,
    pub plan_badges: Vec<(u32, Rect)>,
    pub final_plan: Rect,
    pub survived: Rect,
    pub appendix: Rect,
}
fn exchange(p: &mut Painter<'_>, item: &Exchange) {
    p.text(&format!("Q: {}", item.question), Style::default());
    p.text(&format!("A: {}", item.answer), Style::default());
    p.text(
        &format!("Plan changes: {}", item.plan_changes),
        Style::default(),
    );
}
pub fn render(view: &GrillView, area: Rect, buf: &mut Buffer, palette: &Palette) -> GrillRects {
    let mut p = Painter::new(area, buf, view.scroll);
    let mut rects = GrillRects {
        banner: p.text(
            &format!(
                "{} GRILL · {}",
                if view.expanded { "▾" } else { "▸" },
                view.state
            ),
            heading(palette),
        ),
        ..Default::default()
    };
    if !view.expanded {
        return rects;
    }
    if let Some(id) = view.id {
        p.text(&format!("ID {id}"), Style::default());
    }
    p.text(
        &format!(
            "Leader @{} · grillers {}",
            view.leader,
            view.grillers.join(", ")
        ),
        Style::default(),
    );
    p.text(&view.focus, Style::default());
    p.text("Shared floor: leader and grillers receive committed plans, questions and answers. Non-participants receive nothing.", Style::default());
    p.text(
        &format!(
            "GRILL_ROUNDS {} · round {} · turn {} · plan v{}",
            view.total_rounds,
            view.current_round,
            view.current_turn,
            view.plans.last().map_or(0, |v| v.version)
        ),
        Style::default(),
    );
    p.text(
        &format!(
            "questions asked {} · passed {}",
            view.questions_asked, view.passed
        ),
        Style::default(),
    );
    let mut previous_round = None;
    for (i, turn) in view.turns.iter().enumerate() {
        if previous_round != Some(turn.round) {
            let r = p.text(&format!("GRILL ROUND {}", turn.round), heading(palette));
            if !r.is_empty() {
                rects.rounds.push((turn.round, r));
            }
            previous_round = Some(turn.round);
        }
        let r = p.text(
            &format!(
                "{} ▸ grills ▸ {} · {}",
                turn.griller, view.leader, turn.status
            ),
            Style::default().fg(palette.member(&turn.griller, i)),
        );
        if !r.is_empty() {
            rects.turns.push((turn.round, turn.griller.clone(), r));
        }
        p.text(&format!("Angle: {}", turn.angle), Style::default());
        match &turn.content {
            TurnContent::Questions(items) => {
                for item in items {
                    exchange(&mut p, item);
                }
            }
            TurnContent::Pass(text) => {
                p.text(&format!("PASS · {text}"), Style::default());
            }
            TurnContent::Skipped(reason) => {
                p.text(&format!("SKIPPED · {reason}"), Style::default());
            }
        }
    }
    for (i, plan) in view.plans.iter().enumerate() {
        let expanded = plan.expanded.unwrap_or(i + 1 == view.plans.len());
        let r = p.text(
            &format!(
                "{} PLAN v{}{}",
                if expanded { "▾" } else { "▸" },
                plan.version,
                if i + 1 == view.plans.len() {
                    " · latest"
                } else {
                    ""
                }
            ),
            heading(palette),
        );
        if !r.is_empty() {
            rects.plan_badges.push((plan.version, r));
        }
        if expanded {
            p.text(&plan.text, Style::default());
        }
    }
    if let Some(plan) = &view.final_plan {
        rects.final_plan = p.text("FINAL PLAN", heading(palette));
        p.text(plan, Style::default());
    }
    if view.state == "COMPLETE" && view.reason.as_deref() == Some("survived") {
        rects.survived = p.text("SURVIVED", heading(palette));
    } else if let Some(reason) = &view.reason {
        p.text(reason, Style::default());
    }
    rects.appendix = p.text(
        &format!(
            "{} QUESTION-AND-ANSWER APPENDIX",
            if view.appendix_expanded { "▾" } else { "▸" }
        ),
        heading(palette),
    );
    if view.appendix_expanded {
        for turn in &view.turns {
            if let TurnContent::Questions(items) = &turn.content {
                p.text(
                    &format!(
                        "Round {} · @{} → @{}",
                        turn.round, turn.griller, view.leader
                    ),
                    heading(palette),
                );
                for item in items {
                    exchange(&mut p, item);
                }
            }
        }
    }
    rects
}
