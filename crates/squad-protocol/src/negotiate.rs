//! negotiate contracts; enabled only after the mode implementation lands.
use crate::*;
use crate::validation::{ValidationResult, ensure, unique};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NegotiateStatus {
    Preflight,
    WaitingForParticipants,
    Mapping,
    Briefing,
    Packaging,
    Rehearsing,
    WritingPlaybook,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl NegotiateStatus { pub fn is_terminal(self) -> bool { matches!(self, Self::Complete | Self::Stopped | Self::Failed) } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateState {
 pub contest_id: ContestId,
 pub state_version: u64,
 pub state: NegotiateStatus,
 #[schemars(extend("x-required-nullable" = true))]
 pub step_id: Option<StepId>,
 pub round: u32,
 pub artifact_event_ids: Vec<EventId>,
 #[schemars(extend("x-required-nullable" = true))]
 pub terminal_reason: Option<Text>,
}
impl NegotiateState { pub fn validate(&self) -> ValidationResult {
 ensure(self.state_version > 0 && unique(&self.artifact_event_ids), "invalid negotiate state")?;
 ensure(self.state.is_terminal() == self.terminal_reason.is_some(), "terminal reason mismatch")?;
 ensure(self.round <= 6, "invalid negotiate round")
} }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum NegotiateTransition {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum NegotiateCaseMaterial {}
pub fn validate_artifact(_kind: &str, _body: &serde_json::Value) -> ValidationResult {
 ensure(false, "negotiate is disabled")
}
