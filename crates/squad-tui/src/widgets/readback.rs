//! readback view projection belongs here.
use squad_protocol::*;
pub fn lines(_state: &ReadbackState) -> Vec<String> {
    vec![]
}

#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<ReadbackState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::ReadbackState(s)
            | EnvelopePayload::ContestState(ContestState::Readback(s)) => {
                self.state = Some(s.clone())
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Readback) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(_view: &View) -> Vec<String> {
    vec![]
}
