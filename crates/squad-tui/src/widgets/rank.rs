//! rank view projection belongs here.
use squad_protocol::*;
pub fn lines(state: &RankState) -> Vec<String> {
    vec![
        "RANK".into(),
        format!("{:?}", state.state),
        state
            .terminal_reason
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
    ]
}

#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<RankState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::RankState(s)
            | EnvelopePayload::ContestState(ContestState::Rank(s)) => self.state = Some(s.clone()),
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Rank) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(view: &View) -> Vec<String> {
    let mut out = view
        .state
        .as_ref()
        .map(lines)
        .unwrap_or_else(|| vec!["RANK".into()]);
    let mut returned = 0;
    for a in view.artifacts.values() {
        if a.artifact_kind.as_str() == "rank.compare" {
            returned += a
                .body
                .as_ref()
                .and_then(|b| b["choices"].as_array())
                .map_or(0, Vec::len);
        }
        if a.artifact_kind.as_str() == "rank.report"
            && let Some(body) = &a.body
            && let Ok(r) = serde_json::from_value::<squad_protocol::rank::RankReport>(body.clone())
        {
            let mut lines = vec![
                "RANK".into(),
                format!(
                    "COMPARING {}/{} judgments returned",
                    r.batches.values().map(|b| b.choices.len()).sum::<usize>(),
                    r.setup.pairs.len() * r.setup.votes_per_pair
                ),
            ];
            if let Some(t) = &r.tally {
                lines.push("ORDER".into());
                for (pos, &i) in t.order.iter().enumerate() {
                    let wins: u32 = t.wins[i].iter().sum();
                    let losses: u32 = t.wins.iter().map(|row| row[i]).sum();
                    lines.push(format!(
                        "{}. {} · {:.6} · {}–{}",
                        pos + 1,
                        r.setup.items[i].text,
                        t.scores[i],
                        wins,
                        losses
                    ));
                    if let Some(g) = t.gaps.get(pos) {
                        lines.push(
                            if g.label == "clear" {
                                "──────── clear"
                            } else {
                                "········ close"
                            }
                            .into(),
                        );
                    }
                }
                lines.push(format!(
                    "CYCLES {} ({} displayed)",
                    t.cycle_count,
                    t.cycles.len()
                ));
            }
            lines.extend(
                squad_core::export::rank::report_text(&r, false)
                    .lines()
                    .map(str::to_owned),
            );
            return lines;
        }
    }
    out.push(format!("COMPARING {returned} judgments returned"));
    out
}
