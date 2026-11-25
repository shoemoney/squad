//! Committed crosscheck coverage. COMPLETE and bounded-stop are separate facts.
use crate::fx::Palette;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Default)]
pub struct CrosscheckView {
    pub setup: Option<CrosscheckSetup>,
    pub state: Option<CrosscheckState>,
    pub preflight: Option<CrosscheckPreflight>,
    pub grid: Option<CrosscheckGrid>,
    pub inventories: BTreeMap<MemberNick, CrosscheckInventory>,
    pub reviews: BTreeMap<CellId, CrosscheckCellReview>,
    pub review_work_ids: BTreeSet<WorkId>,
    pub audits: BTreeMap<MemberNick, CrosscheckAudit>,
    pub report: Option<CrosscheckReport>,
    pub ruling: Option<CrosscheckRulingRecord>,
    pub collapsed_cells: BTreeSet<CellId>,
    pub acceptances: Vec<CrosscheckAcceptance>,
}
impl CrosscheckView {
    pub fn toggle_cell(&mut self, id: CellId) {
        if !self.collapsed_cells.remove(&id) {
            self.collapsed_cells.insert(id);
        }
    }
    pub fn lines(&self) -> Vec<Line<'static>> {
        let mut rows = vec![Line::from("CROSSCHECK")];
        if let Some(s) = &self.state {
            rows.push(Line::from(format!(
                "{:?} · plan v{} · grid v{}",
                s.status, s.plan_version, s.grid_version
            )));
        }
        if let Some(p) = &self.preflight {
            rows.push(Line::from(format!(
                "{} normal calls · {} reserved attempts · optional extraction {}",
                p.base.estimate.model_calls, p.reserved_attempts, p.extraction_required
            )));
        }
        if let Some(s) = &self.setup {
            rows.push(Line::from(format!("Plan: {}", s.plan)));
            if let Some(g) = &s.goal {
                rows.push(Line::from(format!("Goal: {}", g.goal)));
                for c in &g.constraints {
                    rows.push(Line::from(format!("Constraint: {c}")));
                }
            } else {
                rows.push(Line::from(
                    "Extracting goal and constraints before blind inventory.",
                ));
            }
        }
        rows.push(Line::from(format!(
            "Blind inventories: {} · plan hidden from writers",
            self.inventories.len()
        )));
        for inventory in self.inventories.values() {
            rows.push(Line::from(format!(
                "{} · {:?} lens · {} independent cases",
                inventory.member,
                inventory.lens,
                inventory.response.items.len()
            )));
        }
        if let Some(g) = &self.grid {
            rows.push(Line::from(format!(
                "{} retained cells · {} cap omissions",
                g.cells.len(),
                g.omitted.len()
            )));
            for omitted in &g.omitted {
                rows.push(Line::from(format!(
                    "Cap omission {} · {:?}",
                    omitted.cell_id, omitted.items
                )));
            }
            for cell in &g.cells {
                let ledger = self
                    .report
                    .as_ref()
                    .and_then(|r| r.ledger.iter().find(|c| c.cell.cell_id == cell.cell_id));
                let review = if let Some(ledger) = ledger {
                    ledger.ruling.as_ref().or(ledger.review.as_ref())
                } else {
                    self.ruling
                        .as_ref()
                        .and_then(|r| r.ruling.cells.iter().find(|c| c.cell_id == cell.cell_id))
                        .or_else(|| self.reviews.get(&cell.cell_id))
                };
                let accepted = ledger.is_some_and(|c| c.acceptance_event_id.is_some());
                rows.push(Line::from(format!(
                    "{} · {} · {}",
                    cell.cell_id,
                    if accepted {
                        "HUMAN ACCEPTED".into()
                    } else {
                        review
                            .map(|r| format!("{:?}", r.status))
                            .unwrap_or_else(|| "OPEN".into())
                    },
                    cell.items
                        .iter()
                        .map(|i| i.as_str())
                        .collect::<Vec<_>>()
                        .join(" × ")
                )));
                if self.collapsed_cells.contains(&cell.cell_id) {
                    continue;
                }
                if let Some(original) = self
                    .reviews
                    .get(&cell.cell_id)
                    .or_else(|| ledger.and_then(|c| c.review.as_ref()))
                    && review != Some(original)
                {
                    rows.push(Line::from(format!(
                        "  Original review {:?}: {}",
                        original.status, original.reason
                    )));
                }
                if let Some(r) = review {
                    rows.push(Line::from(format!("  {}", r.reason)));
                    if let Some(c) = &r.citation {
                        rows.push(Line::from(format!(
                            "  Citation {}: {}",
                            c.location, c.quote
                        )));
                    }
                    if let Some(f) = &r.proposed_fix {
                        rows.push(Line::from(format!("  Fix: {f}")));
                    }
                }
                if cell.reviewer.is_none() {
                    rows.push(Line::from("  Assignment hole: no eligible reviewer."));
                }
            }
        }
        for audit in self.audits.values() {
            rows.push(Line::from(format!(
                "Audit {} · {}/5 sampled closures",
                audit.auditor,
                audit.sample.len()
            )));
            for f in &audit.response.audits {
                rows.push(Line::from(format!(
                    "  {} {:?}: {}",
                    f.cell_id, f.verdict, f.reason
                )));
                if let Some(citation) = &f.citation {
                    rows.push(Line::from(format!(
                        "  Audit citation {}: {}",
                        citation.location, citation.quote
                    )));
                }
            }
        }
        for audit in self.audits.values() {
            rows.push(Line::from(format!(
                "Missing-case proposal: {} · {} · fix {}",
                audit.response.missed_case.case,
                audit.response.missed_case.reason,
                audit.response.missed_case.proposed_fix
            )));
        }
        if let Some(ruling) = &self.ruling {
            for addition in &ruling.ruling.additions {
                rows.push(Line::from(format!(
                    "Addition {:?}: {}",
                    addition.decision, addition.reason
                )));
            }
        }
        if let Some(r) = &self.report {
            for c in &r.ledger {
                if self
                    .grid
                    .as_ref()
                    .is_some_and(|g| !g.cells.iter().any(|cell| cell.cell_id == c.cell.cell_id))
                {
                    rows.push(Line::from(format!(
                        "Audit addition {} · {} · {:?}",
                        c.cell.cell_id,
                        if c.acceptance_event_id.is_some() {
                            "HUMAN ACCEPTED"
                        } else {
                            "OPEN"
                        },
                        c.cell.items
                    )));
                }
            }
            for risk in &r.residual_risks {
                rows.push(Line::from(format!(
                    "Residual {} · {}: {} · fix {}",
                    risk.id,
                    if risk.acceptance_event_id.is_some() {
                        "HUMAN ACCEPTED"
                    } else {
                        "OPEN"
                    },
                    risk.case,
                    risk.proposed_fix
                )));
            }
            rows.push(Line::from(format!(
                "Missing inventories {:?} · failed reviewers {:?} · failed auditors {:?}",
                r.failed_inventories, r.failed_reviewers, r.failed_auditors
            )));
            rows.push(Line::from(format!(
                "Clean audit: {} · bounded stop: {}",
                r.audit_clean, r.bounded_stop
            )));
        }
        for a in &self.acceptances {
            rows.push(Line::from(format!(
                "Human acceptance · plan v{} grid v{} · cells {:?} · residuals {:?}: {}",
                a.plan_version, a.grid_version, a.cell_ids, a.residual_risk_ids, a.reason
            )));
        }
        rows.push(Line::from("Stop rule: every retained cell has evidence, justified exclusion or explicit human acceptance; audit has no unresolved material finding. Bounded coverage is not proof of completeness."));
        rows.push(Line::from("On COMPLETE: /accept <cellId…> -- <reason>"));
        rows
    }
}
pub fn render(view: &CrosscheckView, area: Rect, buf: &mut Buffer, palette: &Palette) {
    Paragraph::new(view.lines())
        .style(Style::default().fg(palette.chrome()))
        .wrap(Wrap { trim: false })
        .render(area, buf);
}
