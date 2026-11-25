//! Static probe evidence. Animation only accents connectors from this geometry.
use super::tournament::paint::{Painter, heading};
use crate::fx::Palette;
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct ProbeView {
    pub setup: Option<ProbeSetup>,
    pub state: Option<ProbeState>,
    pub explanations: Vec<(String, ProbeExplanation)>,
    pub checks: Vec<(String, ProbeCheck)>,
    pub predictions: Vec<(String, ProbePrediction)>,
    pub material: Option<ProbeCaseMaterial>,
    pub report: Option<ProbeReport>,
}
impl ProbeView {
    pub fn apply(&mut self, event: &CommittedEnvelope) {
        self.apply_payload(&event.content, &event.base.sender);
    }
    pub fn apply_payload(&mut self, payload: &EnvelopePayload, actor: &Actor) {
        let author = actor.to_string();
        match payload {
            EnvelopePayload::ProbeSetup(v) => self.setup = Some(v.clone()),
            EnvelopePayload::ProbeState(v) => self.state = Some(v.clone()),
            EnvelopePayload::ProbeExplanation(v) => {
                self.explanations.push((author, v.data.clone()))
            }
            EnvelopePayload::ProbeCheck(v) => self.checks.push((author, v.data.clone())),
            EnvelopePayload::ProbePrediction(v) => self.predictions.push((author, v.data.clone())),
            EnvelopePayload::ProbeReport(v) => self.report = Some(v.data.clone()),
            EnvelopePayload::CaseOpened(v) => {
                if let CaseMaterial::Probe(m) = &v.material {
                    self.material = Some(m.clone());
                }
            }
            _ => {}
        }
    }
}
#[derive(Default)]
pub struct ProbeRects {
    pub banner: Rect,
    pub connectors: Vec<(u16, u16)>,
    pub content_height: usize,
}
pub fn render(view: &ProbeView, area: Rect, buf: &mut Buffer, palette: &Palette) -> ProbeRects {
    let mut p = Painter::new(area, buf, 0);
    let mut out = ProbeRects::default();
    let body = Style::default();
    let gold = heading(palette);
    out.banner = p.text("PROBE", gold);
    if let Some(s) = &view.setup {
        p.text(&format!("Brief: {}", s.brief.observation), body);
        p.text(
            &format!(
                "Explainers: {} · Checkers: {}",
                s.explainers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                s.checkers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            gold,
        );
        if let Some(n) = &s.sitting_out {
            p.text(&format!("Sitting out: @{n}"), body);
        }
    }
    if let Some(s) = &view.state {
        p.text(&format!("{:?}", s.state), gold);
        for n in &s.skipped_members {
            p.text(&format!("Skipped member: @{n}"), body);
        }
        for w in &s.skipped_work_ids {
            p.text(&format!("Skipped work: {w}"), body);
        }
        if let Some(reason) = &s.terminal_reason {
            p.text(reason.as_str(), body);
        }
    }
    for (author, e) in &view.explanations {
        p.text(&format!("{author}: {}", e.explanation), body);
        p.text(&format!("Key assumption: {}", e.key_assumption), body);
        for l in &e.limitations {
            p.text(&format!("Limit: {l}"), body);
        }
    }
    for (author, c) in &view.checks {
        p.text(&format!("{author}: {}", c.check), body);
        p.text(
            &format!(
                "Procedure: {} · Measure: {} · Effort: {}",
                c.procedure, c.measurement, c.effort
            ),
            body,
        );
        for o in &c.outcomes {
            p.text(&format!("{}: {}", o.id, o.description), body);
        }
        for s in &c.safety_notes {
            p.text(&format!("Safety: {s}"), body);
        }
    }
    for (author, row) in &view.predictions {
        p.text(&format!("Locked predictions · {author}"), gold);
        for cell in &row.predictions {
            p.text(
                &format!(
                    "{}: {} · {}",
                    cell.check_id,
                    cell.expected_observation
                        .as_ref()
                        .map(Text::as_str)
                        .unwrap_or("cannot predict"),
                    cell.reason
                ),
                body,
            );
        }
    }
    if let Some(m) = &view.material {
        for id in &m.missing_rows {
            p.text(
                &format!("Missing prediction row: {id}; no elimination evidence"),
                body,
            );
        }
        for n in &m.skipped_members {
            p.text(&format!("Skipped member: @{n}"), body);
        }
    }
    if let Some(r) = &view.report {
        p.text("Prediction matrix", gold);
        for (explanation, row) in &r.prediction_matrix {
            p.text(&format!("Explanation {explanation}"), gold);
            if let Some(row) = row {
                for cell in &row.predictions {
                    p.text(
                        &format!(
                            "{}: {} · {}",
                            cell.check_id,
                            cell.expected_observation
                                .as_ref()
                                .map(Text::as_str)
                                .unwrap_or("cannot predict"),
                            cell.reason
                        ),
                        body,
                    );
                }
            } else {
                p.text("Missing row; no elimination evidence", body);
            }
        }
        p.text("Decision tree", gold);
        let mut pending = vec![(r.decision_tree.root.to_string(), false)];
        let mut seen = std::collections::BTreeSet::new();
        while let Some((id, unexpected)) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(node) = r.decision_tree.nodes.get(&id) else {
                continue;
            };
            let line = match node {
                ProbeNode::Check {
                    check_id,
                    measurement,
                    branches,
                    unexpected,
                } => {
                    pending.push((unexpected.to_string(), true));
                    for (outcome, next) in branches.iter().rev() {
                        pending.push((next.to_string(), false));
                        p.text(&format!("Result {outcome} → {next}"), body);
                    }
                    format!("├── {id}: check {check_id}; measure {measurement}")
                }
                ProbeNode::Leaf {
                    remaining,
                    next_action,
                    reassess,
                } => format!(
                    "└── {}{id}: {} remain · {next_action}{}",
                    if unexpected {
                        "Unexpected result · "
                    } else {
                        ""
                    },
                    remaining
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    if *reassess {
                        " · reopen diagnostic brief"
                    } else {
                        ""
                    }
                ),
            };
            let rect = p.text(&line, body);
            if !rect.is_empty() {
                out.connectors.push((rect.x, rect.y));
                out.connectors.push((rect.x + 1, rect.y));
                out.connectors.push((rect.x + 2, rect.y));
            }
        }
        for d in &r.discarded_checks {
            p.text(&format!("Dropped check {}: {}", d.check_id, d.reason), body);
        }
        for g in &r.inseparable_groups {
            p.text(
                &format!(
                    "Inseparable: {} · {}",
                    g.explanations
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    g.reason
                ),
                body,
            );
        }
        for l in &r.limitations {
            p.text(&format!("Limitation: {l}"), body);
        }
    }
    p.text(
        "Inspect manually. No explanation is declared true; checks have not been executed.",
        body,
    );
    out.content_height = p.height();
    out
}
