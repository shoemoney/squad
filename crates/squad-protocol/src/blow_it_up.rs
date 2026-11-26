//! blow_it_up contracts; enabled only after the mode implementation lands.
use crate::*;
use crate::validation::{ValidationResult, ensure, unique};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlowItUpStatus {
    Preflight,
    WaitingForParticipants,
    Attacking,
    Challenging,
    Ruling,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl BlowItUpStatus { pub fn is_terminal(self) -> bool { matches!(self, Self::Complete | Self::Stopped | Self::Failed) } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlowItUpState {
 pub contest_id: ContestId,
 pub state_version: u64,
 pub state: BlowItUpStatus,
 #[schemars(extend("x-required-nullable" = true))]
 pub step_id: Option<StepId>,
 pub round: u32,
 pub artifact_event_ids: Vec<EventId>,
 #[schemars(extend("x-required-nullable" = true))]
 pub terminal_reason: Option<Text>,
}
impl BlowItUpState { pub fn validate(&self) -> ValidationResult {
 ensure(self.state_version > 0 && unique(&self.artifact_event_ids), "invalid blow_it_up state")?;
 ensure(self.state.is_terminal() == self.terminal_reason.is_some(), "terminal reason mismatch")?;
 ensure(self.round == 0, "invalid blow_it_up round")
} }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum BlowItUpTransition {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum BlowItUpCaseMaterial {}
pub fn validate_artifact(_kind: &str, _body: &serde_json::Value) -> ValidationResult {
 ensure(false, "blow_it_up is disabled")
}
