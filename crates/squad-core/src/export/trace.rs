//! Auditable human report and source-packet export from committed events only.
use squad_protocol::trace::*;
use squad_protocol::*;
use std::fmt::Write;
pub fn report_text(r: &TraceReport, details: bool) -> String {
    let mut out = format!(
        "# TRACE\n\n{}\n\nResult: {}\n\n## BRIEF\n",
        r.question, r.reason
    );
    if let Some(b) = &r.brief {
        for s in &b.sections {
            let _ = writeln!(out, "\n### {}\n", s.heading);
            for p in &s.paragraphs {
                let _ = writeln!(
                    out,
                    "{} {}\n",
                    p.text,
                    p.claim_ids
                        .iter()
                        .map(|id| format!("[{id}]"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
        out.push_str("\n## Disputed claims\n");
        for d in &b.disputed {
            let _ = writeln!(out, "- [{}] {}", d.claim_id, d.text);
        }
        out.push_str("\n## Evidence gaps\n");
        for g in &b.gaps {
            let _ = writeln!(out, "- {g}");
        }
    }
    out.push_str("\n## Claim-to-source ledger\n");
    for c in &r.ledger.claims {
        let _ = writeln!(
            out,
            "\n### [{}] {}\n\nStatus: {:?}; independent sources: {}; contributing members: {}\n",
            c.claim_id, c.text, c.status, c.independent_sources, c.contributing_members
        );
        for q in &c.quotes {
            if let Some(span) = &q.span {
                let _ = writeln!(
                    out,
                    "- {} {} bytes {}..{}: > {}",
                    if q.counter { "Counter-quote" } else { "Quote" },
                    q.quote.source_id,
                    span.start,
                    span.end,
                    q.quote.quote
                );
            }
        }
        for (n, v) in &c.verdicts {
            let _ = writeln!(out, "- @{n}: {:?}, {:?}: {}", v.entailment, v.scope, v.note);
            if let Some(q) = &v.qualifier_quote {
                let _ = writeln!(out, "  Qualifier {}: > {}", q.source_id, q.quote);
            }
        }
    }
    out.push_str("\n## Flagged claims (separate from the brief)\n");
    for c in r
        .ledger
        .claims
        .iter()
        .filter(|c| c.status != TraceClaimStatus::Supported)
    {
        let _ = writeln!(out, "- [{}] {:?}: {}", c.claim_id, c.status, c.text);
    }
    out.push_str("\n## Quotes not found\n");
    for q in r.quote_checks.iter().filter(|q| q.span.is_none()) {
        let _ = writeln!(
            out,
            "- {} / {} / {}: quote_not_found: {}",
            q.member_label, q.local_id, q.quote.source_id, q.quote.quote
        );
    }
    out.push_str("\n## Skipped members\n");
    for n in &r.skipped {
        let _ = writeln!(out, "- {n}");
    }
    if details {
        out.push_str("\n## Numbered source packet\n");
        for s in &r.sources {
            let _ = writeln!(out, "\n### {}\n\n{}\n", s.source_id, s.text);
        }
        out.push_str("\n## Extraction and verification evidence\n");
        // Explicit typed human evidence only: no prompt bundles, seeds or credentials.
        for (title, value) in [
            ("Extractions", serde_json::to_value(&r.extractions)),
            ("Reader provenance", serde_json::to_value(&r.provenance)),
            (
                "Quote checks and UTF-8 byte spans",
                serde_json::to_value(&r.quote_checks),
            ),
            ("Canonical claims", serde_json::to_value(&r.claims)),
            ("Verifier assignments", serde_json::to_value(&r.assignments)),
            ("Every verdict", serde_json::to_value(&r.verifications)),
        ] {
            if let Ok(v) = value {
                let _ = writeln!(
                    out,
                    "\n### {title}\n\n```json\n{}\n```",
                    serde_json::to_string_pretty(&v).expect("typed evidence")
                );
            }
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
        .filter_map(|e| match &e.content {
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id
                    && a.artifact_kind.as_str() == "trace.report"
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                Some((e.commit.seq, a))
            }
            _ => None,
        })
        .max_by_key(|(cursor, _)| *cursor)
        .and_then(|(_, a)| a.body.as_ref())
        .ok_or(super::ExportError::ContestNotFound)?;
    let r = validation::decode::<TraceReport>(report)
        .map_err(|_| super::ExportError::InvalidSnapshot)?;
    Ok(report_text(&r, true))
}
