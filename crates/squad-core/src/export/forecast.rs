//! Human report and reproducible audit export from committed events only.
use squad_protocol::{forecast::*, *};
use std::fmt::Write;
pub fn report_text(r: &ForecastReport) -> String {
    let mut out = String::from("# FORECAST\n\n");
    let _ = writeln!(
        out,
        "{}\n\nResolution: {} — {}\nSource: {}\nVoid rule: {}\n",
        r.frame.question,
        r.frame.resolution_date,
        r.frame.resolution_criteria,
        r.frame.resolution_source,
        r.frame.void_rule
    );
    out.push_str(&aggregate_text(&r.final_aggregate));
    out.push_str("\n| Member | First | Final | Revision |\n|---|---:|---:|---|\n");
    for m in &r.members {
        let _ = writeln!(
            out,
            "| {} | {}% | {}% | {}{} |",
            m.member,
            m.first.probability_pct,
            m.final_probability_pct,
            if m.not_revised {
                "not revised"
            } else {
                "revised"
            },
            if m.uncited_move {
                "; uncited move (possible herding)"
            } else {
                ""
            }
        );
    }
    if let Some(c) = &r.cruxes {
        out.push_str("\n## Cruxes\n");
        for crux in &c.cruxes {
            let _ = writeln!(
                out,
                "- {} (higher: {}; lower: {})",
                crux.text,
                crux.higher_labels.join(", "),
                crux.lower_labels.join(", ")
            );
        }
        out.push_str("\n## Update if you see\n");
        for signpost in &c.signposts {
            let _ = writeln!(
                out,
                "- {} — {}, by {}",
                signpost.observable,
                squad_protocol::human::label(&signpost.direction),
                signpost.by_when
            );
        }
        out.push_str("\n## Shared assumptions\n\nModels share training data and blind spots.\n");
        for assumption in &c.shared_assumptions {
            let _ = writeln!(out, "- {assumption}");
        }
        out.push_str("\n## Weak evidence\n");
        for item in &c.weak_evidence {
            let _ = writeln!(out, "- {item}");
        }
    }
    if !r.skipped.is_empty() {
        let _ = writeln!(
            out,
            "\nSkipped first forecasts: {}",
            r.skipped
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    out
}
pub fn aggregate_text(a: &ForecastAggregate) -> String {
    let mut out = String::new();
    if let Some(split) = &a.split {
        let _ = writeln!(
            out,
            "The room is split.\n\nLower side: {:.1}%; higher side: {:.1}%.\n",
            100.0 * split.lower,
            100.0 * split.higher
        );
    }
    let _ = writeln!(
        out,
        "Pooled probability: **{:.1}%**\n\nSpread of the room: {:.1}%–{:.1}%; individual range: {}%–{}%.\nThe pool is not extremised. This spread is not a confidence interval.\n",
        100.0 * a.p,
        100.0 * a.low,
        100.0 * a.high,
        a.minimum_pct,
        a.maximum_pct
    );
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut events = snapshot
        .events
        .iter()
        .filter(|e| match &e.content {
            EnvelopePayload::ContestArtifact(a) => {
                a.contest_id == id && a.kind == Some(ContestKind::Forecast)
            }
            EnvelopePayload::ForecastState(s) => s.contest_id == id,
            EnvelopePayload::ForecastSetup(s) => s.contest_id == id,
            _ => false,
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|e| e.commit.seq);
    if events.is_empty() {
        return Err(super::ExportError::ContestNotFound);
    }
    let report = events.iter().rev().find_map(|e| match &e.content {
        EnvelopePayload::ContestArtifact(a) if a.artifact_kind.as_str() == "forecast.report" => a
            .body
            .clone()
            .and_then(|v| serde_json::from_value::<ForecastReport>(v).ok()),
        _ => None,
    });
    let mut out = report
        .as_ref()
        .map(report_text)
        .unwrap_or_else(|| "# FORECAST\n\nPartial run; accepted evidence retained.\n".into());
    for e in events {
        match &e.content {
            EnvelopePayload::ForecastState(s) => {
                if let Some(reason) = &s.terminal_reason {
                    let _ = writeln!(
                        out,
                        "\nOutcome: {} ({reason})",
                        squad_protocol::human::label(&s.state)
                    );
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.artifact_kind.as_str() != "forecast.report" =>
            {
                let body = a.body.as_ref().ok_or(super::ExportError::InvalidSnapshot)?;
                if a.artifact_kind.as_str() == "forecast.aggregate" && report.is_none() {
                    let aggregate: ForecastAggregate = serde_json::from_value(body.clone())
                        .map_err(|_| super::ExportError::InvalidSnapshot)?;
                    out.push_str(&aggregate_text(&aggregate));
                }
                let _ = writeln!(
                    out,
                    "\n## {} — {} — {}\n\n```json\n{}\n```",
                    a.artifact_kind,
                    squad_protocol::human::actor_label(&a.actor),
                    e.commit.accepted_at,
                    serde_json::to_string_pretty(body)
                        .map_err(|_| super::ExportError::InvalidSnapshot)?
                );
            }
            _ => {}
        }
    }
    Ok(out)
}
