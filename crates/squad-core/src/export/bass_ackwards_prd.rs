//! Deterministic house-style PRD and complete committed contest export.
use squad_protocol::bass_ackwards_prd::identity;
use squad_protocol::bass_ackwards_prd::*;
use squad_protocol::*;
use std::{collections::BTreeMap, fmt::Write};
pub fn render_document(
    s: &BassAckwardsPrdSetup,
    done: &BassAckwardsPrdDone,
    d: &BassAckwardsPrdDocument,
    c: &BassAckwardsPrdChecks,
) -> String {
    let mut numbers = BTreeMap::new();
    for (mi, m) in d.milestones.iter().enumerate() {
        for (pi, p) in m.phases.iter().enumerate() {
            for (li, l) in p.line_items.iter().enumerate() {
                numbers.insert(
                    identity(&l.key, &l.local_id).to_owned(),
                    format!("M{}.P{}.L{}", mi + 1, pi + 1, li + 1),
                );
            }
        }
    }
    let refs = |ids: &[String]| {
        ids.iter()
            .map(|id| numbers.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect::<Vec<_>>()
            .join(" → ")
    };
    let checkline = if c.mapped && c.covered && c.dependencies && c.acyclic {
        "Checks passed: 4/4."
    } else {
        "Checks have findings; see Appendix → Checks."
    };
    let mut out = format!(
        "# {}\n\n{}\n\n**End result:** {}\n**Starting point:** {}\n\n## Overview\n{}\n\n## Definition of done\n",
        d.title,
        checkline,
        s.end_result,
        s.today.as_deref().unwrap_or("Not stated."),
        d.overview
    );
    for item in &done.items {
        let _ = writeln!(
            out,
            "- {} — {} (verified by: {}; support: {})",
            item.id, item.test, item.verified_by, item.support
        );
    }
    let _ = write!(
        out,
        "\n## Dependency order\n**Critical path:** {}\n**Can run in parallel:** {}\n\n## Milestones\n",
        refs(&c.critical_path),
        c.levels
            .iter()
            .enumerate()
            .map(|(i, l)| format!("level {}: {}", i + 1, refs(l)))
            .collect::<Vec<_>>()
            .join(" · ")
    );
    for (mi, m) in d.milestones.iter().enumerate() {
        let serves = m
            .phases
            .iter()
            .flat_map(|p| &p.line_items)
            .flat_map(|l| l.serves.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        let _ = write!(
            out,
            "\n### M{} — {}\n**Goal:** {}\n**Serves:** {}\n",
            mi + 1,
            m.title,
            m.goal,
            serves
        );
        for (pi, p) in m.phases.iter().enumerate() {
            let _ = write!(out, "\n#### Phase M{}.P{} — {}\n", mi + 1, pi + 1, p.title);
            for l in &p.line_items {
                let _ = writeln!(
                    out,
                    "- [ ] {} — {} (owner: {}; needs: {}; after: {}; done: {}; risk: {})",
                    numbers[identity(&l.key, &l.local_id)],
                    l.text,
                    l.owner_role,
                    l.precondition,
                    refs(&l.depends_on),
                    l.serves.join(", "),
                    l.risk
                );
            }
        }
    }
    out.push_str("\n## Open questions\n");
    for q in &d.open_questions {
        let _ = writeln!(out, "- {q}");
    }
    out.push_str("\n## Appendix\n\n### Single-source done items\n");
    for g in &done.single_source {
        let _ = writeln!(out, "- {}", g.text);
    }
    let _ = write!(
        out,
        "\n### Checks\n- Every line maps to DoD: {}\n- Every DoD item served: {}\n- No missing dependency: {}\n- No dependency cycle: {}\n- Uncovered DoD: {}\n",
        c.mapped,
        c.covered,
        c.dependencies,
        c.acyclic,
        c.uncovered.join(", ")
    );
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut records = vec![];
    let mut report = None;
    let mut saved = None;
    for e in &snapshot.events {
        if let EnvelopePayload::ContestArtifact(a) = &e.content
            && a.contest_id == id
            && a.kind == Some(ContestKind::BassAckwardsPrd)
        {
            let body = a.body.as_ref().ok_or(super::ExportError::InvalidSnapshot)?;
            if a.artifact_kind.as_str() == "bass_ackwards_prd.report" {
                report = Some(
                    serde_json::from_value::<BassAckwardsPrdReport>(body.clone())
                        .map_err(|_| super::ExportError::InvalidSnapshot)?,
                );
            }
            if a.artifact_kind.as_str() == "bass_ackwards_prd.saved" {
                saved = Some(
                    serde_json::from_value::<BassAckwardsPrdSaved>(body.clone())
                        .map_err(|_| super::ExportError::InvalidSnapshot)?,
                );
            }
            records.push((a.artifact_kind.to_string(), body.clone()));
        }
    }
    if records.is_empty() {
        return Err(super::ExportError::ContestNotFound);
    }
    let mut out = String::new();
    if let Some(saved) = saved {
        let _ = writeln!(out, "PRD saved to {}\n", saved.path);
    }
    if let Some(r) = report {
        if let Some(e) = r.save_error {
            let _ = writeln!(
                out,
                "Could not save the PRD. Use /export bass_ackwards_prd {id}.\n{e}\n"
            );
        }
        out.push_str(&r.markdown);
    }
    out.push_str("\n## Full contest record\n");
    for (kind, body) in records {
        let _ = write!(
            out,
            "\n### {kind}\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&body).map_err(|_| super::ExportError::InvalidSnapshot)?
        );
    }
    Ok(out)
}
