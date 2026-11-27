//! Replay-safe Decide view; both tables are derived from committed arithmetic.
use squad_protocol::{decide::*, *};
pub fn lines(state: &DecideState) -> Vec<String> {
    vec![
        "DECIDE".into(),
        format!(
            "{} · version {}",
            squad_protocol::human::label(&state.state),
            state.state_version
        ),
        state
            .terminal_reason
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
    ]
}
#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<DecideState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
    pub leave_one_out: bool,
}
impl View {
    pub fn toggle(&mut self) {
        self.leave_one_out = !self.leave_one_out;
    }
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::DecideState(s)
            | EnvelopePayload::ContestState(ContestState::Decide(s)) => {
                if self.state.as_ref().is_none_or(|old| {
                    old.contest_id != s.contest_id || s.state_version >= old.state_version
                }) {
                    self.state = Some(s.clone());
                }
            }
            EnvelopePayload::ContestArtifact(a)
                if a.kind == Some(ContestKind::Decide)
                    && a.audience == Some(ArtifactAudience::Human) =>
            {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(view: &View) -> Vec<String> {
    let mut out = view.state.as_ref().map(lines).unwrap_or_default();
    let contest = view.state.as_ref().map(|s| s.contest_id);
    let artifacts = view
        .artifacts
        .values()
        .filter(|a| Some(a.contest_id) == contest)
        .collect::<Vec<_>>();
    if let Some(r) = artifacts
        .iter()
        .find(|a| a.artifact_kind.as_str() == "decide.report")
        .and_then(|a| validation::decode::<DecideReport>(a.body.as_ref()?).ok())
    {
        out.extend(
            squad_core::export::decide::report_text(&r)
                .lines()
                .map(str::to_owned),
        );
    } else if let Some(m) = artifacts
        .iter()
        .find(|a| a.artifact_kind.as_str() == "decide.matrix")
        .and_then(|a| validation::decode::<DecideMatrix>(a.body.as_ref()?).ok())
    {
        out.extend(
            squad_core::export::decide::matrix_text(&m)
                .lines()
                .map(str::to_owned),
        );
    } else {
        out.extend(["OPTIONS", "RUBRIC", "SCORES", "WHAT FLIPS IT"].map(str::to_owned));
    }
    if view.leave_one_out {
        let start = out.iter().position(|s| s == "WHAT FLIPS IT");
        if let Some(i) = start {
            let mut table = vec!["DECIDE · leave one criterion out".into()];
            table.extend_from_slice(&out[i..]);
            return table;
        }
    }
    out
}
