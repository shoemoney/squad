//! Negotiate committed-event projection. Draft content is collapsed by default.
use squad_protocol::*;
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<NegotiateState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::NegotiateState(s)
            | EnvelopePayload::ContestState(ContestState::Negotiate(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || old.state_version <= s.state_version
                }) {
                    if self
                        .state
                        .as_ref()
                        .is_some_and(|old| old.contest_id != s.contest_id)
                    {
                        self.artifacts.clear();
                    }
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Negotiate)
                    && self
                        .state
                        .as_ref()
                        .is_none_or(|s| s.contest_id == a.contest_id) =>
            {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn lines(s: &NegotiateState) -> Vec<String> {
    vec![
        "NEGOTIATE".into(),
        format!(
            "{:?}{}",
            s.state,
            s.terminal_reason
                .as_ref()
                .map(|r| format!(" · {r}"))
                .unwrap_or_default()
        ),
    ]
}
pub fn render(v: &View) -> Vec<String> {
    render_width(v, 80)
}
/// Cell callers with a measured width can render the three tables side by side.
pub fn render_width(v: &View, width: usize) -> Vec<String> {
    let mut out = v.state.as_ref().map(lines).unwrap_or_default();
    out.push("PREP".into());
    for a in v
        .artifacts
        .values()
        .filter(|a| a.artifact_kind.as_str() == "negotiate.roles")
    {
        if let Some(b) = &a.body
            && let Ok(setup) = validation::decode::<negotiate::NegotiateSetup>(b)
        {
            for t in setup.tables {
                out.push(format!(
                    "TABLE {}: @{} negotiator / @{} counterpart",
                    t.table, t.negotiator, t.counterpart
                ));
            }
        }
    }
    if !v
        .artifacts
        .values()
        .any(|a| a.artifact_kind.as_str() == "negotiate.report")
    {
        let mut turns = v
            .artifacts
            .values()
            .filter(|a| a.artifact_kind.as_str() == "negotiate.turn")
            .filter_map(|a| a.body.as_ref())
            .filter_map(|b| validation::decode::<negotiate::NegotiateTurn>(b).ok())
            .collect::<Vec<_>>();
        turns.sort_by_key(|t| (t.table, t.round, t.counterpart));
        for t in turns {
            out.push(format!(
                "TABLE {} · R{} · {}: {}",
                t.table,
                t.round,
                if t.counterpart {
                    "counterpart"
                } else {
                    "negotiator"
                },
                t.body["message"].as_str().unwrap_or_default()
            ));
        }
    }
    for a in v
        .artifacts
        .values()
        .filter(|a| a.artifact_kind.as_str() == "negotiate.report")
    {
        if let Some(b) = &a.body
            && let Ok(r) = validation::decode::<negotiate::NegotiateReport>(b)
        {
            let columns = r
                .tables
                .iter()
                .map(|t| {
                    let mut lines = vec![format!("TABLE {} · {:?}", t.table, t.outcome)];
                    for turn in &t.turns {
                        lines.push(format!(
                            "R{} {}: {}",
                            turn.round,
                            if turn.counterpart {
                                "counterpart"
                            } else {
                                "negotiator"
                            },
                            turn.body["message"].as_str().unwrap_or_default()
                        ));
                    }
                    lines
                })
                .collect::<Vec<_>>();
            if width >= 120 && columns.len() > 1 {
                let col = width / columns.len();
                let wrapped = columns
                    .iter()
                    .map(|lines| {
                        lines
                            .iter()
                            .flat_map(|s| wrap(s, col.saturating_sub(3)))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                for row in 0..wrapped.iter().map(Vec::len).max().unwrap_or(0) {
                    out.push(
                        wrapped
                            .iter()
                            .map(|c| {
                                format!(
                                    "{:<w$}",
                                    c.get(row).map(String::as_str).unwrap_or(""),
                                    w = col.saturating_sub(1)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("│"),
                    );
                }
            } else {
                for c in columns {
                    out.extend(c);
                }
            }
            out.push("PLAYBOOK".into());
            if r.no_rehearsal_evidence {
                out.push("No rehearsal evidence: all tables abandoned".into());
            }
            if let Some(p) = r.playbook {
                out.push(p.opening);
                out.push(format!("Walk away: {}", p.walk_away_line));
                out.extend(p.depends_on_assumptions);
            }
        }
    }
    out
}
fn wrap(s: &str, width: usize) -> Vec<String> {
    let chars = s.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        vec![String::new()]
    } else {
        chars
            .chunks(width.max(1))
            .map(|c| c.iter().collect())
            .collect()
    }
}
