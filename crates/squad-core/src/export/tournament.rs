use super::{heading, paragraph};
use squad_protocol::*;
pub(super) fn render(out: &mut String, payload: &EnvelopePayload) -> bool {
    match payload {
        EnvelopePayload::TournamentSetup(s) => {
            super::banner(out, crate::banner::Banner::Tournament);
            heading(out, &format!("Tournament {}", s.contest_id));
            paragraph(out, &s.prompt);
            paragraph(
                out,
                &format!(
                    "Entrants: {} · judge {} · {} rounds",
                    s.entrants
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    s.judge_model,
                    s.total_rounds
                ),
            );
        }
        EnvelopePayload::TournamentPreflight(p) => {
            paragraph(
                out,
                &format!(
                    "Planned calls: {} · estimated ${:.4} · reserved ${:.4} including retries and one replay per round",
                    p.base.estimate.model_calls,
                    p.base.estimate.estimated_usd,
                    p.base.estimate.reserved_usd
                ),
            );
        }
        EnvelopePayload::TournamentState(s) => {
            if s.state == TournamentStatus::Answering {
                super::banner(
                    out,
                    crate::banner::Banner::TournamentRound {
                        round: s.round as usize,
                        remaining: s.survivors.len(),
                    },
                );
            }

            heading(
                out,
                &format!(
                    "Tournament {} · round {} · replay {}",
                    squad_protocol::human::label(&s.state),
                    s.round,
                    s.replay
                ),
            );
            if let Some(reason) = &s.terminal_reason {
                paragraph(out, reason);
            }
        }
        EnvelopePayload::TournamentEntry(e) => {
            heading(
                out,
                &format!(
                    "Round {} · replay {} · {} · attempt {}",
                    e.round, e.replay, e.entrant, e.generation.attempt
                ),
            );
            match &e.outcome {
                DebateTurnOutcome::Completed { content, .. } => paragraph(out, content),
                DebateTurnOutcome::Forfeit { text, .. } => {
                    paragraph(out, &format!("Forfeit: {text}"))
                }
            }
        }
        EnvelopePayload::TournamentRanking(r) => {
            heading(
                out,
                &format!("Accepted ranking · round {} · replay {}", r.round, r.replay),
            );
            for row in &r.ranking {
                paragraph(
                    out,
                    &format!(
                        "{}. {} · {} · {}{}",
                        row.rank,
                        row.nick,
                        row.score
                            .map_or_else(|| "forfeit".into(), |s| format!("{s}/10")),
                        row.critique.as_deref().unwrap_or("No critique (forfeit)"),
                        if row.rank == r.ranking.len() as u32 {
                            " · ELIMINATED"
                        } else {
                            ""
                        }
                    ),
                );
            }
        }
        EnvelopePayload::TournamentResult(r) => {
            if !r.survived {
                paragraph(
                    out,
                    &format!(
                        "Eliminated: {} · placement {}",
                        r.entrant, r.remaining_before
                    ),
                );
            }
        }
        EnvelopePayload::TournamentChampion(c) => {
            super::banner(out, crate::banner::Banner::Champion);
            heading(
                out,
                &format!("Champion: {} · trophies {}", c.champion, c.trophies),
            );
            for p in &c.placements {
                paragraph(out, &format!("{}. {}", p.placement, p.nick));
            }
        }
        _ => return false,
    }
    true
}
