//! Static probe evidence. Animation only accents connectors from this geometry.
use super::{banner::Canvas, tournament::paint::heading};
use crate::fx::Palette;
use ratatui::{buffer::Buffer, layout::Rect, style::Style};
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct ProbeView {
    pub banner_mode: Option<squad_core::banner::BannerMode>,
    pub banner_art: Option<std::sync::Arc<squad_core::banner::RenderedBanner>>,
    pub matrix_collapsed: bool,
    pub setup: Option<ProbeSetup>,
    pub state: Option<ProbeState>,
    pub explanations: Vec<(String, ProbeExplanation)>,
    pub checks: Vec<(String, ProbeCheck)>,
    pub predictions: Vec<(String, ProbePrediction)>,
    pub material: Option<ProbeCaseMaterial>,
    pub report: Option<ProbeReport>,
}
impl ProbeView {
    pub fn toggle_matrix(&mut self) {
        self.matrix_collapsed = !self.matrix_collapsed;
    }
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
    let mut p = Canvas::new(area, buf, 0);
    let mut out = ProbeRects::default();
    let body = Style::default();
    let gold = heading(palette);
    let fallback;
    let art = if let Some(art) = &view.banner_art {
        art.as_ref()
    } else {
        fallback = squad_core::banner::render(
            "PROBE",
            usize::from(area.width),
            view.banner_mode
                .unwrap_or(squad_core::banner::BannerMode::On),
        );
        &fallback
    };
    out.banner = p.art(art, gold);
    if let Some(s) = &view.setup {
        p.line(&format!("Brief: {}", s.brief.observation), body);
        p.line(
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
            p.line(&format!("Sitting out: @{n}"), body);
        }
    }
    if let Some(s) = &view.state {
        p.line(&squad_protocol::human::label(&s.state), gold);
        for n in &s.skipped_members {
            p.line(&format!("Skipped member: @{n}"), body);
        }
        for w in &s.skipped_work_ids {
            p.line(&format!("Skipped work: {w}"), body);
        }
        if let Some(reason) = &s.terminal_reason {
            p.line(reason.as_str(), body);
        }
    }
    for (author, e) in &view.explanations {
        p.line(&format!("{author}: {}", e.explanation), body);
        p.line(&format!("Key assumption: {}", e.key_assumption), body);
        for l in &e.limitations {
            p.line(&format!("Limit: {l}"), body);
        }
    }
    for (author, c) in &view.checks {
        p.line(&format!("{author}: {}", c.check), body);
        p.line(
            &format!(
                "Procedure: {} · Measure: {} · Effort: {}",
                c.procedure, c.measurement, c.effort
            ),
            body,
        );
        for o in &c.outcomes {
            p.line(&format!("{}: {}", o.id, o.description), body);
        }
        for s in &c.safety_notes {
            p.line(&format!("Safety: {s}"), body);
        }
    }
    for (author, row) in &view.predictions {
        p.line(&format!("Locked predictions · {author}"), gold);
        for cell in &row.predictions {
            p.line(
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
        p.line("Accepted anonymous explanations", gold);
        for e in &m.explanations {
            p.line(
                &format!(
                    "{}: {} · assumption: {}",
                    e.id, e.data.explanation, e.data.key_assumption
                ),
                body,
            );
        }
        p.line("Accepted anonymous checks", gold);
        for c in &m.checks {
            p.line(
                &format!(
                    "{}: {} · procedure: {} · measure: {}",
                    c.id, c.data.check, c.data.procedure, c.data.measurement
                ),
                body,
            );
            for o in &c.data.outcomes {
                p.line(&format!("{}: {}", o.id, o.description), body);
            }
        }
        for id in &m.missing_rows {
            p.line(
                &format!("Missing prediction row: {id}; no elimination evidence"),
                body,
            );
        }
        for n in &m.skipped_members {
            p.line(&format!("Skipped member: @{n}"), body);
        }
    }
    if let Some(r) = &view.report {
        p.line(
            if view.matrix_collapsed {
                "Prediction matrix · F7 to expand"
            } else {
                "Prediction matrix · F7 to collapse"
            },
            gold,
        );
        if !view.matrix_collapsed {
            for (explanation, row) in &r.prediction_matrix {
                p.line(&format!("Explanation {explanation}"), gold);
                if let Some(row) = row {
                    for cell in &row.predictions {
                        p.line(
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
                    p.line("Missing row; no elimination evidence", body);
                }
            }
        }
        p.line("Decision tree", gold);
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
                        p.line(&format!("Result {outcome} → {next}"), body);
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
            let rect = p.line(&line, body);
            if !rect.is_empty() {
                out.connectors.push((rect.x, rect.y));
                out.connectors.push((rect.x + 1, rect.y));
                out.connectors.push((rect.x + 2, rect.y));
            }
        }
        for d in &r.discarded_checks {
            p.line(&format!("Dropped check {}: {}", d.check_id, d.reason), body);
        }
        for g in &r.inseparable_groups {
            p.line(
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
            p.line(&format!("Limitation: {l}"), body);
        }
    }
    p.line(
        "Inspect manually. No explanation is declared true; checks have not been executed.",
        body,
    );
    out.content_height = p.row;
    out
}
