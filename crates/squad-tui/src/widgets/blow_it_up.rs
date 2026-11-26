//! blow_it_up view projection belongs here.
use squad_protocol::*;
pub fn lines(_state: &BlowItUpState) -> Vec<String> {
    vec![]
}

#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<BlowItUpState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::BlowItUpState(s)
            | EnvelopePayload::ContestState(ContestState::BlowItUp(s)) => {
                self.state = Some(s.clone())
            }
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::BlowItUp) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(_view: &View) -> Vec<String> {
    vec![]
}
