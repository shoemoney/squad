//! decide contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecideStatus {
    Preflight,
    WaitingForParticipants,
    ProposingOptions,
    Shortlisting,
    ProposingCriteria,
    Freezing,
    Scoring,
    Computing,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl DecideStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: DecideStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl DecideState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid decide state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid decide round")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum DecideTransition {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum DecideCaseMaterial {}
pub fn validate_artifact(_kind: &str, _body: &serde_json::Value) -> ValidationResult {
    ensure(false, "decide is disabled")
}

/// Mode-owned generated schema used by shared invocation provenance.
/// Filled alongside the mode's output types; disabled modes have no output schema.
pub fn output_schema(_kind: &WorkKind) -> Option<serde_json::Value> {
    None
}
