//! Idempotent projection of committed debugging evidence.

use squad_protocol::*;
pub fn lines(s: &DebuggingState) -> Vec<String> {
    vec![
        "DEBUG".into(),
        format!("ROUND {} · {:?}", s.round, s.state),
        match s.state {
            DebuggingStatus::Hypothesizing | DebuggingStatus::Merging => "DIFFERENTIAL",
            DebuggingStatus::AwaitingHuman => {
                "RUN THE TEST · answer pending · /answer O1 <what you saw>"
            }
            DebuggingStatus::Eliminating => "RESULT",
            _ => "",
        }
        .into(),
        s.terminal_reason
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
    ]
}

#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<DebuggingState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::DebuggingState(s)
            | EnvelopePayload::ContestState(ContestState::Debugging(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || old.state_version < s.state_version
                }) {
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Debugging)
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(view: &View) -> Vec<String> {
    styled_lines(view)
        .into_iter()
        .map(|line| line.to_string())
        .collect()
}

pub fn styled_lines(view: &View) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::{
        style::{Color, Style},
        text::Line,
    };
    use squad_protocol::debugging::DebuggingHypothesisStatus;
    let Some(state) = &view.state else {
        return vec![];
    };
    let mut out: Vec<Line<'static>> = lines(state).into_iter().map(Line::from).collect();
    for a in view
        .artifacts
        .values()
        .filter(|a| a.contest_id == state.contest_id)
    {
        if let Some(body) = &a.body {
            match a.artifact_kind.as_str() {
                "debug.report" if body.get("brief").is_some() => {
                    if let Ok(r) = validation::decode::<debugging::DebuggingReport>(body) {
                        let eliminated = r
                            .differential
                            .iter()
                            .filter(|h| h.status == DebuggingHypothesisStatus::Eliminated)
                            .map(|h| format!("| {} |", h.hypothesis.hypothesis_id))
                            .collect::<Vec<_>>();
                        out.extend(
                            squad_core::export::debugging::report_text(&r, false)
                                .lines()
                                .map(|text| {
                                    let dim =
                                        eliminated.iter().any(|prefix| text.starts_with(prefix));
                                    Line::from(text.to_owned()).style(if dim {
                                        Style::default().fg(Color::DarkGray)
                                    } else {
                                        Style::default()
                                    })
                                }),
                        );
                    }
                }
                "debug.merge" => {
                    if let Ok(d) = validation::decode::<debugging::DebuggingDifferential>(body) {
                        out.push(Line::from("DIFFERENTIAL"));
                        for h in d.hypotheses {
                            out.push(Line::from(format!(
                                "{} | {} | live | 0",
                                h.hypothesis_id, h.cause
                            )));
                        }
                    }
                }
                "debug.elimination" => {
                    if let Ok(d) = validation::decode::<debugging::DebuggingElimination>(body) {
                        out.push(Line::from("RESULT · DIFFERENTIAL"));
                        for h in d.hypotheses {
                            let style = if h.status == DebuggingHypothesisStatus::Eliminated {
                                Style::default().fg(Color::DarkGray)
                            } else {
                                Style::default()
                            };
                            out.push(
                                Line::from(format!(
                                    "{} | {} | {:?} | {}",
                                    h.hypothesis.hypothesis_id,
                                    h.hypothesis.cause,
                                    h.status,
                                    h.consistent_results
                                ))
                                .style(style),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}
