//! Presentation consumes only committed tournament records, in replay order.
use super::tournament::*;
use crate::fx::FxCue;
use squad_protocol::*;
impl TournamentView {
    fn round_mut(&mut self, number: u32, replay: u8) -> &mut Round {
        let tag = (replay > 0).then(|| "replay 1".to_string());
        if !self
            .rounds
            .iter()
            .any(|r| r.number == number && r.replay == tag)
        {
            self.rounds.push(Round {
                number,
                remaining: self.survivors.len(),
                expanded: true,
                replay: tag.clone(),
                entries: self
                    .entrants
                    .iter()
                    .filter(|n| self.survivors.contains(&n.nick))
                    .map(|n| Entry {
                        nick: n.nick.clone(),
                        status: "waiting".into(),
                        text: String::new(),
                    })
                    .collect(),
                ranking: vec![],
            });
        }
        self.rounds
            .iter_mut()
            .find(|r| r.number == number && r.replay == tag)
            .expect("inserted round")
    }
    pub fn apply_payload(&mut self, payload: &EnvelopePayload) -> Option<FxCue> {
        match payload {
            EnvelopePayload::TournamentSetup(s) => {
                self.id = Some(s.contest_id);
                self.prompt = s.prompt.clone();
                self.judge_model = s.judge_model.clone();
                self.entrants = s
                    .entrants
                    .iter()
                    .map(|n| Entrant {
                        nick: n.to_string(),
                        ..Default::default()
                    })
                    .collect();
                self.survivors = s.entrants.iter().map(ToString::to_string).collect();
                self.expanded = true;
            }
            EnvelopePayload::TournamentState(s) => {
                self.id = Some(s.contest_id);
                self.current_round = s.round;
                self.state = s.state.as_str().into();
                self.reason = s.terminal_reason.clone();
                self.survivors = s.survivors.iter().map(ToString::to_string).collect();
                for n in &mut self.entrants {
                    n.eliminated = s.eliminated.iter().any(|e| e.as_str() == n.nick);
                }
                if s.state == TournamentStatus::Answering {
                    self.round_mut(s.round, s.replay);
                }
            }
            EnvelopePayload::TournamentEntry(e) => {
                let round = self.round_mut(e.round, e.replay);
                if let Some(slot) = round
                    .entries
                    .iter_mut()
                    .find(|s| s.nick == e.entrant.as_str())
                {
                    match &e.outcome {
                        DebateTurnOutcome::Completed { content, .. } => {
                            slot.text = content.clone();
                            slot.status = "complete".into();
                        }
                        DebateTurnOutcome::Forfeit { .. } => slot.status = "forfeit".into(),
                    }
                }
            }
            EnvelopePayload::TournamentRanking(r) => {
                let round = self.round_mut(r.round, r.replay);
                round.ranking = r
                    .ranking
                    .iter()
                    .map(|row| Ranking {
                        nick: row.nick.to_string(),
                        score: row.score.map_or(0, i64::from),
                        critique: row.critique.clone().unwrap_or_else(|| "forfeit".into()),
                        eliminated: row.rank == r.ranking.len() as u32,
                    })
                    .collect();
                return Some(FxCue::TournamentRankingReady {
                    contest_id: r.contest_id.to_string().parse().unwrap(),
                    round: r.round,
                });
            }
            EnvelopePayload::TournamentResult(r) if !r.survived => {
                for n in &mut self.entrants {
                    if n.nick == r.entrant.as_str() {
                        n.eliminated = true;
                    }
                }
                return Some(FxCue::TournamentEliminated {
                    contest_id: r.contest_id.to_string().parse().unwrap(),
                    member: r.entrant.clone(),
                });
            }
            EnvelopePayload::TournamentChampion(c) => {
                self.champion = Some(c.champion.to_string());
                self.placements = c.placements.iter().map(|p| p.nick.to_string()).collect();
                for n in &mut self.entrants {
                    if n.nick == c.champion.as_str() {
                        n.trophies = c.trophies.min(u64::from(u32::MAX)) as u32;
                    }
                }
                return Some(FxCue::TournamentChampion {
                    contest_id: c.contest_id.to_string().parse().unwrap(),
                    member: c.champion.clone(),
                });
            }
            _ => {}
        }
        None
    }
}
