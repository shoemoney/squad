//! Markdown report assembled only from committed debugging evidence.
use squad_protocol::debugging::*;
use squad_protocol::*;
use std::fmt::Write;
pub const NO_PATCH: &str = "No patch: the evidence does not yet isolate a cause.";
pub fn report_text(r: &DebuggingReport, details: bool) -> String {
    let mut out = format!(
        "# DEBUG\n\nResult: {}\n\n## Brief\n\n{}\n\nLimits: {}\n\n## DIFFERENTIAL\n\n| ID | Cause | Status | Consistent results | Failed prediction |\n| --- | --- | --- | --- | --- |\n",
        r.reason, r.brief, r.limits
    );
    for h in &r.differential {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            h.hypothesis.hypothesis_id,
            h.hypothesis.cause.replace('|', "\\|"),
            if h.status == DebuggingHypothesisStatus::Live {
                "live"
            } else {
                "eliminated"
            },
            h.consistent_results,
            h.failed_prediction.as_deref().unwrap_or("")
        );
    }
    for round in &r.rounds {
        let _ = writeln!(out, "\n## ROUND {}\n", round.round);
        if let Some(s) = &round.selection {
            let _ = writeln!(out, "RUN THE TEST: {}\n", s.instruction);
            for o in &s.outcomes {
                let _ = writeln!(out, "- {}: {}", o.label, o.description);
            }
        }
        if let Some(result) = &round.result {
            let _ = writeln!(
                out,
                "\nRESULT: {}{}\n\n{}",
                result.outcome,
                if result.unexpected {
                    " (unexpected)"
                } else {
                    ""
                },
                result.evidence
            );
        } else {
            out.push_str("\nResult not recorded.\n");
        }
        for (label, p) in &round.proposals {
            for t in &p.tests {
                if details || t.safety == DebuggingSafety::Risky {
                    let _ = writeln!(
                        out,
                        "\n{}:{} · {:?} · {:?}\n{}\n{}",
                        label, t.local_id, t.safety, t.effort, t.test, t.procedure
                    );
                    for prediction in &t.predictions {
                        let _ = writeln!(
                            out,
                            "- {}: {}",
                            prediction.hypothesis_id,
                            prediction.outcome.as_deref().unwrap_or_else(|| prediction
                                .cannot_predict_reason
                                .as_deref()
                                .unwrap_or("cannot_predict"))
                        );
                    }
                }
            }
        }
        if let Some(s) = &round.selection {
            for n in &s.not_selected {
                let _ = writeln!(out, "Not selected {}: {}", n.test_ref, n.why);
            }
        }
    }
    if let Some(n) = &r.narrative {
        let _ = writeln!(
            out,
            "\n## Report\n\n{}\n\nNext test: {}",
            n.summary,
            n.next_test.as_deref().unwrap_or("None proposed")
        );
        if r.patch_allowed {
            if let Some(p) = &n.patch {
                let _ = writeln!(out, "\n## Suggested patch\n\n{p}\n\n{}", n.patch_rationale);
            } else {
                out.push_str("\nNo patch proposed.\n");
            }
        } else {
            let _ = writeln!(out, "\n{NO_PATCH}");
        }
        for risk in &n.risks {
            let _ = writeln!(out, "- Risk: {risk}");
        }
    } else if !r.patch_allowed {
        let _ = writeln!(out, "\n{NO_PATCH}");
    }
    for q in &r.quotes {
        if q.unanchored {
            let _ = writeln!(out, "Unanchored quote (not evidence): {}", q.quote);
        }
    }
    for skip in &r.skipped {
        let _ = writeln!(out, "Skipped: {skip}");
    }
    if details {
        out.push_str("\n## Hypothesis tables\n");
        for (label, t) in &r.tables {
            let _ = writeln!(
                out,
                "\n{label} · @{}",
                r.provenance
                    .get(label)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
            for h in &t.hypotheses {
                let _ = writeln!(
                    out,
                    "- {}: {} · {:?}\n  Supporting: {:?}\n  Contradicting: {:?}",
                    h.local_id, h.cause, h.prior, h.supporting, h.contradicting
                );
            }
        }
        out.push_str("\n## Raw answers\n");
        for a in &r.raw_answers {
            let _ = writeln!(out, "- {a}");
        }
    }
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let report = snapshot.events.iter().rev().find_map(|e| match &e.content {
        EnvelopePayload::ContestArtifact(a)
            if a.contest_id == id
                && a.artifact_kind.as_str() == "debug.report"
                && a.audience == Some(ArtifactAudience::Human)
                && a.body.as_ref().is_some_and(|b| b.get("brief").is_some()) =>
        {
            a.body.as_ref()
        }
        _ => None,
    });
    if let Some(report) = report {
        let r = validation::decode::<DebuggingReport>(report)
            .map_err(|_| super::ExportError::InvalidSnapshot)?;
        let mut out = report_text(&r, true);
        // A stop may arrive between acceptance and the parallel barrier. Those
        // accepted outputs remain evidence even when no run snapshot consumed them.
        out.push_str("\n## Accepted step artifacts\n");
        for e in &snapshot.events {
            if let EnvelopePayload::ContestArtifact(a) = &e.content
                && a.contest_id == id
                && a.audience == Some(ArtifactAudience::Human)
                && a.work_id.is_some()
            {
                let _ = writeln!(
                    out,
                    "\n### {} · round {}\n\n```json\n{}\n```",
                    a.artifact_kind,
                    a.round.unwrap_or_default(),
                    serde_json::to_string_pretty(a.body.as_ref().unwrap_or(&a.data))
                        .unwrap_or_default()
                );
            }
        }
        return Ok(out);
    }
    // A human wait has no final report yet: expose every committed artifact and raw answer.
    let mut out = String::from("# DEBUG — in progress\n");
    let mut found = false;
    for e in &snapshot.events {
        match &e.content {
            EnvelopePayload::DebuggingSetup(s) if s.contest_id == id => {
                found = true;
                let _ = writeln!(
                    out,
                    "\nBrief: {}\nLimits: {}",
                    s.settings["brief"], s.settings["limits"]
                );
            }
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id && a.audience == Some(ArtifactAudience::Human) =>
            {
                let _ = writeln!(
                    out,
                    "\n## {}\n\n```json\n{}\n```",
                    a.artifact_kind,
                    serde_json::to_string_pretty(a.body.as_ref().unwrap_or(&a.data))
                        .unwrap_or_default()
                );
            }
            EnvelopePayload::ContestHumanRequested(h) if h.pending.contest_id == id => {
                let _ = writeln!(
                    out,
                    "\n{}\n{}",
                    h.pending.prompt,
                    h.pending.items.join("\n")
                );
            }
            EnvelopePayload::ContestHumanAnswered(a) if a.contest_id == id => {
                let _ = writeln!(
                    out,
                    "\nRaw answer: {}",
                    a.text.as_deref().unwrap_or("/continue")
                );
            }
            _ => {}
        }
    }
    if found {
        Ok(out)
    } else {
        Err(super::ExportError::ContestNotFound)
    }
}
