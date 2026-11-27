//! Human-readable evidence projection; never exports scheduling or frozen prompts.
use squad_protocol::readback::*;
use squad_protocol::*;
use std::fmt::Write;
pub fn report_text(report: &ReadbackReport, details: bool) -> String {
    let mut out = format!(
        "# READBACK\n\nResult: {}\n\n## Final spec\n\n{}\n",
        report.reason, report.final_spec
    );
    if report.last_rewrite_not_read_back {
        out.push_str("\nThe last rewrite was not read back.\n");
    }
    out.push_str("\n## Convergence\n\n| Round | Version | Readings | Entries | Resolved | Open | Cut |\n| --- | --- | --- | --- | --- | --- | --- |\n");
    for c in &report.convergence {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            c.round,
            c.spec_version,
            c.accepted_interpretations,
            c.ledger_entries,
            c.resolved,
            c.open,
            c.cut
        );
    }
    for (i, r) in report.rounds.iter().enumerate() {
        let _ = writeln!(out, "\n## Round {}\n", i + 1);
        if details {
            let _ = writeln!(out, "### Spec version {}\n\n{}\n", i + 1, r.spec);
            for (label, reading) in &r.interpretations {
                let _ = writeln!(
                    out,
                    "### {} · @{}\n\n{}\n",
                    label,
                    r.provenance
                        .get(label)
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    reading.build
                );
                for b in &reading.behaviours {
                    let _ = writeln!(
                        out,
                        "- {} → {} (source: {})",
                        b.situation, b.behaviour, b.spec_quote
                    );
                }
                for e in &reading.edge_cases {
                    let _ = writeln!(out, "- Edge case: {} → {}", e.case, e.answer);
                }
                for a in &reading.assumptions {
                    let _ = writeln!(out, "- Assumption: {}", a.text);
                }
            }
        }
        if let Some(l) = &r.ledger {
            for (i, e) in l.entries.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "### {}. {}\n\n> {}\n\n{}\n",
                    i + 1,
                    e.question,
                    e.passage,
                    e.behaviour_change
                );
                for (index, reading) in e.readings.iter().enumerate() {
                    let readers = reading
                        .source_labels
                        .iter()
                        .filter_map(|l| r.provenance.get(l))
                        .map(|n| format!("@{n}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let _ = writeln!(
                        out,
                        "- {}. {} — {}",
                        (b'a' + index as u8) as char,
                        reading.text,
                        readers
                    );
                }
                let chosen = r.choices.as_ref().and_then(|c| c.answers.get(&e.entry_id));
                let _ = writeln!(
                    out,
                    "\nChosen meaning: {}",
                    chosen.map_or("OPEN", String::as_str)
                );
                if let Some(change) = r
                    .rewrite
                    .as_ref()
                    .and_then(|rw| rw.changes.iter().find(|c| c.entry_id == e.entry_id))
                {
                    let _ = writeln!(
                        out,
                        "\nBefore: {}\n\nAfter: {}",
                        change.before, change.after
                    );
                }
            }
        }
        for n in &r.skipped {
            let _ = writeln!(out, "Skipped interpretation: @{n}");
        }
    }
    if details {
        out.push_str("\n## Human raw answers\n\n");
        for a in &report.raw_answers {
            let _ = writeln!(out, "- {a}");
        }
    }
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let report = snapshot
        .events
        .iter()
        .rev()
        .find_map(|e| match &e.content {
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id
                    && a.artifact_kind.as_str() == "readback.report"
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                a.body.as_ref()
            }
            _ => None,
        })
        .ok_or(super::ExportError::ContestNotFound)?;
    let report = validation::decode::<ReadbackReport>(report)
        .map_err(|_| super::ExportError::InvalidSnapshot)?;
    Ok(report_text(&report, true))
}
