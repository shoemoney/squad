//! Human-visible survey artifacts; no executor metadata or private routing maps.
use squad_protocol::*;
use std::fmt::Write;
pub fn render(payload: &EnvelopePayload) -> Option<String> {
    let mut out = String::new();
    match payload {
        EnvelopePayload::SurveySetup(s) => {
            writeln!(
                out,
                "## SURVEY {}\n\n{}\n\nSource: {}\n",
                s.contest_id,
                s.question,
                if s.collect {
                    "new question"
                } else {
                    "existing parallel answers"
                }
            )
            .ok()?;
            for a in &s.answers {
                writeln!(
                    out,
                    "### Source @{} · {}\n\n{}\n",
                    a.nick, a.message_id, a.text
                )
                .ok()?;
            }
        }
        EnvelopePayload::SurveyState(s) => {
            writeln!(
                out,
                "Survey state: {}\n",
                squad_protocol::human::label(&s.state)
            )
            .ok()?;
            if let Some(c) = &s.completeness {
                writeln!(out, "{}\n", c.summary()).ok()?;
            } else if let Some(reason) = &s.terminal_reason {
                writeln!(out, "{reason}\n").ok()?;
            }
        }
        EnvelopePayload::SurveyClaims(s) => {
            for c in &s.extraction.claims {
                writeln!(
                    out,
                    "### {} · {}\n\nOrigins: {}\n",
                    c.id,
                    c.claim,
                    c.source_answer_ids
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .ok()?;
            }
        }
        EnvelopePayload::SurveyVote(s) | EnvelopePayload::SurveyRevote(s) => {
            writeln!(
                out,
                "### {} · @{}\n",
                if matches!(payload, EnvelopePayload::SurveyRevote(_)) {
                    "Second ballot"
                } else {
                    "First ballot"
                },
                s.voter
            )
            .ok()?;
            for v in &s.ballot.votes {
                writeln!(
                    out,
                    "- {}: {} · confidence {}/5 · {}",
                    v.claim_id,
                    squad_protocol::human::label(&v.vote),
                    v.confidence,
                    v.reason
                )
                .ok()?;
                if let Some(e) = &v.change_evidence {
                    writeln!(out, "  Change evidence: {e}").ok()?;
                }
            }
        }
        EnvelopePayload::SurveySelection(s) => {
            out.push_str("### Selected claims and anonymous reasons\n\n");
            for c in &s.claims {
                writeln!(
                    out,
                    "{} · {}\n\nAGREE: {}\n\nDISAGREE: {}\n\nUNSURE: {}\n",
                    c.claim_id,
                    c.claim,
                    c.agree_reasons.join("; "),
                    c.disagree_reasons.join("; "),
                    c.unsure_reasons.join("; ")
                )
                .ok()?;
            }
        }
        EnvelopePayload::SurveyReport(s) => {
            out.push_str("## Survey claim map\n\n");
            if let Some(summary) = s.participation_summary() {
                writeln!(out, "{summary}\n").ok()?;
            }
            for (label, entries) in [
                ("Consensus", &s.report.consensus),
                ("Contested", &s.report.contested),
                ("Lone insights", &s.report.lone_insights),
                ("Lone dissents", &s.report.lone_dissents),
                (
                    "Rejected outliers · likely errors, verify",
                    &s.report.rejected_outliers,
                ),
            ] {
                writeln!(out, "### {label}\n").ok()?;
                if entries.is_empty() {
                    out.push_str("None\n\n");
                }
                for e in entries {
                    writeln!(
                        out,
                        "- {}: {}\n  Settling question: {}",
                        e.claim_id, e.explanation, e.settling_question
                    )
                    .ok()?;
                }
            }
            for m in &s.map {
                let first = &m.first;
                let t = &m.final_tally;
                writeln!(out,"### {}{}\n\nFirst: AGREE {} · DISAGREE {} · UNSURE {} · MISSING {}\n\nFinal: AGREE {} · DISAGREE {} · UNSURE {} · MISSING {}\n",m.claim_id,if m.bands.is_empty(){" · unresolved"}else{""},first.agree,first.disagree,first.unsure,first.missing,t.agree,t.disagree,t.unsure,t.missing).ok()?;
                for s in &m.switches {
                    writeln!(
                        out,
                        "Switch @{}: {} → {}",
                        s.voter,
                        squad_protocol::human::label(&s.from),
                        squad_protocol::human::label(&s.to)
                    )
                    .ok()?;
                }
                if !m.not_revoted.is_empty() {
                    writeln!(
                        out,
                        "Not re-voted (retained first ballot): {}",
                        m.not_revoted
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                    .ok()?;
                }
            }
            writeln!(out, "\nLimitations: {}\n", s.report.limitations).ok()?;
        }
        _ => return None,
    }
    Some(out)
}
