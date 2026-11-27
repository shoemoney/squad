//! Human report and complete committed provenance for Decide.
use squad_protocol::{decide::*, *};
use std::fmt::Write;
pub fn matrix_text(m: &DecideMatrix) -> String {
    let mut s = String::new();
    if let Some(w) = &m.winner {
        writeln!(s, "Winner: {w}").expect("string");
    } else {
        writeln!(
            s,
            "Too close to call between {} and {}.",
            m.ranking[0].option_id, m.ranking[1].option_id
        )
        .expect("string");
    }
    writeln!(
        s,
        "{} · margin {:.4} · {} members rank the leader first",
        if m.robust { "robust" } else { "fragile" },
        m.margin,
        m.member_support
    )
    .expect("string");
    s.push_str("\n| Option | Total |\n|---|---:|\n");
    for t in &m.ranking {
        writeln!(s, "| {} | {:.4} |", t.option_id, t.total).expect("string");
    }
    s.push_str("\nSCORES (* contested: score range ≥ 4)\n\n| Criterion |");
    for t in &m.ranking {
        write!(s, " {} |", t.option_id).expect("string");
    }
    s.push_str("\n|---|");
    for _ in &m.ranking {
        s.push_str("---:|");
    }
    s.push('\n');
    let mut cs = vec![];
    for c in &m.cells {
        if !cs.contains(&c.criterion_id) {
            cs.push(c.criterion_id.clone());
        }
    }
    for c in cs {
        write!(s, "| {c} |").expect("string");
        for o in &m.ranking {
            let cell = m
                .cells
                .iter()
                .find(|x| x.option_id == o.option_id && x.criterion_id == c)
                .expect("complete");
            write!(
                s,
                " {:.2}{} |",
                cell.median,
                if cell.contested { "*" } else { "" }
            )
            .expect("string");
        }
        s.push('\n');
    }
    s.push_str("\nWHAT FLIPS IT\n\n| Removed criterion | Ranking |\n|---|---|\n");
    for l in &m.leave_one_out {
        writeln!(
            s,
            "| {} | {} |",
            l.criterion_id,
            l.ranking
                .iter()
                .map(|x| format!("{} ({:.4})", x.option_id, x.total))
                .collect::<Vec<_>>()
                .join(" > ")
        )
        .expect("string");
    }
    s.push_str("\n| Criterion | Rival | Overtakes when weight |\n|---|---|---:|\n");
    for f in &m.flips {
        writeln!(
            s,
            "| {} | {} | {} {:.4}% |",
            f.criterion_id,
            f.rival_option_id,
            if f.above { "above" } else { "below" },
            100.0 * f.threshold
        )
        .expect("string");
    }
    s
}
pub fn report_text(r: &DecideReport) -> String {
    let mut s = String::from("DECIDE\n\nOPTIONS\n");
    for o in &r.options {
        writeln!(s, "- {}: {} — {}", o.option_id, o.name, o.description).expect("string");
    }
    s.push_str("\nRUBRIC\n\n| Criterion | Definition | Weight | Support |\n|---|---|---:|---:|\n");
    for c in &r.rubric.kept {
        writeln!(
            s,
            "| {}: {} | {} | {:.0}% | {} |",
            c.criterion.criterion_id,
            c.criterion.name,
            c.criterion.definition,
            c.weight * 100.0,
            c.support
        )
        .expect("string");
    }
    s.push('\n');
    s.push_str(&matrix_text(&r.matrix));
    s.push_str("\nDropped criteria\n");
    for c in &r.rubric.dropped {
        writeln!(
            s,
            "- {} (support {}): {}",
            c.criterion.name,
            c.support,
            c.criterion
                .sources
                .iter()
                .map(|x| format!(
                    "{}:{}",
                    r.proposers
                        .get(&x.member_label)
                        .map(ToString::to_string)
                        .unwrap_or_else(|| x.member_label.clone()),
                    x.local_id
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .expect("string");
    }
    if let Some(e) = &r.explanation {
        s.push_str("\nFlip conditions\n");
        for f in &e.flip_conditions {
            writeln!(
                s,
                "- {} / {}: {}",
                f.criterion_id, f.rival_option_id, f.text
            )
            .expect("string");
        }
        s.push_str("\nDissents\n");
        for d in &e.dissents {
            writeln!(
                s,
                "- {}: {} ({})",
                d.option_id,
                d.text,
                d.cell_refs
                    .iter()
                    .map(|r| format!("{}/{}", r.option_id, r.criterion_id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string");
        }
        s.push_str("\nEvidence gaps\n");
        for gap in &e.evidence_gaps {
            writeln!(s, "- {gap}").expect("string");
        }
    }
    s.push_str("\nSkipped members\n");
    for x in &r.skipped {
        writeln!(s, "- {x}").expect("string");
    }
    s
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut events = snapshot.events.iter().collect::<Vec<_>>();
    events.sort_by_key(|e| e.commit.seq);
    let found = events
        .iter()
        .any(|e| matches!(&e.content,EnvelopePayload::DecideSetup(s) if s.contest_id==id));
    if !found {
        return Err(super::ExportError::ContestNotFound);
    }
    let mut out = format!("# Decide {id}\n\n");
    for e in &events {
        if let EnvelopePayload::DecideState(s) = &e.content
            && s.contest_id == id
            && let Some(reason) = &s.terminal_reason
        {
            writeln!(out, "Status: {:?} ({reason})\n", s.state).expect("string");
        }
    }
    let artifacts = events
        .iter()
        .filter_map(|e| match &e.content {
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id
                    && a.kind == Some(ContestKind::Decide)
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                Some(a)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if let Some(a) = artifacts
        .iter()
        .find(|a| a.artifact_kind.as_str() == "decide.report")
    {
        let r: DecideReport =
            serde_json::from_value(a.body.clone().ok_or(super::ExportError::InvalidSnapshot)?)
                .map_err(|_| super::ExportError::InvalidSnapshot)?;
        out.push_str(&report_text(&r));
    } else if let Some(a) = artifacts
        .iter()
        .find(|a| a.artifact_kind.as_str() == "decide.matrix")
    {
        let m: DecideMatrix =
            serde_json::from_value(a.body.clone().ok_or(super::ExportError::InvalidSnapshot)?)
                .map_err(|_| super::ExportError::InvalidSnapshot)?;
        out.push_str(&matrix_text(&m));
    }
    out.push_str("\n## Committed proposals and tally inputs\n\n");
    for a in artifacts {
        writeln!(
            out,
            "### {} · {:?}\n\n```json\n{}\n```\n",
            a.artifact_kind,
            a.actor,
            serde_json::to_string_pretty(
                a.body.as_ref().ok_or(super::ExportError::InvalidSnapshot)?
            )
            .map_err(|_| super::ExportError::InvalidSnapshot)?
        )
        .expect("string");
    }
    Ok(out)
}
