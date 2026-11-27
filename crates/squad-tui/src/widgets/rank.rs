//! Human-visible ranking, gap markers and comparison progress.
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
    pub planned_judgments: usize,
    pub report_lines: Option<Vec<String>>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::RankSetup(s) => {
                self.planned_judgments = s.settings["judgments"].as_u64().unwrap_or(0) as usize;
            }
            EnvelopePayload::RankState(s)
            | EnvelopePayload::ContestState(ContestState::Rank(s)) => {
                if self
                    .state
                    .as_ref()
                    .map(|old| old.contest_id)
                    .or_else(|| self.artifacts.values().next().map(|a| a.contest_id))
                    .is_some_and(|old| old != s.contest_id)
                {
                    self.artifacts.clear();
                    self.planned_judgments = 0;
                    self.report_lines = None;
                }
                self.state = Some(s.clone());
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Rank)
                    && a.audience == Some(ArtifactAudience::Human)
                    && self
                        .state
                        .as_ref()
                        .is_none_or(|s| s.contest_id == a.contest_id) =>
            {
                if a.artifact_kind.as_str() == "rank.report"
                    && let Some(body) = &a.body
                    && let Ok(r) = validation::decode::<squad_protocol::rank::RankReport>(body)
                {
                    self.report_lines = Some(report_lines(&r));
                }
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(view: &View) -> Vec<String> {
    if let Some(lines) = &view.report_lines {
        return lines.clone();
    }
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
    }
    out.push(if view.planned_judgments > 0 {
        format!(
            "COMPARING {returned}/{} judgments returned",
            view.planned_judgments
        )
    } else {
        format!("COMPARING {returned} judgments returned")
    });
    out
}

fn report_lines(r: &squad_protocol::rank::RankReport) -> Vec<String> {
    let mut lines = vec![format!(
        "COMPARING {}/{} judgments returned",
        r.batches.values().map(|b| b.choices.len()).sum::<usize>(),
        r.setup.pairs.len() * r.setup.votes_per_pair
    )];
    lines.extend(
        squad_core::export::rank::report_text(r, false)
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("clear gap") {
                    format!("──────── {line}")
                } else if line.trim_start().starts_with("close gap") {
                    format!("········ {line}")
                } else {
                    line.to_owned()
                }
            }),
    );

    lines
}
