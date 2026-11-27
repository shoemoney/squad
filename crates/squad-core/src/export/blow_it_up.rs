//! Human report and complete evidence appendix, excluding frozen provider prompts.
use squad_protocol::blow_it_up::*;
use squad_protocol::*;
use std::fmt::Write;
pub fn report_text(report: &BlowItUpReport, expanded: bool) -> String {
    let mut out = String::from("BLOW IT UP\n\nWHAT SURVIVED\n\n");
    for row in &report.findings {
        let f = &row.finding;
        let _ = writeln!(
            out,
            "### {} / {} · {} · score {}\n\n{}\n\nBlast radius: {}\n\nMitigation: {}\n\nRebuttal: {}. Residual: {}\n",
            squad_protocol::human::label(&f.severity),
            squad_protocol::human::label(&f.likelihood),
            f.title,
            row.score,
            f.exploit,
            f.blast_radius,
            f.mitigation,
            squad_protocol::human::label(&f.rebuttal_status),
            f.residual
        );
        let _ = writeln!(
            out,
            "Blind finders: {}{}{}\n",
            row.finder_count,
            if row.lone {
                " · Lone finding: the other attackers missed this"
            } else {
                ""
            },
            if row.addition {
                " · Includes challenge-stage addition (no finder count for additions)"
            } else {
                ""
            }
        );
        if let Some(why) = &f.challenge_rejection {
            let _ = writeln!(out, "Challenges rejected: {why}\n");
        }
    }
    if report.findings.is_empty() {
        out.push_str("No surviving findings were reported. This does not establish safety.\n\n");
    }
    for v in &report.additions {
        let _ = writeln!(out, "Addition · {}: {}\n", v.title, v.exploit);
    }
    if !report.unchallenged.is_empty() {
        let _ = writeln!(
            out,
            "Unchallenged vectors ({}): the false-positive filter did not run on these.\n{}\n",
            report.unchallenged.len(),
            report
                .unchallenged
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(
        out,
        "Killed appendix · {} vectors{}\n",
        report.killed.len(),
        if expanded { "" } else { " (collapsed)" }
    );
    if expanded {
        for k in &report.killed {
            let _ = writeln!(
                out,
                "- {} · false_positive {}/{}; weak {}\n",
                k.vector.title, k.tally.false_positive, k.tally.accepted, k.tally.weak
            );
            for q in &k.defeating_quotes {
                let _ = writeln!(out, "  Defeating quote: {q}\n");
            }
            if let Some(n) = &k.judge_note {
                let _ = writeln!(out, "  Judge note: {n}\n");
            }
        }
    }
    for skip in &report.skipped {
        let _ = writeln!(out, "Skipped @{} · {}\n", skip.member, skip.step);
    }
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut out = String::new();
    let mut evidence = String::new();
    let mut found = false;
    let mut status = None;
    for e in &snapshot.events {
        match &e.content {
            EnvelopePayload::BlowItUpSetup(s) if s.contest_id == id => {
                found = true;
                let setup: BlowItUpSetup = serde_json::from_value(s.settings.clone())
                    .map_err(|_| super::ExportError::InvalidSnapshot)?;
                let _ = writeln!(
                    evidence,
                    "## Target snapshot\n\n{}\n\nScope: {}\n\n## Private lens draw\n",
                    setup.target, setup.scope
                );
                for (n, l) in setup.lenses {
                    let _ = writeln!(evidence, "- @{n}: {l}");
                }
            }
            EnvelopePayload::BlowItUpState(s) if s.contest_id == id => {
                found = true;
                status = Some(format!(
                    "Status: {}{}\n\n",
                    squad_protocol::human::label(&s.state),
                    s.terminal_reason
                        .as_ref()
                        .map(|r| format!(" · {r}"))
                        .unwrap_or_default()
                ));
            }
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id && a.kind == Some(ContestKind::BlowItUp) =>
            {
                found = true;
                let body = a.body.as_ref().ok_or(super::ExportError::InvalidSnapshot)?;
                if a.artifact_kind.as_str() == "blow_it_up.report" {
                    let report: BlowItUpReport = serde_json::from_value(body.clone())
                        .map_err(|_| super::ExportError::InvalidSnapshot)?;
                    out = report_text(&report, true);
                } else if a.artifact_kind.as_str() != "blow_it_up.case" {
                    let _ = writeln!(
                        evidence,
                        "\n## {} · {}\n\n```json\n{}\n```\n",
                        a.artifact_kind,
                        a.actor
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_default(),
                        serde_json::to_string_pretty(body)
                            .map_err(|_| super::ExportError::InvalidSnapshot)?
                    );
                }
            }
            _ => {}
        }
    }
    if !found {
        return Err(super::ExportError::ContestNotFound);
    }
    if out.is_empty() {
        out.push_str("# BLOW IT UP\n\nPartial evidence; the contest has no completed report.\n\n");
    }
    if let Some(status) = status {
        out.push_str(&status);
    }
    out.push_str(&evidence);
    Ok(out)
}
