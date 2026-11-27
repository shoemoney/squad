//! Deterministic human report plus the accepted evidence appendix.
use squad_protocol::forge::*;
use squad_protocol::*;
use std::fmt::Write;
pub fn report(r: &ForgeReport) -> String {
    let mut out = format!("# FORGE\n\n{}\n\n{}\n\n", verdict_line(r), r.body);
    out.push_str(
        "## Borda\n\n| Author | Mean score | First places | Flaw voters |\n|---|---:|---:|---:|\n",
    );
    for s in &r.tally.scores {
        let _ = writeln!(
            out,
            "| @{} | {:.6} | {} | {} |",
            s.author, s.score, s.first_places, s.flaw_voters
        );
    }
    out.push_str("\n## Provenance\n\n| Element | Source draft | Author |\n|---|---|---|\n");
    for p in &r.provenance {
        for label in &p.source_labels {
            let _ = writeln!(
                out,
                "| {} | {} | @{} |",
                p.element.replace('|', "\\|"),
                label,
                r.sources
                    .get(label)
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
        }
    }
    out.push_str("\n## Steals and fatal flaws\n\n");
    for n in &r.notes {
        let _ = writeln!(
            out,
            "- {}: {}{}",
            n.label,
            n.steal,
            n.fatal_flaw
                .as_ref()
                .map(|f| format!("; fatal flaw: {f}"))
                .unwrap_or_default()
        );
    }
    out.push_str("\n## Handled flaws\n\n");
    for f in &r.handled_flaws {
        let _ = writeln!(out, "- {}: {} — {}", f.label, f.flaw, f.handling);
    }
    if let Some(v) = &r.verdict {
        out.push_str("\n## Judge reasons and seams\n\n");
        for reason in &v.reasons {
            let _ = writeln!(out, "- {}: {}", reason.label, reason.reason);
        }
        for seam in &v.seam_issues {
            let _ = writeln!(out, "- {}: {}", seam.label, seam.issue);
        }
    }
    if !r.skipped.is_empty() {
        let _ = writeln!(out, "\n## Skipped\n\n{}", r.skipped.join("\n"));
    }
    out
}
pub fn verdict_line(r: &ForgeReport) -> String {
    if r.reason == "fusion_won" {
        format!("Fusion by @{} beat the best draft.", r.winner)
    } else {
        format!("The best draft by @{} stands. No fusion beat it.", r.winner)
    }
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut artifacts = vec![];
    let mut found = false;
    let mut state = None;
    let mut brief = None;
    let mut events = snapshot.events.iter().collect::<Vec<_>>();
    events.sort_by_key(|e| e.commit.seq);
    for e in events {
        match &e.content {
            EnvelopePayload::ForgeSetup(setup) if setup.contest_id == id => {
                found = true;
                brief = Some((
                    setup.settings["brief"].as_str().unwrap_or_default(),
                    setup.settings["constraints"].as_str().unwrap_or_default(),
                ));
            }
            EnvelopePayload::ForgeState(s) if s.contest_id == id => {
                found = true;
                state = Some(s);
            }
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id && a.kind == Some(ContestKind::Forge) =>
            {
                artifacts.push(a);
                found = true;
            }
            _ => {}
        }
    }
    if !found {
        return Err(super::ExportError::ContestNotFound);
    }
    let mut out = String::from("# FORGE\n\n");
    for a in &artifacts {
        if a.artifact_kind.as_str() == "forge.report" {
            let r: ForgeReport =
                serde_json::from_value(a.body.clone().ok_or(super::ExportError::InvalidSnapshot)?)
                    .map_err(|_| super::ExportError::InvalidSnapshot)?;
            out = report(&r);
            out.push_str("\n## Ballot label maps\n\n```json\n");
            out.push_str(
                &serde_json::to_string_pretty(&r.ballot_maps)
                    .map_err(|_| super::ExportError::InvalidSnapshot)?,
            );
            out.push_str("\n```\n");
        }
    }
    if let Some((text, constraints)) = brief {
        let _ = writeln!(out, "\n## Brief\n\n{text}\n\n{constraints}\n");
    }
    if let Some(s) = state {
        let _ = writeln!(
            out,
            "\nState: {}{}\n",
            squad_protocol::human::label(&s.state),
            s.terminal_reason
                .as_ref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default()
        );
    }
    for a in artifacts {
        if a.artifact_kind.as_str() == "forge.report" {
            continue;
        }
        let _ = writeln!(
            out,
            "\n## {} · {}\n\n```json\n{}\n```",
            a.artifact_kind,
            squad_protocol::human::actor_label(&a.actor),
            serde_json::to_string_pretty(&a.body)
                .map_err(|_| super::ExportError::InvalidSnapshot)?
        );
    }
    Ok(out)
}
