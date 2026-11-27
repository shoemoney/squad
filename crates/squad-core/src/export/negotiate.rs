//! Human report and complete rehearsal evidence, rendered from committed events only.
use squad_protocol::negotiate::*;
use squad_protocol::*;
use std::fmt::Write;
fn section(out: &mut String, title: &str, value: &impl serde::Serialize) {
    let _ = writeln!(
        out,
        "\n## {title}\n\n```json\n{}\n```",
        serde_json::to_string_pretty(value).expect("protocol serialization")
    );
}
pub fn report(r: &NegotiateReport) -> String {
    let mut out = String::from(
        "# NEGOTIATE\n\nRehearsal with modelled counterparts. Hypothetical counterpart, not a fact.\n",
    );
    let _ = writeln!(
        out,
        "\nOur side: {}\n\nKnown counterpart information: {}",
        r.setup.our_side, r.setup.counterpart
    );
    out.push_str("\n## Role draw\n\n| Table | Negotiator | Counterpart | Outcome | Rounds |\n|---|---|---|---|---|\n");
    for t in &r.tables {
        let _ = writeln!(
            out,
            "| {} | @{} | @{} | {:?} | {} |",
            t.table, t.negotiator, t.counterpart, t.outcome, t.rounds_used
        );
    }
    section(&mut out, "Outcome counts", &r.counts);
    if r.no_rehearsal_evidence {
        out.push_str("\nNo rehearsal evidence: every table was abandoned. The playbook is based on preparation only.\n");
    }
    section(
        &mut out,
        "Interest maps — supplied and inferred; BATNA and ZOPA",
        &r.maps,
    );
    if let Some(b) = &r.brief {
        let _ = writeln!(
            out,
            "\n## Strategy sheet\n\n{}\n\n## Public scenario\n\n{}",
            b.strategy_sheet, b.public_scenario
        );
        section(
            &mut out,
            "Assumptions to confirm",
            &b.assumptions_to_confirm,
        );
        for (i, brief) in b.counterpart_briefs.iter().enumerate() {
            section(
                &mut out,
                &format!("Table {} — Hypothetical counterpart, not a fact", i + 1),
                brief,
            );
        }
    }
    section(&mut out, "Packages", &r.packages);
    for t in &r.tables {
        let _ = writeln!(out, "\n## TABLE {} — {:?}\n", t.table, t.outcome);
        for turn in &t.turns {
            let _ = writeln!(
                out,
                "\nRound {} · {} · @{}\n\n```json\n{}\n```",
                turn.round,
                if turn.counterpart {
                    "counterpart"
                } else {
                    "negotiator"
                },
                turn.actor,
                serde_json::to_string_pretty(&turn.body).expect("turn")
            );
        }
    }
    if let Some(p) = &r.playbook {
        let _ = writeln!(
            out,
            "\n## PLAYBOOK\n\n### Opening\n\n{}\n\n### Walk-away line\n\n{}",
            p.opening, p.walk_away_line
        );
        section(&mut out, "Anchors", &p.anchors);
        section(
            &mut out,
            "Concession ladder — give only for",
            &p.concession_ladder,
        );
        section(&mut out, "Objections and responses", &p.objections);
        section(&mut out, "ZOPA map — modelled limits", &p.zopa_map);
        section(&mut out, "Questions to ask", &p.questions_to_ask);
        section(
            &mut out,
            "Recommendations depending on assumptions",
            &p.depends_on_assumptions,
        );
        section(
            &mut out,
            "Table notes and reservation breaches",
            &p.table_notes,
        );
        for t in r
            .tables
            .iter()
            .filter(|t| t.outcome == Some(NegotiateOutcome::Deal))
        {
            let flagged = p
                .table_notes
                .iter()
                .find(|n| n.table == t.table)
                .is_some_and(|n| !n.unrealistic_concessions.is_empty());
            let _ = writeln!(
                out,
                "\nTable {} accepted deal: hidden reservation breach flagged by judge: {}.",
                t.table, flagged
            );
        }
    }
    if !r.skipped.is_empty() {
        section(&mut out, "Skipped members and steps", &r.skipped);
    }
    out
}
pub fn render(
    snapshot: &super::ExportSnapshot,
    id: ContestId,
) -> Result<String, super::ExportError> {
    let mut events = snapshot.events.iter().collect::<Vec<_>>();
    events.sort_by_key(|e| e.commit.seq);
    let mut found = false;
    let mut state = None;
    let mut artifacts = vec![];
    let mut out = String::from("# NEGOTIATE\n\nHypothetical counterpart, not a fact.\n");
    for e in events {
        match &e.content {
            EnvelopePayload::NegotiateSetup(s) if s.contest_id == id => {
                found = true;
                section(&mut out, "Setup and role draw", &s.settings);
            }
            EnvelopePayload::NegotiateState(s) if s.contest_id == id => {
                found = true;
                state = Some(s);
            }
            EnvelopePayload::ContestArtifact(a)
                if a.contest_id == id && a.kind == Some(ContestKind::Negotiate) =>
            {
                found = true;
                if a.artifact_kind.as_str() == "negotiate.report" {
                    let r: NegotiateReport = serde_json::from_value(
                        a.body.clone().ok_or(super::ExportError::InvalidSnapshot)?,
                    )
                    .map_err(|_| super::ExportError::InvalidSnapshot)?;
                    out = report(&r);
                } else {
                    artifacts.push(a);
                }
            }
            _ => {}
        }
    }
    if !found {
        return Err(super::ExportError::ContestNotFound);
    }
    if let Some(s) = state {
        let _ = writeln!(out, "\nState: {:?} · {:?}", s.state, s.terminal_reason);
    }
    for a in artifacts {
        if a.artifact_kind.as_str() == "negotiate.roles" {
            continue;
        }
        section(
            &mut out,
            &format!(
                "{} · {:?} · round {}",
                a.artifact_kind,
                a.actor,
                a.round.unwrap_or(0)
            ),
            &a.body,
        );
    }
    Ok(out)
}
