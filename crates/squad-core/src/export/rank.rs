//! Human report and full audit export. Never serialize private prompt/model maps.
use squad_protocol::{rank::*, *};
use std::fmt::Write;
pub fn report_text(report: &RankReport, full: bool) -> String {
    let mut out = format!(
        "# RANK\n\nCriterion: {}\n\nResult: {}\n\n",
        report.setup.criterion, report.reason
    );
    let s = &report.setup;
    let _ = writeln!(
        out,
        "Design: Q = {}, v = {}, {} planned judgments.\n",
        s.pairs.len(),
        s.votes_per_pair,
        s.pairs.len() * s.votes_per_pair
    );
    for (n, ds) in &s.deal {
        let _ = writeln!(out, "- @{n}: {} judgments", ds.len());
    }
    if let Some(t) = &report.tally {
        out.push_str("\n## ORDER\n\n");
        for (pos, &i) in t.order.iter().enumerate() {
            let wins: u32 = t.wins[i].iter().sum();
            let losses: u32 = t.wins.iter().map(|r| r[i]).sum();
            let _ = writeln!(
                out,
                "{}. {} — {} (score {:.9}; {wins}–{losses})",
                pos + 1,
                s.items[i].item_id,
                s.items[i].text,
                t.scores[i]
            );
            if let Some(g) = t.gaps.get(pos) {
                let _ = writeln!(
                    out,
                    "   {} gap ({:.1}% implied win chance)",
                    g.label,
                    g.probability * 100.0
                );
            }
        }
        if let Some(e) = &report.explanation {
            out.push_str("\n### Why the top items\n\n");
            for top in &e.top {
                let _ = writeln!(out, "- {}: {}", top.item_id, top.why);
            }
            let _ = writeln!(out, "\nCaveats: {}", e.caveats);
        }
        let _ = writeln!(
            out,
            "\n## CYCLES\n\n{} total; {} displayed, smallest combined majority margin first.\n",
            t.cycle_count,
            t.cycles.len()
        );
        for c in &t.cycles {
            let ids = c.items.map(|i| s.items[i].item_id.clone());
            let _ = writeln!(
                out,
                "- {}: {} → {} → {} → {} (margin {})",
                c.cycle_id, ids[0], ids[1], ids[2], ids[0], c.margin
            );
            if let Some(reading) = report
                .explanation
                .as_ref()
                .and_then(|e| e.cycles.iter().find(|r| r.cycle_id == c.cycle_id))
            {
                let _ = writeln!(out, "  {}", reading.reading);
            }
        }
        let _ = writeln!(
            out,
            "\nSplit pairs: {}\n",
            if t.split_pairs.is_empty() {
                "none".into()
            } else {
                t.split_pairs
                    .iter()
                    .map(|id| {
                        s.pairs.iter().find(|p| p.pair_id == *id).map_or_else(
                            || id.clone(),
                            |p| {
                                format!(
                                    "{}: {} vs {} ({}–{})",
                                    id,
                                    s.items[p.left].item_id,
                                    s.items[p.right].item_id,
                                    t.wins[p.left][p.right],
                                    t.wins[p.right][p.left]
                                )
                            },
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            }
        );
        if t.components.len() > 1 {
            out.push_str("Warning: order between components rests on no comparison.\n");
            for c in &t.components {
                let _ = writeln!(
                    out,
                    "- Component: {}",
                    c.iter()
                        .map(|i| s.items[*i].item_id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
        let _ = writeln!(
            out,
            "\nJudgments returned: {}/{}\n",
            t.judgments,
            s.pairs.len() * s.votes_per_pair
        );
        if full {
            out.push_str(
                "## Win matrix and fitted strengths\n\nRows beat columns; input item order.\n\n",
            );
            for (i, row) in t.wins.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "{}: {}; strength {:.17}; ln strength {:.17}",
                    s.items[i].item_id,
                    squad_protocol::human::label(&row),
                    t.strengths[i],
                    t.scores[i]
                );
            }
        }
    } else {
        out.push_str(
            "\nNo fitted order: insufficient accepted comparisons or stopped before aggregation.\n",
        );
    }
    if !report.skipped.is_empty() {
        let _ = writeln!(
            out,
            "\nSkipped at rank.compare: {}\n",
            report
                .skipped
                .iter()
                .map(|n| format!("@{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if full {
        out.push_str("\n## All judgments\n\n");
        for (nick, b) in &report.batches {
            for choice in &b.choices {
                if let Some(d) = s.deal[nick].iter().find(|d| d.pair_id == choice.pair_id) {
                    let _ = writeln!(
                        out,
                        "- @{nick} {}: A={} B={}; winner={}; {}",
                        choice.pair_id,
                        s.items[d.a].item_id,
                        s.items[d.b].item_id,
                        squad_protocol::human::label(&choice.winner),
                        choice.reason
                    );
                }
            }
        }
    }
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut setup = None;
    let mut state = None;
    let mut report = None;
    for e in &snapshot.events {
        match &e.content {
            EnvelopePayload::RankSetup(s) if s.contest_id == id => setup = Some(s),
            EnvelopePayload::RankState(s) if s.contest_id == id => state = Some(s),
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id
                    && a.kind == Some(ContestKind::Rank)
                    && a.audience == Some(ArtifactAudience::Human)
                    && a.artifact_kind.as_str() == "rank.report" =>
            {
                report = Some(
                    serde_json::from_value::<RankReport>(
                        a.body.clone().ok_or(super::ExportError::InvalidSnapshot)?,
                    )
                    .map_err(|_| super::ExportError::InvalidSnapshot)?,
                );
            }
            _ => {}
        }
    }
    if let Some(report) = report {
        return Ok(report_text(&report, true));
    }
    if setup.is_none() && state.is_none() {
        return Err(super::ExportError::ContestNotFound);
    }
    let mut out = format!("# RANK {id}\n\n");
    if let Some(s) = state {
        let _ = writeln!(out, "State: {}", squad_protocol::human::label(&s.state));
    }
    for e in &snapshot.events {
        if let EnvelopePayload::ContestArtifact(a) = &e.content
            && a.contest_id == id
            && a.kind == Some(ContestKind::Rank)
            && a.audience == Some(ArtifactAudience::Human)
        {
            let _ = writeln!(
                out,
                "\n## {}\n\n{}",
                a.artifact_kind,
                a.body.as_ref().ok_or(super::ExportError::InvalidSnapshot)?
            );
        }
    }
    Ok(out)
}
