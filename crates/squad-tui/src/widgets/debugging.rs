//! debug view projection belongs here.
use squad_protocol::*;
pub fn lines(_state: &DebuggingState) -> Vec<String> {
    vec![]
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
                self.state = Some(s.clone())
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Debugging) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(_view: &View) -> Vec<String> {
    vec![]
}
