//! forge view projection belongs here.
use squad_protocol::*;
pub fn lines(_state: &ForgeState) -> Vec<String> {
    vec![]
}

#[derive(Debug, Clone, Default)]
pub struct View {
    pub state: Option<ForgeState>,
    pub artifacts: std::collections::BTreeMap<EventId, ContestArtifact>,
}
impl View {
    pub fn apply(&mut self, event: EventId, payload: &EnvelopePayload) {
        match payload {
            EnvelopePayload::ForgeState(s)
            | EnvelopePayload::ContestState(ContestState::Forge(s)) => self.state = Some(s.clone()),
            EnvelopePayload::ContestArtifact(a) if a.kind == Some(ContestKind::Forge) => {
                self.artifacts.insert(event, a.clone());
            }
            _ => {}
        }
    }
}
pub fn render(_view: &View) -> Vec<String> {
    vec![]
}
