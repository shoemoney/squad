//! Authoritative estimates; the UI does not calculate a competing plan.
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use squad_protocol::*;
#[derive(Debug, Clone)]
pub struct PreflightView {
    pub plan: PendingPreflight,
    pub pending: bool,
}
/// Question-first context shown after a pick (M4.P3.L7.T4). All values come from
/// the core's pick acknowledgement; none is chosen or counted here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostPick {
    /// Nicks whose answers became the debate solutions, or the grill leader first.
    pub picked: Vec<String>,
    pub grill: bool,
    /// Question-first debates request judging by default.
    pub judge_default: bool,
    /// Collection answers already billed and therefore excluded from the estimate.
    pub billed_collection_answers: u32,
}
impl PostPick {
    pub fn text(&self) -> String {
        let mut lines = vec![if self.grill {
            format!(
                "Grill leader {} · every other answering member grills",
                self.picked.first().map_or("?", String::as_str)
            )
        } else {
            format!("Solutions from {}", self.picked.join(" and "))
        }];
        if !self.grill && self.judge_default {
            lines.push("Judging requested by default".into());
        }
        if self.billed_collection_answers > 0 {
            lines.push(format!(
                "{} collection answers already billed · excluded from this estimate",
                self.billed_collection_answers
            ));
        }
        lines.join("\n")
    }
}
/// What the room shows again once a plan is cancelled or expires.
pub fn restored_status<'a>(status: &PreflightStatus, prior: &'a str) -> Option<&'a str> {
    matches!(
        status,
        PreflightStatus::Cancelled | PreflightStatus::Expired
    )
    .then_some(prior)
}
impl PreflightView {
    pub fn plan_id(&self) -> PlanId {
        match &self.plan {
            PendingPreflight::Debate(p) => p.plan_id,
            PendingPreflight::Contest(p) => p.plan_id,
        }
    }
    pub fn model_calls(&self) -> u32 {
        match &self.plan {
            PendingPreflight::Debate(p) => p.estimate.model_calls,
            PendingPreflight::Contest(p) => p.estimate.model_calls,
        }
    }
    pub fn confirmation_required(&self) -> bool {
        match &self.plan {
            PendingPreflight::Debate(p) => p.confirmation_required,
            PendingPreflight::Contest(p) => p.confirmation_required,
        }
    }
    /// Composition of the authoritative count, derived from the plan's own fields.
    /// It explains the core's number and never replaces it.
    pub fn breakdown(&self) -> String {
        match &self.plan {
            PendingPreflight::Debate(p) => {
                if p.judge_only {
                    return "judge only · 1 final judging call · no round scoring".into();
                }
                let turns = 2 * (p.rounds + u32::from(p.opening_round_required));
                if p.judge_requested {
                    format!(
                        "{turns} debate turns · {} round-scoring judge calls · 1 final judging call",
                        p.rounds
                    )
                } else {
                    format!("{turns} debate turns · no judge calls")
                }
            }
            PendingPreflight::Contest(p) => {
                let n = p.participants.len() as u32;
                match p.kind {
                    ContestKind::Tournament => {
                        let answers: u32 = (2..=n).sum();
                        format!(
                            "{n} entrants · {answers} answer calls · {} judge calls",
                            n.saturating_sub(1)
                        )
                    }
                    ContestKind::Grill => {
                        let grillers = n.saturating_sub(1);
                        match p.settings["totalRounds"]
                            .as_u64()
                            .or_else(|| p.settings["setup"]["totalRounds"].as_u64())
                        {
                            Some(r) => format!(
                                "{r} rounds × ({grillers} grillers × 2 + 1) calls · leader {}",
                                p.participants
                                    .first()
                                    .map_or("?".into(), ToString::to_string)
                            ),
                            None => format!("{grillers} grillers × 2 + 1 calls per round"),
                        }
                    }
                    ContestKind::Census => format!(
                        "{} passes × ({n} members × 2 + 2) calls · {} reserved attempts",
                        p.settings["passes"], p.settings["reservedCalls"]
                    ),
                    ContestKind::Crosscheck => {
                        let extraction =
                            p.settings["extractionRequired"].as_bool().unwrap_or(false);
                        format!(
                            "{n} members · optional extraction call {}",
                            if extraction { "included" } else { "not needed" }
                        )
                    }
                    ContestKind::Survey => match p.settings["collect"]
                        .as_array()
                        .map(|a| a.len() as u64)
                        .or_else(|| p.settings["collect"].as_u64())
                        .or_else(|| {
                            p.settings["collect"]
                                .as_bool()
                                .map(|collect| if collect { u64::from(n) } else { 0 })
                        }) {
                        Some(0) | None => format!("{n} members · using existing answers"),
                        Some(k) => format!("{n} members · collecting {k} new answers"),
                    },
                    ContestKind::Probe => format!("{n} participants after adjustment"),
                    ContestKind::Debate => "debate".into(),
                }
            }
        }
    }
    fn sources(&self) -> Option<String> {
        let PendingPreflight::Contest(p) = &self.plan else {
            return None;
        };
        if p.kind == ContestKind::Grill
            && let Some(plan) = p.settings["setup"]["initialPlan"].as_str()
        {
            return Some(format!(
                "Plan from @{}:\n{}",
                p.participants.first().map_or("?", Nick::as_str),
                crate::markdown::sanitize(plan)
            ));
        }
        let list = p.settings.get("sourceEventIds")?.as_array()?;
        (!list.is_empty()).then(|| {
            format!(
                "sources: {} · {}",
                list.len(),
                list.iter()
                    .filter_map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
    /// Tournaments are blind and never swap judges, so a substitution warning is
    /// never displayed for them even if one arrives.
    fn visible_warning<'a>(&self, warning: Option<&'a Text>) -> &'a str {
        let text = warning.map(Text::as_str).unwrap_or("");
        let tournament = matches!(
            &self.plan,
            PendingPreflight::Contest(p) if p.kind == ContestKind::Tournament
        );
        let lower = text.to_ascii_lowercase();
        if tournament && (lower.contains("substitut") || lower.contains("swap")) {
            ""
        } else {
            text
        }
    }
    pub fn text(&self) -> String {
        self.text_with(None)
    }
    pub fn text_with(&self, post_pick: Option<&PostPick>) -> String {
        let (e, people, extra, warning) = match &self.plan {
            PendingPreflight::Debate(p) => (
                &p.estimate,
                p.debaters
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                format!(
                    "{} rounds · judge {} · opening {}",
                    p.rounds,
                    p.judge_model.as_deref().unwrap_or("none"),
                    p.opening_round_required
                ),
                p.warning.as_ref(),
            ),
            PendingPreflight::Contest(p) => (
                &p.estimate,
                p.participants.iter().map(ToString::to_string).collect(),
                if p.kind == ContestKind::Census {
                    format!(
                        "census · {} passes · {} reserved attempts",
                        p.settings["passes"], p.settings["reservedCalls"]
                    )
                } else {
                    p.kind.as_str().to_owned()
                },
                p.warning.as_ref(),
            ),
        };
        let mut lines = vec![
            people.join(" vs "),
            extra,
            format!(
                "{} calls · estimate ${:.4} · reserve ${:.4} · remaining ${:.4}",
                e.model_calls, e.estimated_usd, e.reserved_usd, e.remaining_usd
            ),
            self.breakdown(),
        ];
        lines.extend(self.sources());
        lines.extend(post_pick.map(PostPick::text));
        lines.push(self.visible_warning(warning).to_owned());
        lines.push(
            if self.pending {
                "Waiting for acknowledgement…"
            } else if !self.confirmation_required() {
                "Core auto-confirmed"
            } else {
                "Y Confirm · N Cancel"
            }
            .to_owned(),
        );
        lines.join("\n")
    }
}
pub fn render(view: &PreflightView, area: Rect, buffer: &mut Buffer) {
    render_with_source(view, "", area, buffer);
}

/// Frozen human-visible source context from the same committed setup as the plan.
pub fn source_preview(plan: &PendingPreflight, session: &crate::session::Session) -> String {
    let PendingPreflight::Contest(plan) = plan else {
        return String::new();
    };
    let id = plan.contest_id;
    let text = match plan.kind {
        ContestKind::Census => session
            .census
            .get(&id)
            .and_then(|v| v.setup.as_ref())
            .map(|s| {
                format!(
                    "Plan v{} · sources {} · {}",
                    s.plan.version,
                    s.plan
                        .source_event_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    s.plan.text
                )
            }),
        ContestKind::Crosscheck => session
            .contests
            .get(&id)
            .and_then(|v| v.crosscheck.setup.as_ref())
            .map(|s| format!("Plan {} v{} · {}", s.plan_id, s.plan_version, s.plan)),
        ContestKind::Survey => session
            .contests
            .get(&id)
            .and_then(|v| v.survey.as_ref())
            .and_then(|v| {
                v.artifacts.iter().find_map(|a| {
                    if let EnvelopePayload::SurveySetup(s) = a {
                        Some(format!(
                            "Question: {} · answers {}",
                            s.question,
                            s.answers
                                .iter()
                                .map(|a| a.message_id.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    } else {
                        None
                    }
                })
            }),
        ContestKind::Probe => session
            .probes
            .get(&id)
            .and_then(|v| v.setup.as_ref())
            .map(|s| format!("Brief: {}", s.brief.observation)),
        _ => None,
    };
    crate::markdown::sanitize(text.as_deref().unwrap_or(""))
        .chars()
        .take(240)
        .collect()
}

pub fn render_with_source(view: &PreflightView, source: &str, area: Rect, buffer: &mut Buffer) {
    let text = view.text();
    let (body, controls) = text.rsplit_once('\n').unwrap_or((&text, ""));
    let body = if source.is_empty() {
        body.to_owned()
    } else {
        format!("{body}\n{source}")
    };
    let block = Block::default().borders(Borders::ALL).title("Preflight");
    let inner = block.inner(area);
    block.render(area, buffer);
    if inner.height == 0 {
        return;
    }
    // The composer can occupy eight rows. Keep plan-bound controls visible even
    // when its source preview exceeds the remaining transcript height.
    Paragraph::new(body).wrap(Wrap { trim: false }).render(
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        ),
        buffer,
    );
    Paragraph::new(controls).render(
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        buffer,
    );
}
