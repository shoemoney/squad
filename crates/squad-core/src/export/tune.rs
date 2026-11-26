//! Human report plus a reusable, complete tune archive from committed records.
use squad_protocol::tune::*;
use squad_protocol::*;
use std::fmt::Write;
pub fn unified_diff(original: &str, winner: &str) -> String {
    if original == winner {
        return String::new();
    }
    // A single complete hunk is a valid unified line diff and preserves every
    // byte of each line; common prefix/suffix lines are context within the hunk.
    let a: Vec<_> = original.split_inclusive('\n').collect();
    let b: Vec<_> = winner.split_inclusive('\n').collect();
    let mut out = format!(
        "--- original\n+++ winner\n@@ -1,{} +1,{} @@\n",
        a.len(),
        b.len()
    );
    for (prefix, lines) in [('-', a), ('+', b)] {
        for line in lines {
            out.push(prefix);
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    out
}
pub fn report(run: &squad_store::tune::TuneRun) -> String {
    let Some(report) = &run.report else {
        return "Tune is in progress. Committed work is retained below.\n".into();
    };
    let mut out = format!(
        "# TUNE\n\n{}\n\nReason: `{}`\n\n## Winning prompt\n\n{}\n\n## Diff\n\n```diff\n{}```\n",
        report.result_line, report.reason, report.prompt, report.diff
    );
    let mut lineage = vec![];
    let mut id = Some(report.winner.as_str());
    while let Some(current) = id {
        let Some(c) = run.candidates.iter().find(|c| c.id == current) else {
            break;
        };
        lineage.push(c);
        id = c.parent.as_deref();
    }
    out.push_str("\n## Lineage\n");
    for c in lineage.into_iter().rev() {
        let _ = writeln!(
            out,
            "\nGeneration {} · {}",
            c.generation,
            c.author
                .as_ref()
                .map_or("original".into(), ToString::to_string)
        );
        for change in &c.changes {
            let _ = writeln!(
                out,
                "- {} — {} ({})",
                change.what,
                change.why,
                change.targets.join(", ")
            );
        }
    }
    for holdout in [true, false] {
        let Some(record) = run.sheets.iter().rev().find(|s| s.holdout == holdout) else {
            continue;
        };
        let _ = writeln!(
            out,
            "\n## {}\n\n| Case | Criterion | Original | Winner |\n|---|---|---:|---:|",
            if holdout {
                "Held-out scores"
            } else {
                "Train scores (last sheet)"
            }
        );
        for case in &record.sheet.cases {
            let find = |candidate: &str| {
                case.outputs.iter().find(|o| {
                    record.maps[&case.case_label]
                        .get(&o.label)
                        .is_some_and(|id| id == candidate)
                })
            };
            let original = find(&run.candidates[0].id);
            let winner = find(&report.winner);
            if let (Some(a), Some(b)) = (original, winner) {
                for score in &a.scores {
                    if let Some(other) = b.scores.iter().find(|s| s.criterion == score.criterion) {
                        let _ = writeln!(
                            out,
                            "| {} | {} | {} | {} |",
                            case.case_label, score.criterion, score.score, other.score
                        );
                    }
                }
                let q = |o: &TuneOutputScore| {
                    o.scores.iter().map(|s| f64::from(s.score)).sum::<f64>()
                        / (4.0 * o.scores.len() as f64)
                };
                if q(a) - q(b) >= 0.25 - 1e-12 {
                    let _ = writeln!(
                        out,
                        "\nRegression: {} ({:.2} → {:.2}).\n",
                        case.case_label,
                        q(a),
                        q(b)
                    );
                }
                if holdout && b.scores.iter().any(|s| s.score <= 2) && !b.note.is_empty() {
                    let _ = writeln!(
                        out,
                        "\nRemaining failures · {}: {}\n",
                        case.case_label, b.note
                    );
                }
            }
        }
    }
    out.push_str("\n## Generations\n\n| Generation | Champion | Best variant | Promoted |\n|---|---|---|---|\n");
    for g in &run.generations {
        let score = |id: Option<&String>| {
            id.and_then(|id| g.scores.get(id))
                .map_or("empty".into(), |s| format!("{s:.2}"))
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            g.generation,
            score(Some(&g.champion)),
            score(g.best.as_ref()),
            g.promoted
        );
    }
    if let Some(suite) = &run.suite {
        for d in &suite.dropped {
            let _ = writeln!(out, "\nDropped case {}: {}", d.case_ref, d.why);
        }
    }
    if let Some(h) = &run.holdout {
        for d in &h.dropped {
            let _ = writeln!(
                out,
                "\nDropped held-out case {d}: missing finalist execution"
            );
        }
    }
    for c in &run.candidates {
        if c.incomplete {
            let _ = writeln!(out, "\nIncomplete candidate {}", c.id);
        }
    }
    for n in &run.skipped_members {
        let _ = writeln!(out, "\nSkipped member @{n}");
    }
    for w in &run.skipped {
        let _ = writeln!(out, "\nSkipped work {w}");
    }
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let archive = snapshot.events.iter().rev().find_map(|e| match &e.content {
        EnvelopePayload::ContestReport(r)
            if r.contest_id == id && r.data.get("tuneArchive").is_some() =>
        {
            Some(&r.data)
        }
        _ => None,
    });
    let Some(archive) = archive else {
        let events: Vec<_> = snapshot
            .events
            .iter()
            .filter(|e| matches!(&e.content,EnvelopePayload::ContestArtifact(a)if a.contest_id==id))
            .collect();
        return Ok(format!(
            "# TUNE {id}\n\nIn progress or stopped; committed artifacts:\n\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&events)
                .map_err(|_| super::ExportError::InvalidSnapshot)?
        ));
    };
    let run: squad_store::tune::TuneRun = serde_json::from_value(archive["tuneArchive"].clone())
        .map_err(|_| super::ExportError::InvalidSnapshot)?;
    let mut out = report(&run);
    out.push_str("\n## Reusable suite and full archive\n\nThe archive includes proposals, the frozen split, every candidate, execution, sheet label map, digest, lineage and skipped work.\n\n```json\n");
    out.push_str(
        &serde_json::to_string_pretty(archive).map_err(|_| super::ExportError::InvalidSnapshot)?,
    );
    out.push_str("\n```\n");
    Ok(out)
}
