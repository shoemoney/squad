//! Human-only census evidence. Never serialize router maps or scheduling inputs.
use squad_protocol::*;
use std::fmt::Write;
pub fn report_text(report: &CensusReport) -> String {
    let mut out = format!(
        "CENSUS · pass {} · plan version {}\n\n{}\n{}\n\n",
        report.pass, report.plan_version, CENSUS_RANGE_UNAVAILABLE, CENSUS_CAVEAT
    );
    for gap in &report.gaps {
        let _ = writeln!(
            out,
            "- {:?}: {} — {}",
            gap.disposition, gap.gap.omission, gap.gap.consequence
        );
        let _ = writeln!(
            out,
            "  Finders ({}): {} · lens: {} · accepted ratings: {} (covered {}, trivial {})",
            gap.finders.len(),
            gap.finders
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            gap.gap
                .lenses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            gap.votes,
            gap.covered,
            gap.trivial
        );
        if gap.finders.len() == 1 && gap.disposition == CensusDisposition::Confirmed {
            out.push_str("  Singleton: the other members missed this gap.\n");
        }
        for citation in &gap.citations {
            let _ = writeln!(
                out,
                "  Covered citation · {}: {}",
                citation.location, citation.quote
            );
        }
    }
    for addition in &report.additions {
        let _ = writeln!(
            out,
            "- Rating-stage discovery: {} — {}",
            addition.omission, addition.consequence
        );
    }
    for missing in &report.missing {
        let _ = writeln!(out, "Missing: @{} · {}", missing.member, missing.step);
    }
    let _ = writeln!(
        out,
        "\nChao1 provenance: f1={}, f2={}, branch={}, unrounded U={}; input gaps: {}",
        report.f1,
        report.f2,
        report.formula_branch,
        report.unseen,
        report
            .input_gap_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    out.push_str("\nDiscovery curve\n");
    for point in &report.discovery {
        let _ = writeln!(
            out,
            "Pass {} · plan {}: new {}, cumulative {}, singletons {}",
            point.pass,
            point.plan_version,
            point.new_confirmed,
            point.cumulative_confirmed,
            point.singletons
        );
    }
    out.push_str(
        "\nSend singletons to $grill. Stop census first to release its active-contest claim.\n",
    );
    out
}
pub(super) fn render(out: &mut String, payload: &EnvelopePayload) -> bool {
    match payload {
        EnvelopePayload::CensusSetup(s) => {
            super::banner(out, crate::banner::Banner::Census);
            super::paragraph(
                out,
                &format!(
                    "Census {} · {} members · {} passes · {} normal calls · {} reserved attempts",
                    s.contest_id,
                    s.participants.len(),
                    s.passes,
                    s.normal_calls(),
                    s.reserved_calls()
                ),
            );
        }
        EnvelopePayload::CensusPlan(p) => {
            super::block(out, &format!("Plan version {}", p.version), p.text.as_str())
        }
        EnvelopePayload::CensusState(s) => super::paragraph(
            out,
            &format!(
                "Pass {} · plan {} · {:?}{}",
                s.pass,
                s.plan_version,
                s.status,
                s.terminal_reason
                    .as_ref()
                    .map(|r| format!(" · {r}"))
                    .unwrap_or_default()
            ),
        ),
        EnvelopePayload::CensusHunt(h) => {
            super::heading(
                out,
                &format!("Hunt · @{} · pass {} · {}", h.member, h.pass, h.lens),
            );
            for gap in &h.output.gaps {
                super::paragraph(
                    out,
                    &format!("{}: {} — {}", gap.local_id, gap.omission, gap.consequence),
                );
            }
        }
        EnvelopePayload::CensusLedger(l) => {
            super::heading(out, &format!("Anonymous ledger · pass {}", l.pass));
            for g in &l.output.ledger {
                super::paragraph(
                    out,
                    &format!("{}: {} — {}", g.id, g.omission, g.consequence),
                );
            }
        }
        EnvelopePayload::CensusRating(r) => {
            super::heading(out, &format!("Rating · @{} · pass {}", r.member, r.pass));
            for g in &r.output.ratings {
                super::paragraph(out, &format!("{}: {:?} — {}", g.gap_id, g.rating, g.reason));
                if let Some(c) = &g.citation {
                    super::paragraph(out, &format!("{}: {}", c.location, c.quote));
                }
            }
            for a in &r.output.additions {
                super::paragraph(
                    out,
                    &format!("Rating addition: {} — {}", a.omission, a.consequence),
                );
            }
        }
        EnvelopePayload::CensusReport(r) => super::paragraph(out, &report_text(r)),
        _ => return false,
    }
    true
}
