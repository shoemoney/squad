//! Explicit public typed artifacts; private permutations and raw work never render.
use squad_protocol::*;
use std::fmt::Write;
pub(super) fn render(payload: &EnvelopePayload) -> Option<String> {
    let mut out = String::new();
    match payload {
        EnvelopePayload::CrosscheckSetup(s) => {
            let _ = writeln!(
                out,
                "## CROSSCHECK · {}\n\nPlan version {}\n\n{}\n",
                s.contest_id, s.plan_version, s.plan
            );
            if let Some(goal) = &s.goal {
                let _ = writeln!(out, "Goal: {}\n", goal.goal);
                for c in &goal.constraints {
                    let _ = writeln!(out, "- Constraint: {c}");
                }
            }
        }
        EnvelopePayload::CrosscheckState(s) => {
            let _ = writeln!(
                out,
                "### Crosscheck {:?} · version {}\n",
                s.status, s.state_version
            );
        }
        EnvelopePayload::CrosscheckPreflight(p) => {
            let _ = writeln!(
                out,
                "{} normal calls; {} reserved attempts; extraction {}.\n",
                p.base.estimate.model_calls, p.reserved_attempts, p.extraction_required
            );
        }
        EnvelopePayload::CrosscheckGoal(s) => {
            let _ = writeln!(out, "### Extracted goal\n\n{}\n", s.goal.goal);
            for c in &s.goal.constraints {
                let _ = writeln!(out, "- Constraint: {c}");
            }
            for u in &s.goal.uncertainties {
                let _ = writeln!(out, "- Uncertainty: {u}");
            }
        }
        EnvelopePayload::CrosscheckInventory(s) => {
            let _ = writeln!(out, "### Inventory · {} · {:?}\n", s.member, s.lens);
            for i in &s.response.items {
                let _ = writeln!(out, "- {}: {} — {}", i.local_id, i.item, i.rationale);
            }
        }
        EnvelopePayload::CrosscheckGrid(g) => {
            let _ = writeln!(
                out,
                "### Grid v{} · {} retained · {} omitted by 200-cell cap\n",
                g.grid_version,
                g.cells.len(),
                g.omitted.len()
            );
            for item in &g.items {
                let _ = writeln!(
                    out,
                    "- Normalized {} · {:?}: {} · sources {:?}",
                    item.id, item.lens, item.item, item.source_item_ids
                );
            }
            for c in &g.cells {
                let _ = writeln!(
                    out,
                    "- {} · {} · {:?} · reviewer {} · contributors {:?}",
                    c.cell_id,
                    c.key,
                    c.items,
                    c.reviewer
                        .as_ref()
                        .map(|n| n.as_str())
                        .unwrap_or("unassignable; open"),
                    c.originators
                );
            }
            for c in &g.omitted {
                let _ = writeln!(
                    out,
                    "- Cap omission {} · {} · {:?}",
                    c.cell_id, c.key, c.items
                );
            }
        }
        EnvelopePayload::CrosscheckReview(r) => {
            let _ = writeln!(out, "### Review · {}\n", r.reviewer);
            for c in &r.response.cells {
                cell(&mut out, c);
            }
        }
        EnvelopePayload::CrosscheckAudit(a) => {
            let _ = writeln!(
                out,
                "### Audit · {} · {}/5 closures sampled\n",
                a.auditor,
                a.sample.len()
            );
            for f in &a.response.audits {
                let _ = writeln!(
                    out,
                    "- {} · {:?}: {} · citation {:?}",
                    f.cell_id, f.verdict, f.reason, f.citation
                );
            }
            let c = &a.response.missed_case;
            let _ = writeln!(
                out,
                "Missing-case proposal: {} · {} · fix {}\n",
                c.case, c.reason, c.proposed_fix
            );
        }
        EnvelopePayload::CrosscheckRuling(r) => {
            let _ = writeln!(out, "### Ruling\n");
            for c in &r.ruling.cells {
                cell(&mut out, c);
            }
            for a in &r.ruling.additions {
                let _ = writeln!(
                    out,
                    "- Addition {} · {:?}: {}",
                    a.audit_work_id, a.decision, a.reason
                );
            }
            for risk in &r.ruling.residual_risks {
                let _ = writeln!(out, "- Residual risk: {risk}");
            }
        }
        EnvelopePayload::CrosscheckAcceptance(a) => {
            let _ = writeln!(
                out,
                "### Explicit human risk acceptance\n\nPlan {} v{}, grid v{} · cells {:?} · residuals {:?}\n\n{}\n",
                a.plan_id,
                a.plan_version,
                a.grid_version,
                a.cell_ids,
                a.residual_risk_ids,
                a.reason
            );
        }
        EnvelopePayload::CrosscheckReport(r) => {
            let _ = writeln!(out, "### Coverage ledger\n");
            for c in &r.ledger {
                let _ = writeln!(
                    out,
                    "- {} · {:?} · acceptance {:?}",
                    c.cell.cell_id, c.cell.items, c.acceptance_event_id
                );
                if let Some(review) = c.ruling.as_ref().or(c.review.as_ref()) {
                    cell(&mut out, review);
                } else {
                    out.push_str("  Open: no accepted review.\n");
                }
            }
            for risk in &r.residual_risks {
                let _ = writeln!(
                    out,
                    "- Residual {}: {} · fix {}",
                    risk.id, risk.case, risk.proposed_fix
                );
            }
            let _ = writeln!(out, "Missing inventories: {:?}\n", r.failed_inventories);
            let _ = writeln!(
                out,
                "\nFailed reviewers: {:?}\n\nFailed auditors: {:?}\n\nCap omissions: {}\n\nClean audit: {}\n\nBounded stop: {}. COMPLETE means script execution finished. This is not proof of completeness.\n",
                r.failed_reviewers,
                r.failed_auditors,
                r.omitted.len(),
                r.audit_clean,
                r.bounded_stop
            );
        }
        _ => return None,
    }
    Some(out)
}
fn cell(out: &mut String, c: &CrosscheckCellReview) {
    let _ = writeln!(out, "- {} · {:?}: {}", c.cell_id, c.status, c.reason);
    if let Some(cite) = &c.citation {
        let _ = writeln!(out, "  Citation {}: {}", cite.location, cite.quote);
    }
    if let Some(fix) = &c.proposed_fix {
        let _ = writeln!(out, "  Proposed fix: {fix}");
    }
}

/// Decode only the public response contracts, never opaque work metadata.
pub(super) fn partial(a: &ContestArtifact, actor: &Actor) -> Option<String> {
    let work_id = a.work_id?;
    let Actor::Member(member) = actor else {
        return None;
    };
    match a.artifact_kind.as_str() {
        "crosscheck.cell" => render(&EnvelopePayload::CrosscheckReview(CrosscheckReview {
            contest_id: a.contest_id,
            work_id,
            reviewer: member.clone(),
            response: validation::decode(&a.data).ok()?,
        })),
        "crosscheck.audit" => {
            let response: CrosscheckAuditResponse = validation::decode(&a.data).ok()?;
            render(&EnvelopePayload::CrosscheckAudit(CrosscheckAudit {
                contest_id: a.contest_id,
                work_id,
                auditor: member.clone(),
                sample: response.audits.iter().map(|f| f.cell_id).collect(),
                response,
            }))
        }
        "crosscheck.inventory" => {
            let response: CrosscheckInventoryResponse = validation::decode(&a.data).ok()?;
            let mut out = format!("### Partial inventory · {member}\n\n");
            for item in response.items {
                let _ = writeln!(
                    out,
                    "- {}: {} — {}",
                    item.local_id, item.item, item.rationale
                );
            }
            Some(out)
        }
        _ => None,
    }
}
