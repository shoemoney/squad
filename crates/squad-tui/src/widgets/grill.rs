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
    pub question_turns: std::collections::BTreeMap<squad_protocol::EventId, usize>,
    pub state_version: u64,
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

impl GrillView {
    /// Consume only committed typed payloads. Session owns event deduplication
    /// and suppresses returned effects during replay.
    pub fn apply_event(
        &mut self,
        event: &squad_protocol::CommittedEnvelope,
    ) -> Option<crate::fx::FxCue> {
        use crate::fx::FxCue;
        use squad_protocol::*;
        let id = match &event.content {
            EnvelopePayload::GrillSetup(v) => v.contest_id,
            EnvelopePayload::GrillState(v) => v.contest_id,
            EnvelopePayload::GrillQuestion(v) => v.contest_id,
            EnvelopePayload::GrillAnswer(v) => v.contest_id,
            EnvelopePayload::GrillPass(v) => v.contest_id,
            EnvelopePayload::GrillPlan(v) => v.contest_id,
            EnvelopePayload::GrillComplete(v) => v.contest_id,
            _ => return None,
        };
        if self.id.is_some_and(|old| old != id) {
            return None;
        }
        self.id = Some(id);
        let effect_id: EventId = id.to_string().parse().expect("UUID identity");
        match &event.content {
            EnvelopePayload::GrillSetup(v) => {
                self.leader = v.leader.to_string();
                self.grillers = v.grillers.iter().map(ToString::to_string).collect();
                self.focus = v.focus.clone();
                self.total_rounds = v.total_rounds;
            }
            EnvelopePayload::GrillState(v) if v.state_version > self.state_version => {
                self.state_version = v.state_version;
                self.state = v.state.as_str().to_owned();
                self.reason = v.terminal_reason.clone();
                self.current_round = v.round;
                self.questions_asked = v.questions_asked;
                self.passed = v.passed;
                let nick = v.griller_order.get(v.turn as usize)?;
                let changed = self.current_turn != nick.as_str();
                self.current_turn = nick.to_string();
                if v.state == GrillStatus::Complete
                    && v.terminal_reason.as_deref() == Some("survived")
                {
                    return Some(FxCue::GrillSurvived {
                        contest_id: effect_id,
                    });
                }
                if changed && v.state == GrillStatus::Questioning {
                    return Some(FxCue::GrillTurnChanged {
                        contest_id: effect_id,
                        member: nick.clone(),
                    });
                }
            }
            EnvelopePayload::GrillQuestion(v) => {
                if self.question_turns.contains_key(&event.base.id) {
                    return None;
                }
                self.question_turns.insert(event.base.id, self.turns.len());
                self.turns.push(Turn {
                    round: v.round,
                    griller: v.griller.to_string(),
                    angle: v.angle.clone(),
                    status: "awaiting answer".into(),
                    content: TurnContent::Questions(
                        v.questions
                            .iter()
                            .map(|q| Exchange {
                                question: q.clone(),
                                ..Default::default()
                            })
                            .collect(),
                    ),
                });
            }
            EnvelopePayload::GrillAnswer(v) => {
                let turn = self
                    .turns
                    .get_mut(*self.question_turns.get(&v.question_event_id)?)?;
                if let TurnContent::Questions(items) = &mut turn.content {
                    for answer in &v.answers {
                        let item = items.get_mut(answer.question_index as usize)?;
                        item.answer = answer.answer.clone();
                        item.plan_changes = answer.plan_changes.clone();
                    }
                    turn.status = "complete".into();
                }
            }
            EnvelopePayload::GrillPass(v) => self.turns.push(Turn {
                round: v.round,
                griller: v.griller.to_string(),
                angle: String::new(),
                status: "complete".into(),
                content: match v.reason {
                    GrillPassReason::Pass => TurnContent::Pass("PASS".into()),
                    GrillPassReason::Timeout => TurnContent::Skipped("timeout".into()),
                    GrillPassReason::Error => TurnContent::Skipped("error".into()),
                },
            }),
            EnvelopePayload::GrillPlan(v) => {
                if self.plans.iter().any(|p| p.version == v.version) {
                    return None;
                }
                self.plans.push(PlanVersion {
                    version: v.version,
                    text: v.text.clone(),
                    expanded: None,
                });
                return Some(FxCue::GrillPlanReady {
                    contest_id: effect_id,
                    version: v.version,
                });
            }
            EnvelopePayload::GrillComplete(v) => {
                self.final_plan = self
                    .plans
                    .iter()
                    .find(|p| p.version == v.final_plan_version)
                    .map(|p| p.text.clone())
            }
            _ => {}
        }
        None
    }
}
