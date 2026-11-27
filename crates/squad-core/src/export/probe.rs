//! Human export from typed, committed probe records.
use super::*;
pub(super) fn render(out: &mut String, payload: &EnvelopePayload) -> bool {
    match payload {
        EnvelopePayload::ProbeSetup(s) => {
            banner(out, Banner::Probe);
            heading(out, &format!("Probe {}", s.contest_id));
            block(out, "Diagnostic brief", s.brief.observation.as_str());
            block(out, "Ruled out", &s.brief.ruled_out);
            block(out, "Inspectable resources", s.brief.inspectable.as_str());
            block(
                out,
                "Explainers",
                &s.explainers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            block(
                out,
                "Checkers",
                &s.checkers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            if let Some(n) = &s.sitting_out {
                block(out, "Sitting out", n.as_str());
            }
        }
        EnvelopePayload::ProbeState(s) | EnvelopePayload::ContestState(ContestState::Probe(s)) => {
            heading(
                out,
                &format!("Probe · {}", squad_protocol::human::label(&s.state)),
            );
            if let Some(reason) = &s.terminal_reason {
                paragraph(out, reason.as_str());
            }
            for n in &s.skipped_members {
                block(out, "Skipped member", n.as_str());
            }
            for id in &s.skipped_work_ids {
                paragraph(out, &format!("Skipped work: {id}"));
            }
        }
        EnvelopePayload::ProbeExplanation(s) => {
            block(out, "Explanation", s.data.explanation.as_str());
            block(out, "Key assumption", s.data.key_assumption.as_str());
            for l in &s.data.limitations {
                block(out, "Limitation", l.as_str());
            }
        }
        EnvelopePayload::ProbeCheck(s) => {
            block(out, "Check", s.data.check.as_str());
            block(out, "Procedure", s.data.procedure.as_str());
            block(out, "Measure", s.data.measurement.as_str());
            for o in &s.data.outcomes {
                block(out, o.id.as_str(), o.description.as_str());
            }
            block(out, "Effort", s.data.effort.as_str());
            for n in &s.data.safety_notes {
                block(out, "Safety", n.as_str());
            }
        }
        EnvelopePayload::ProbePrediction(s) => {
            heading(out, "Locked predictions");
            predictions(out, &s.data);
        }
        EnvelopePayload::CaseOpened(s) => {
            let CaseMaterial::Probe(m) = &s.material else {
                return false;
            };
            heading(out, "Frozen anonymous items");
            for e in &m.explanations {
                block(out, &e.id.to_string(), e.data.explanation.as_str());
            }
            for c in &m.checks {
                block(out, &c.id.to_string(), c.data.check.as_str());
                block(out, "Procedure", c.data.procedure.as_str());
                block(out, "Measure", c.data.measurement.as_str());
                for outcome in &c.data.outcomes {
                    block(out, outcome.id.as_str(), outcome.description.as_str());
                }
                block(out, "Effort", c.data.effort.as_str());
            }
            for id in &m.missing_rows {
                block(out, "Missing prediction row", &id.to_string());
            }
            for n in &m.skipped_members {
                block(out, "Skipped member", n.as_str());
            }
        }
        EnvelopePayload::ProbeReport(s) => {
            heading(out, "Prediction matrix");
            for (id, row) in &s.data.prediction_matrix {
                heading(out, &format!("Explanation {id}"));
                if let Some(row) = row {
                    predictions(out, row)
                } else {
                    paragraph(out, "Missing row; provides no evidence.");
                }
            }
            heading(out, "Decision tree");
            paragraph(out, &format!("Start at {}", s.data.decision_tree.root));
            for (id, node) in &s.data.decision_tree.nodes {
                heading(out, &format!("Node {id}"));
                match node {
                    ProbeNode::Check {
                        check_id,
                        measurement,
                        branches,
                        unexpected,
                    } => {
                        paragraph(out, &format!("Check {check_id}: measure {measurement}"));
                        for (result, next) in branches {
                            paragraph(out, &format!("{result} → {next}"));
                        }
                        paragraph(
                            out,
                            &format!(
                                "Unexpected result → {unexpected}; reopen the diagnostic brief."
                            ),
                        );
                    }
                    ProbeNode::Leaf {
                        remaining,
                        next_action,
                        reassess,
                    } => {
                        paragraph(
                            out,
                            &format!(
                                "Remaining possibilities: {}",
                                remaining
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        );
                        paragraph(out, next_action.as_str());
                        if *reassess {
                            paragraph(out, "Reassess the diagnostic brief.");
                        }
                    }
                }
            }
            for c in &s.data.discarded_checks {
                block(
                    out,
                    &format!("Dropped check {}", c.check_id),
                    c.reason.as_str(),
                );
            }
            for g in &s.data.inseparable_groups {
                block(
                    out,
                    &format!(
                        "Inseparable: {}",
                        g.explanations
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    g.reason.as_str(),
                );
            }
            for l in &s.data.limitations {
                block(out, "Limitation", l.as_str());
            }
            paragraph(
                out,
                "These are instructions for inspection. No explanation is declared true and no check has been executed.",
            );
        }
        _ => return false,
    }
    true
}
fn predictions(out: &mut String, row: &ProbePrediction) {
    for p in &row.predictions {
        block(
            out,
            &format!("Check {}", p.check_id),
            p.expected_observation
                .as_ref()
                .map(Text::as_str)
                .unwrap_or("Cannot predict"),
        );
        paragraph(out, p.reason.as_str());
    }
}
