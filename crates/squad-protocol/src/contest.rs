//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ContestKind {
    #[serde(rename = "forge")]
    Forge,
    #[serde(rename = "forecast")]
    Forecast,
    #[serde(rename = "decide")]
    Decide,
    #[serde(rename = "negotiate")]
    Negotiate,
    #[serde(rename = "blow_it_up")]
    BlowItUp,
    #[serde(rename = "readback")]
    Readback,
    #[serde(rename = "debug")]
    Debugging,
    #[serde(rename = "trace")]
    Trace,
    #[serde(rename = "rank")]
    Rank,

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
    #[serde(rename = "forge")]
    Forge(ForgeState),
    #[serde(rename = "forecast")]
    Forecast(ForecastState),
    #[serde(rename = "decide")]
    Decide(DecideState),
    #[serde(rename = "negotiate")]
    Negotiate(NegotiateState),
    #[serde(rename = "blow_it_up")]
    BlowItUp(BlowItUpState),
    #[serde(rename = "readback")]
    Readback(ReadbackState),
    #[serde(rename = "debug")]
    Debugging(DebuggingState),
    #[serde(rename = "trace")]
    Trace(TraceState),
    #[serde(rename = "rank")]
    Rank(RankState),

    Generic(GenericContestState),
    Selection(ContestSelectionState),
    Debate(DebateState),
    Tournament(TournamentState),
    Census(CensusState),
    #[cfg(feature = "mode-survey")]
    Survey(SurveyState),
    Probe(ProbeState),
    Grill(GrillState),
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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ContestKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<ArtifactAudience>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,

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
    #[serde(rename = "forge")]
    Forge(ForgeTransition),
    #[serde(rename = "forecast")]
    Forecast(ForecastTransition),
    #[serde(rename = "decide")]
    Decide(DecideTransition),
    #[serde(rename = "negotiate")]
    Negotiate(NegotiateTransition),
    #[serde(rename = "blow_it_up")]
    BlowItUp(BlowItUpTransition),
    #[serde(rename = "readback")]
    Readback(ReadbackTransition),
    #[serde(rename = "debug")]
    Debugging(DebuggingTransition),
    #[serde(rename = "trace")]
    Trace(TraceTransition),
    #[serde(rename = "rank")]
    Rank(RankTransition),

    Grill(GrillTransition),
    Debate(DebateTransition),
    Tournament(TournamentTransition),
    Census(CensusTransition),
    Crosscheck(CrosscheckTransition),
    #[cfg(feature = "mode-survey")]
    Survey(SurveyTransition),
    Probe(ProbeTransition),
}
impl ContestKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Forge => "forge",
            Self::Forecast => "forecast",
            Self::Decide => "decide",
            Self::Negotiate => "negotiate",
            Self::BlowItUp => "blow_it_up",
            Self::Readback => "readback",
            Self::Debugging => "debug",
            Self::Trace => "trace",
            Self::Rank => "rank",
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
        match self {
            Self::Forge => return forge::ENABLED,
            Self::Forecast => return forecast::ENABLED,
            Self::Decide => return decide::ENABLED,
            Self::Negotiate => return negotiate::ENABLED,
            Self::BlowItUp => return blow_it_up::ENABLED,
            Self::Readback => return readback::ENABLED,
            Self::Debugging => return debugging::ENABLED,
            Self::Trace => return trace::ENABLED,
            Self::Rank => return rank::ENABLED,
            _ => {}
        }
        matches!(
            self,
            Self::Debate
                | Self::Grill
                | Self::Tournament
                | Self::Census
                | Self::Crosscheck
                | Self::Probe
        ) || (cfg!(feature = "mode-survey") && self == Self::Survey)
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
        let ContestTransition::Debate(transition) = self.transition else {
            return Err(crate::validation::ValidationError {
                message: "expected debate transition".into(),
            });
        };
        Ok((self.contest_id, self.expected_state_version, transition))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag="kind", rename_all="lowercase", deny_unknown_fields)]
pub enum ArtifactAudience { Human, Case, Members { recipients: Vec<MemberNick> } }
impl ContestArtifact {
 pub fn validate_mode(&self) -> crate::validation::ValidationResult {
  use crate::validation::{ensure,unique};
  let kind=self.kind.ok_or_else(||crate::validation::ValidationError{message:"artifact requires kind".into()})?;
  ensure(self.round.is_some() && self.actor.is_some() && self.body.as_ref().is_some_and(|b|b.is_object()), "mode artifact requires round, actor and object body")?;
  match &self.audience { Some(ArtifactAudience::Members {recipients}) => ensure(!recipients.is_empty() && unique(recipients), "explicit distinct artifact recipients required")?, Some(_) => {}, None => return ensure(false,"artifact audience required") }
  let body=self.body.as_ref().expect("checked");
  match kind {
ContestKind::Forge => forge::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Forecast => forecast::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Decide => decide::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Negotiate => negotiate::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::BlowItUp => blow_it_up::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Readback => readback::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Debugging => debugging::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Trace => trace::validate_artifact(self.artifact_kind.as_str(), body),
ContestKind::Rank => rank::validate_artifact(self.artifact_kind.as_str(), body),
_ => ensure(false,"not a new-mode artifact") }
 }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct ContestHumanRequested { pub pending: parser::PendingHuman }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct ContestHumanAnswered { pub contest_id: ContestId, pub step_id: StepId, pub expected_state_version: u64, pub text: Option<String>, pub complete: bool }
