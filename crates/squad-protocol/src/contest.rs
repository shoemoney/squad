//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ContestKind {
    Debate,
    Tournament,
    Grill,
    Census,
    Crosscheck,
    Survey,
    Probe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", rename_all_fields = "camelCase")]
pub enum ContestSelectionStatus {
    CollectingAnswers,
    AwaitingPick,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestAnswer {
    pub nick: MemberNick,
    pub message_id: MessageId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestCollecting {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub question_message_id: MessageId,
    pub question: String,
    pub addressed: Vec<MemberNick>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestPickRequested {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub question_message_id: MessageId,
    pub eligible: Vec<MemberNick>,
    pub required_picks: u8,
    pub banner: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestPicked {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub nicks: Vec<MemberNick>,
    pub source_message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestSelectionState {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub state: ContestSelectionStatus,
    pub state_version: u64,
    pub question_message_id: MessageId,
    pub addressed: Vec<MemberNick>,
    pub answers: Vec<ContestAnswer>,
    pub required_picks: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestSetup {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub script_version: Text,
    pub participants: Vec<MemberNick>,
    pub source_event_ids: Vec<EventId>,
    pub settings: serde_json::Value, // validated JSON object; mode schema arrives in M5
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenericContestState {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub state_version: u64,
    pub status: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<StepId>,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>, // nullable
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "state",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ContestState {
    Generic(GenericContestState),
    Selection(ContestSelectionState),
    Debate(DebateState),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestPreflight {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub plan_id: PlanId,
    pub expires_at: DateTime,
    pub estimate: DebateEstimate,
    pub participants: Vec<MemberNick>,
    pub confirmation_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<Text>,
    pub settings: serde_json::Value, // object; typed mode settings added by its M5 phase
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestPreflightResult {
    pub contest_id: ContestId,
    pub plan_id: PlanId,
    pub status: PreflightStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "preflight",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum PendingPreflight {
    Debate(DebatePreflight),
    Contest(ContestPreflight),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestStep {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub ordinal: u32,
    pub script_version: Text,
    pub move_name: ContestMove,
    pub inputs: Vec<EventId>,
    pub assignments: Vec<ContestAssignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ContestMove {
    Collect,
    Pick,
    Fanout,
    Turns,
    Judge,
    Tally,
    Barrier,
    Report,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestAssignment {
    pub target: Target,
    pub context_id: ContextId,
    pub payload_refs: Vec<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<WorkId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestArtifact {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub artifact_kind: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<WorkId>,
    pub source_refs: Vec<EventId>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestReport {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub artifact_event_ids: Vec<EventId>,
    pub skipped_work_ids: Vec<WorkId>,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContestTransitionSubmission {
    pub contest_id: ContestId,
    pub kind: ContestKind,
    pub expected_state_version: u64,
    pub transition: ContestTransition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "transition",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ContestTransition {
    Debate(DebateTransition),
    Crosscheck(CrosscheckTransition),
}
impl ContestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debate => "debate",
            Self::Tournament => "tournament",
            Self::Grill => "grill",
            Self::Census => "census",
            Self::Crosscheck => "crosscheck",
            Self::Survey => "survey",
            Self::Probe => "probe",
        }
    }
    pub fn is_built(self) -> bool {
        self == Self::Debate
    }
}
impl ContestTransitionSubmission {
    /// Normalize the shared carrier into the debate engine's versioned operation.
    pub fn into_debate(
        self,
    ) -> Result<(DebateId, u64, DebateTransition), crate::validation::ValidationError> {
        crate::validation::ensure(
            self.kind == ContestKind::Debate,
            "transition kind does not match debate",
        )?;
        let ContestTransition::Debate(transition) = self.transition else { return Err(crate::validation::ValidationError {message: "expected debate transition".into()}); };
        Ok((self.contest_id, self.expected_state_version, transition))
    }
}
