//! negotiate view projection belongs here.
use squad_protocol::*;
pub fn lines(_state: &NegotiateState) -> Vec<String> {
    vec![]
}

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
                self.state = Some(s.clone())
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Negotiate) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(_view: &View) -> Vec<String> {
    vec![]
}
