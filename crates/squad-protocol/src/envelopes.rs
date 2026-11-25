//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum MainRoom {
    Main,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[schemars(extend("x-required-nullable" = true))]
    pub cost_usd: Option<f64>, // nullable
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvelopeBase {
    pub version: u8,
    pub id: EventId,
    pub room_id: MainRoom,
    pub correlation_id: EventId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<EventId>,
    pub sender: Actor,
    pub recipients: Vec<Recipient>,
    pub created_at: DateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommitFields {
    pub seq: Cursor,
    pub accepted_at: DateTime,
    pub commit_index: u32,
    pub commit_size: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProducerEnvelope {
    #[serde(flatten)]
    pub base: EnvelopeBase,
    #[serde(flatten)]
    pub content: EnvelopePayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommittedEnvelope {
    #[serde(flatten)]
    pub base: EnvelopeBase,
    #[serde(flatten)]
    pub content: EnvelopePayload,
    #[serde(flatten)]
    pub commit: CommitFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedQuote {
    pub event_id: EventId,
    pub author: Actor,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserMessageInput {
    pub message_id: MessageId,
    pub text: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<Vec<EventId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Vec<EventId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    #[serde(flatten)]
    pub input: UserMessageInput,
    pub body: String,
    pub resolved_quotes: Vec<ResolvedQuote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerationIdentity {
    pub message_id: MessageId,
    pub work_id: WorkId,
    pub trigger_event_id: EventId,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompletedGeneration {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub model: Text,
    pub finish_reason: FinishReason,
    #[schemars(extend("x-required-nullable" = true))]
    pub usage: Option<Usage>, // nullable
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    #[serde(flatten)]
    pub completed: CompletedGeneration,
    pub in_reply_to_message_id: MessageId,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "payload")]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EnvelopePayload {
    #[serde(rename = "probe.setup")]
    ProbeSetup(ProbeSetup),
    #[serde(rename = "probe.state")]
    ProbeState(ProbeState),
    #[serde(rename = "probe.preflight")]
    ProbePreflight(ProbePreflight),
    #[serde(rename = "probe.explanation")]
    ProbeExplanation(ProbeOutput<ProbeExplanation>),
    #[serde(rename = "probe.check")]
    ProbeCheck(ProbeOutput<ProbeCheck>),
    #[serde(rename = "probe.prediction")]
    ProbePrediction(ProbeOutput<ProbePrediction>),
    #[serde(rename = "probe.report")]
    ProbeReport(ProbeOutput<ProbeReport>),
    #[serde(rename = "user.message")]
    UserMessage(UserMessage),
    #[serde(rename = "crosscheck.setup")]
    CrosscheckSetup(CrosscheckSetup),
    #[serde(rename = "crosscheck.state")]
    CrosscheckState(CrosscheckState),
    #[serde(rename = "crosscheck.preflight")]
    CrosscheckPreflight(CrosscheckPreflight),
    #[serde(rename = "crosscheck.goal")]
    CrosscheckGoal(CrosscheckGoalRecord),
    #[serde(rename = "crosscheck.ruling")]
    CrosscheckRuling(CrosscheckRulingRecord),
    #[serde(rename = "crosscheck.inventory")]
    CrosscheckInventory(CrosscheckInventory),
    #[serde(rename = "crosscheck.grid")]
    CrosscheckGrid(CrosscheckGrid),
    #[serde(rename = "crosscheck.review")]
    CrosscheckReview(CrosscheckReview),
    #[serde(rename = "crosscheck.audit")]
    CrosscheckAudit(CrosscheckAudit),
    #[serde(rename = "crosscheck.acceptance")]
    CrosscheckAcceptance(CrosscheckAcceptance),
    #[serde(rename = "crosscheck.report")]
    CrosscheckReport(CrosscheckReport),

    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.setup")]
    SurveySetup(SurveySetup),
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.state")]
    SurveyState(SurveyState),
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.claims")]
    SurveyClaims(SurveyClaims),
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.vote")]
    SurveyVote(SurveyVote),
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.selection")]
    SurveySelection(SurveySelection),
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.revote")]
    SurveyRevote(SurveyRevote),
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.report")]
    SurveyReport(SurveyReport),
    #[serde(rename = "agent.reply")]
    AgentReply(Completion),
    #[serde(rename = "message.final")]
    MessageFinal(Completion),
    #[serde(rename = "delta")]
    Delta(Delta),
    #[serde(rename = "presence")]
    Presence(Presence),
    #[serde(rename = "system.notice")]
    SystemNotice(SystemNotice),
    #[serde(rename = "error")]
    Error(ErrorPayload),
    #[serde(rename = "var.change")]
    VarChange(VariableChange),
    #[serde(rename = "work.state")]
    WorkState(WorkStateEvent),
    #[serde(rename = "usage.record")]
    UsageRecord(UsageRecord),
    #[serde(rename = "budget.state")]
    BudgetState(BudgetStateEvent),
    #[serde(rename = "grill.setup")]
    GrillSetup(GrillSetup),
    #[serde(rename = "grill.state")]
    GrillState(GrillState),
    #[serde(rename = "grill.question")]
    GrillQuestion(GrillQuestion),
    #[serde(rename = "grill.answer")]
    GrillAnswer(GrillAnswer),
    #[serde(rename = "grill.pass")]
    GrillPass(GrillPass),
    #[serde(rename = "grill.plan")]
    GrillPlan(GrillPlan),
    #[serde(rename = "grill.complete")]
    GrillComplete(GrillComplete),
    #[serde(rename = "debate.preflight")]
    DebatePreflight(DebatePreflight),
    #[serde(rename = "debate.preflight.result")]
    DebatePreflightResult(DebatePreflightResult),
    #[serde(rename = "debate.opening")]
    DebateOpening(DebateOpening),
    #[serde(rename = "debate.setup")]
    DebateSetup(DebateSetup),
    #[serde(rename = "contest.transition")]
    ContestTransition(ContestTransitionSubmission),
    #[serde(rename = "debate.state")]
    DebateState(DebateState),
    #[serde(rename = "debate.turn")]
    DebateTurn(DebateTurn),
    #[serde(rename = "verdict")]
    Verdict(DebateVerdict),
    #[serde(rename = "case.opened")]
    CaseOpened(CaseOpened),
    #[serde(rename = "case.followup")]
    CaseFollowup(CaseFollowup),
    #[serde(rename = "case.ruling")]
    CaseRuling(CaseRuling),
    #[serde(rename = "message.superseded")]
    MessageSuperseded(MessageSuperseded),
    #[serde(rename = "scores.changed")]
    ScoresChanged(ScoresChanged),
    #[serde(rename = "room.cleared")]
    RoomCleared(RoomCleared),
    #[serde(rename = "debate.round_score")]
    DebateRoundScore(DebateRoundScore),
    #[serde(rename = "tournament.setup")]
    TournamentSetup(TournamentSetup),
    #[serde(rename = "tournament.state")]
    TournamentState(TournamentState),
    #[serde(rename = "tournament.preflight")]
    TournamentPreflight(TournamentPreflight),
    #[serde(rename = "tournament.entry")]
    TournamentEntry(TournamentEntry),
    #[serde(rename = "tournament.ranking")]
    TournamentRanking(TournamentRanking),
    #[serde(rename = "tournament.result")]
    TournamentResult(TournamentResult),
    #[serde(rename = "tournament.champion")]
    TournamentChampion(TournamentChampion),
    #[serde(rename = "tournament.judgment")]
    TournamentJudgment(TournamentJudgment),
    #[serde(rename = "census.setup")]
    CensusSetup(CensusSetup),
    #[serde(rename = "census.state")]
    CensusState(CensusState),
    #[serde(rename = "census.plan")]
    CensusPlan(CensusPlan),
    #[serde(rename = "census.hunt")]
    CensusHunt(CensusHunt),
    #[serde(rename = "census.ledger")]
    CensusLedger(CensusLedger),
    #[serde(rename = "census.rating")]
    CensusRating(CensusRating),
    #[serde(rename = "census.report")]
    CensusReport(CensusReport),
    #[serde(rename = "census.case_material")]
    CensusCaseMaterial(CensusCaseMaterial),
    #[serde(rename = "contest.setup")]
    ContestSetup(ContestSetup),
    #[serde(rename = "contest.state")]
    ContestState(ContestState),
    #[serde(rename = "contest.preflight")]
    ContestPreflight(ContestPreflight),
    #[serde(rename = "contest.preflight.result")]
    ContestPreflightResult(ContestPreflightResult),
    #[serde(rename = "contest.step")]
    ContestStep(ContestStep),
    #[serde(rename = "contest.artifact")]
    ContestArtifact(ContestArtifact),
    #[serde(rename = "contest.report")]
    ContestReport(ContestReport),
    #[serde(rename = "contest.collecting")]
    ContestCollecting(ContestCollecting),
    #[serde(rename = "contest.pick_requested")]
    ContestPickRequested(ContestPickRequested),
    #[serde(rename = "contest.picked")]
    ContestPicked(ContestPicked),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub chunk_index: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<StepId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum PresenceStatus {
    Idle,
    Thinking,
    Streaming,
    Error,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub nick: MemberNick,
    pub status: PresenceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<WorkId>,
    pub queue_depth: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<ErrorCode>,
    pub observed_at: DateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RetryKind {
    #[serde(rename = "retry")]
    Retry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RetryAction {
    pub kind: RetryKind,
    pub work_id: WorkId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub text: Text,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_event_id: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<WorkId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub earliest_cursor: Option<Cursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_action: Option<RetryAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemNotice {
    pub code: Text,
    pub text: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<WorkId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariableChange {
    pub name: VariableName,
    pub value: u8,
    pub previous_value: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum WorkStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkStateEvent {
    pub work_id: WorkId,
    pub trigger_event_id: EventId,
    pub target: Target,
    pub context_id: ContextId,
    pub attempt: u32,
    pub status: WorkStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum UsageOutcome {
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    pub work_id: WorkId,
    pub attempt: u32,
    pub target: Target,
    pub context_id: ContextId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
    pub model: Text,
    pub started_at: DateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime>,
    pub outcome: UsageOutcome,
    #[schemars(extend("x-required-nullable" = true))]
    pub usage: Option<Usage>, // nullable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub response_model: Option<Text>, // nullable
    pub day: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStateEvent {
    pub spent_usd: f64,
    pub reserved_usd: f64,
    pub limit_usd: f64,
    pub estimated_usd: f64,
    pub unresolved_attempts: u32,
    pub day: String,
    pub unknown_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
// Keep the shared by-value API specified by section 02.
#[allow(clippy::large_enum_variant)]
pub enum CaseMaterial {
    Tournament {
        material: TournamentCaseMaterial,
    },
    #[cfg(feature = "mode-survey")]
    Survey {
        material: SurveyCaseMaterial,
    },
    Probe(ProbeCaseMaterial),
    Standalone {
        messages: Vec<ResolvedQuote>,
        question: Text,
    },
    Debate {
        setup: DebateSetup,
        rubric_version: Text,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaseOpened {
    pub case_id: CaseId,
    pub judge_invocation_id: EventId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<WorkId>,
    pub judge_model: Text,
    pub material: CaseMaterial,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaseFollowup {
    pub case_id: CaseId,
    pub user_event_id: EventId,
    pub judge_invocation_id: EventId,
    pub work_id: WorkId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaseRuling {
    #[serde(flatten)]
    pub completed: CompletedGeneration,
    pub case_id: CaseId,
    pub judge_invocation_id: EventId,
    pub in_reply_to_event_id: EventId,
    pub content: Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageSuperseded {
    pub superseded_event_id: EventId,
    pub superseded_message_id: MessageId,
    pub member: MemberNick,
    pub in_reply_to_message_id: MessageId,
    pub replacement_work_id: WorkId,
    pub replacement_message_id: MessageId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub nick: MemberNick,
    pub wins: u64,
    pub losses: u64,
    pub trophies: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoresChanged {
    pub verdict_event_id: EventId,
    pub scores: Vec<Score>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoomCleared {
    pub epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_through: Option<Cursor>,
}

impl ProducerEnvelope {
    /// Structural and self-consistency check; debate committers must also call `validate_against` with stored state.
    pub fn from_value(
        value: &serde_json::Value,
    ) -> Result<Self, crate::validation::ValidationError> {
        let envelope: Self = crate::validation::decode(value)?;
        envelope.validate()?;
        Ok(envelope)
    }
    pub fn validate(&self) -> crate::validation::ValidationResult {
        validate_envelope(&self.base, &self.content)
    }
}
impl CommittedEnvelope {
    /// Structural and self-consistency check; debate committers must also call `validate_against` with stored state.
    pub fn from_value(
        value: &serde_json::Value,
    ) -> Result<Self, crate::validation::ValidationError> {
        let envelope: Self = crate::validation::decode(value)?;
        envelope.validate()?;
        Ok(envelope)
    }
    pub fn validate(&self) -> crate::validation::ValidationResult {
        use crate::validation::ensure;
        validate_envelope(&self.base, &self.content)?;
        ensure(
            self.content.is_durable(),
            "only canonical durable kinds may be committed",
        )?;
        ensure(
            self.commit.commit_size > 0 && self.commit.commit_index < self.commit.commit_size,
            "invalid commit group",
        )
    }
}
impl EnvelopePayload {
    pub fn is_durable(&self) -> bool {
        !matches!(
            self,
            Self::Delta(_)
                | Self::Presence(_)
                | Self::AgentReply(_)
                | Self::ContestTransition(_)
                | Self::TournamentJudgment(_)
        )
    }
}
fn validate_envelope(
    base: &EnvelopeBase,
    payload: &EnvelopePayload,
) -> crate::validation::ValidationResult {
    use crate::validation::{ensure, money, unique};
    ensure(base.version == 1, "unsupported protocol version")?;
    ensure(
        !base.recipients.is_empty() && unique(&base.recipients),
        "recipients must be nonempty and unique",
    )?;
    let human_only = base.recipients == [Recipient::Me];
    match payload {
        EnvelopePayload::TournamentState(s) => {
            ensure(
                base.sender == Actor::System && human_only,
                "tournament state requires system to human",
            )?;
            s.validate()?;
        }
        EnvelopePayload::TournamentEntry(e) => {
            ensure(
                human_only
                    && (base.sender == Actor::Member(e.entrant.clone())
                        || (base.sender == Actor::System
                            && matches!(e.outcome, DebateTurnOutcome::Forfeit { .. }))),
                "invalid tournament entry author",
            )?;
            ensure(e.round > 0 && e.replay <= 1, "invalid entry execution")?;
        }
        EnvelopePayload::TournamentJudgment(j) => {
            ensure(
                base.sender == Actor::Judge && human_only && j.round > 0 && j.replay <= 1,
                "invalid tournament judgment",
            )?;
        }
        EnvelopePayload::TournamentSetup(s) => {
            ensure(
                base.sender == Actor::System
                    && human_only
                    && s.entrants.len() >= 3
                    && unique(&s.entrants)
                    && s.total_rounds == s.entrants.len() as u32 - 1
                    && !s.prompt.trim().is_empty(),
                "invalid tournament setup",
            )?;
        }
        EnvelopePayload::TournamentPreflight(p) => {
            ensure(
                base.sender == Actor::System
                    && human_only
                    && p.base.kind == ContestKind::Tournament
                    && p.base.participants.len() >= 3
                    && unique(&p.base.participants)
                    && p.total_rounds == p.base.participants.len() as u32 - 1
                    && p.replays == 1
                    && p.entry_attempts == 2
                    && p.judge_attempts == 3,
                "invalid tournament preflight",
            )?;
            p.base.estimate.validate()?;
        }
        EnvelopePayload::TournamentResult(r) => {
            ensure(
                base.sender == Actor::System
                    && base.recipients.len() == 2
                    && base.recipients.contains(&Recipient::Me)
                    && base
                        .recipients
                        .contains(&Recipient::Member(r.entrant.clone())),
                "tournament result requires human and owning entrant",
            )?;
        }
        EnvelopePayload::TournamentRanking(_) | EnvelopePayload::TournamentChampion(_) => {
            ensure(
                base.sender == Actor::System && human_only,
                "tournament announcement requires system to human",
            )?;
        }
        EnvelopePayload::CrosscheckAcceptance(v) => {
            ensure(
                base.sender == Actor::Me && human_only,
                "crosscheck acceptance requires human",
            )?;
            v.validate()?;
        }
        EnvelopePayload::CrosscheckGrid(v) => {
            ensure(
                base.sender == Actor::System && human_only,
                "crosscheck grid is application-owned",
            )?;
            v.validate()?;
        }
        EnvelopePayload::CrosscheckState(_)
        | EnvelopePayload::CrosscheckSetup(_)
        | EnvelopePayload::CrosscheckPreflight(_)
        | EnvelopePayload::CrosscheckReport(_) => ensure(
            base.sender == Actor::System && human_only,
            "crosscheck control is application-owned",
        )?,
        _ => {}
    }

    match payload {
        EnvelopePayload::UserMessage(message) => {
            ensure(base.sender == Actor::Me, "user.message requires me")?;
            ensure(
                base.id.as_uuid() == message.input.message_id.as_uuid(),
                "user message identity must equal event identity",
            )?;
            if let Some(ids) = &message.input.quote {
                ensure(ids.len() <= 12 && unique(ids), "invalid quote references")?;
            }
            if let Some(ids) = &message.input.reply_to {
                ensure(
                    ids.len() <= 2 && unique(ids) && message.debate_id.is_some(),
                    "replyTo is debate-only",
                )?;
            }
            ensure(
                message.input.quote.as_deref().unwrap_or_default()
                    == message
                        .resolved_quotes
                        .iter()
                        .map(|q| q.event_id)
                        .collect::<Vec<_>>(),
                "resolved quotes must match submitted quote identities",
            )?;
            same_contest(message.debate_id, message.contest_id)?;
        }
        EnvelopePayload::AgentReply(completion) | EnvelopePayload::MessageFinal(completion) => {
            ensure(
                matches!(base.sender, Actor::Member(_)),
                "ordinary completion requires member actor",
            )?;
            completed(&completion.completed, &completion.content)?;
        }
        EnvelopePayload::Delta(delta) => {
            ensure(
                matches!(base.sender, Actor::Member(_) | Actor::Judge),
                "delta requires executor",
            )?;
            generation(&delta.generation)?;
            ensure(
                delta.debate_id.is_none() || delta.case_id.is_some() || delta.round.is_some(),
                "debate delta needs round",
            )?;
            ensure(
                delta
                    .round
                    .is_none_or(|r| r <= u32::from(constants::MAX_ROUNDS)),
                "invalid delta round",
            )?;
            ensure(
                delta.step_id.is_none() || delta.contest_id.is_some(),
                "step delta needs contest",
            )?;
            same_contest(delta.debate_id, delta.contest_id)?;
        }
        EnvelopePayload::Presence(presence) => {
            ensure(
                base.sender == Actor::System || base.sender == Actor::Member(presence.nick.clone()),
                "presence actor does not match nick",
            )?;
            ensure(
                matches!(
                    presence.status,
                    PresenceStatus::Thinking | PresenceStatus::Streaming
                ) == presence.work_id.is_some(),
                "active presence requires work identity; idle presence omits it",
            )?;
            ensure(
                presence.error_code.is_none() || presence.status == PresenceStatus::Error,
                "errorCode requires error presence",
            )?;
        }
        EnvelopePayload::UsageRecord(record) => {
            ensure(
                record.attempt > 0
                    && money_optional(record.estimated_cost_usd)
                    && valid_day(&record.day),
                "invalid usage record",
            )?;
            if let Some(usage) = &record.usage {
                validate_usage(usage)?;
            }
            same_contest(record.debate_id, record.contest_id)?;
        }
        EnvelopePayload::BudgetState(budget) => ensure(
            [
                budget.spent_usd,
                budget.reserved_usd,
                budget.limit_usd,
                budget.estimated_usd,
            ]
            .into_iter()
            .all(money)
                && valid_day(&budget.day),
            "invalid daily spending projection",
        )?,
        EnvelopePayload::VarChange(change) => ensure(
            variable_value(&change.name, change.value)
                && variable_value(&change.name, change.previous_value),
            "invalid variable value",
        )?,
        EnvelopePayload::WorkState(state) => {
            if let Some(error) = &state.error {
                validate_error(error)?;
            }
            ensure(
                !matches!(state.status, WorkStatus::Running | WorkStatus::Completed)
                    || state.attempt > 0,
                "running/completed work requires an attempt",
            )?;
            same_contest(state.debate_id, state.contest_id)?;
        }
        EnvelopePayload::Error(error) => validate_error(error)?,
        EnvelopePayload::GrillSetup(v) => v.validate()?,
        EnvelopePayload::GrillState(v) => v.validate()?,
        EnvelopePayload::GrillQuestion(v) => {
            v.validate()?;
            ensure(
                base.sender == Actor::Member(v.griller.clone()),
                "grill question author mismatch",
            )?;
        }
        EnvelopePayload::GrillPlan(v) => v.validate(u32::MAX)?,
        EnvelopePayload::GrillAnswer(v) => ensure(
            !v.answers.is_empty() && v.answers.len() <= 3,
            "invalid grill answer",
        )?,
        EnvelopePayload::GrillPass(v) => ensure((1..=10).contains(&v.round), "invalid grill pass")?,
        EnvelopePayload::GrillComplete(v) => {
            ensure(v.final_plan_version > 0, "invalid final grill plan")?
        }
        EnvelopePayload::DebatePreflight(preflight) => preflight.validate()?,
        EnvelopePayload::DebatePreflightResult(result) => {
            validate_preflight_result(&result.status, &result.confirmation, &result.error)?
        }
        EnvelopePayload::DebateOpening(opening) => ensure(
            opening.round == 0
                && (u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS))
                    .contains(&opening.rounds)
                && opening.debaters[0] != opening.debaters[1],
            "invalid opening",
        )?,
        EnvelopePayload::DebateSetup(setup) => setup.validate()?,
        EnvelopePayload::CrosscheckSetup(v) => v.validate()?,
        EnvelopePayload::CrosscheckState(v) => v.validate()?,
        EnvelopePayload::CrosscheckPreflight(v) => v.validate()?,
        EnvelopePayload::CrosscheckInventory(v) => v.response.validate()?,
        EnvelopePayload::CrosscheckRuling(_)
        | EnvelopePayload::CrosscheckGoal(_)
        | EnvelopePayload::CrosscheckGrid(_)
        | EnvelopePayload::CrosscheckReview(_)
        | EnvelopePayload::CrosscheckAudit(_)
        | EnvelopePayload::CrosscheckAcceptance(_)
        | EnvelopePayload::CrosscheckReport(_) => {}
        EnvelopePayload::DebateState(state) => state.validate()?,
        EnvelopePayload::DebateTurn(turn) => {
            generation(&turn.generation)?;
            ensure(
                turn.round <= u32::from(constants::MAX_ROUNDS)
                    && (base.sender == Actor::Member(turn.speaker.clone())
                        || (base.sender == Actor::System
                            && matches!(turn.outcome, DebateTurnOutcome::Forfeit { .. }))),
                "invalid debate speaker or round",
            )?;
            if let DebateTurnOutcome::Completed {
                content,
                finish_reason,
                usage,
                ..
            } = &turn.outcome
            {
                visible_content(content, finish_reason)?;
                if let Some(usage) = usage {
                    validate_usage(usage)?;
                }
            }
        }
        EnvelopePayload::Verdict(verdict) => {
            ensure(base.sender == Actor::Judge, "verdict requires judge actor")?;
            generation(&verdict.completed.generation)?;
            let scores = &verdict.verdict.verdict.scores;
            ensure(scores.len() == 2, "verdict requires two score entries")?;
            let nicks = [scores[0].nick.clone(), scores[1].nick.clone()];
            crate::verdict::validate_committed_verdict(
                &verdict.verdict,
                &nicks,
                &verdict.verdict.round_tally,
            )?;
            if let Some(usage) = &verdict.completed.usage {
                validate_usage(usage)?;
            }
        }
        EnvelopePayload::DebateRoundScore(score) => {
            ensure(
                (base.sender == Actor::Judge
                    || (base.sender == Actor::System && score.scores.is_none()))
                    && human_only,
                "round scores are judge-to-human only",
            )?;
            ensure(score.tally.len() == 2, "tally requires two debaters")?;
            let nicks = score.tally.keys().cloned().collect::<Vec<_>>();
            crate::verdict::validate_round_score(
                score,
                &[nicks[0].clone(), nicks[1].clone()],
                u32::from(constants::MAX_ROUNDS),
            )?;
        }
        EnvelopePayload::CaseOpened(case) => {
            same_contest(case.debate_id, case.contest_id)?;
            match &case.material {
                CaseMaterial::Tournament { material } => {
                    ensure(
                        case.contest_id.is_some() && case.debate_id.is_none() && material.round > 0,
                        "invalid tournament case",
                    )?;
                }
                #[cfg(feature = "mode-survey")]
                CaseMaterial::Survey { material } => {
                    let setup = match material {
                        SurveyCaseMaterial::Extraction { setup }
                        | SurveyCaseMaterial::Report { setup, .. } => setup,
                    };
                    setup.validate()?;
                    ensure(
                        case.contest_id == Some(setup.contest_id) && case.debate_id.is_none(),
                        "survey case identity mismatch",
                    )?;
                }
                CaseMaterial::Probe(_) => ensure(
                    case.contest_id.is_some() && case.debate_id.is_none(),
                    "invalid probe case",
                )?,
                CaseMaterial::Standalone { messages, .. } => ensure(
                    !messages.is_empty() && messages.len() <= 12 && case.debate_id.is_none(),
                    "invalid standalone case",
                )?,
                CaseMaterial::Debate { setup, .. } => {
                    setup.validate()?;
                    ensure(
                        case.debate_id == Some(setup.debate_id),
                        "case debate mismatch",
                    )?;
                }
            }
        }
        EnvelopePayload::CaseRuling(ruling) => {
            ensure(
                base.sender == Actor::Judge && human_only,
                "case ruling requires judge-to-human",
            )?;
            completed(&ruling.completed, ruling.content.as_str())?;
        }
        EnvelopePayload::CensusSetup(s) => {
            ensure(
                base.sender == Actor::System && human_only,
                "census setup requires system",
            )?;
            s.validate()?;
        }
        EnvelopePayload::CensusState(s) => {
            ensure(
                base.sender == Actor::System && human_only,
                "census state requires system",
            )?;
            s.validate()?;
        }
        EnvelopePayload::CensusPlan(_) | EnvelopePayload::CensusCaseMaterial(_) => ensure(
            base.sender == Actor::System && human_only,
            "census material requires system",
        )?,
        EnvelopePayload::CensusHunt(h) => {
            ensure(
                base.sender == Actor::Member(h.member.clone()) && human_only,
                "hunt requires assigned member",
            )?;
            h.output.validate()?;
        }
        EnvelopePayload::CensusLedger(l) => {
            ensure(
                (l.stage == CensusLedgerStage::Finalized) == l.final_output.is_some(),
                "ledger stage mismatch",
            )?;
            ensure(
                base.sender == Actor::Judge && human_only,
                "ledger requires judge",
            )?;
        }
        EnvelopePayload::CensusRating(r) => ensure(
            base.sender == Actor::Member(r.member.clone()) && human_only,
            "rating requires assigned member",
        )?,
        EnvelopePayload::CensusReport(r) => {
            ensure(
                base.sender == Actor::System && human_only,
                "report requires system",
            )?;
            r.validate()?;
        }
        #[cfg(feature = "mode-survey")]
        EnvelopePayload::SurveySetup(setup) => {
            ensure(
                base.sender == Actor::System && human_only,
                "survey setup is human only",
            )?;
            setup.validate()?;
        }
        #[cfg(feature = "mode-survey")]
        EnvelopePayload::SurveyState(state) => {
            ensure(
                base.sender == Actor::System && human_only,
                "invalid survey state sender",
            )?;
            state.validate()?;
        }
        #[cfg(feature = "mode-survey")]
        EnvelopePayload::SurveySelection(selection) => {
            ensure(
                base.sender == Actor::System && human_only,
                "survey selection is human only",
            )?;
            selection.validate()?;
        }
        #[cfg(feature = "mode-survey")]
        EnvelopePayload::SurveyClaims(value) => ensure(
            base.sender == Actor::Judge && human_only && value.extraction.claims.len() <= 25,
            "invalid survey extraction",
        )?,
        #[cfg(feature = "mode-survey")]
        EnvelopePayload::SurveyVote(value) | EnvelopePayload::SurveyRevote(value) => ensure(
            base.sender == Actor::Member(value.voter.clone()) && human_only,
            "invalid survey voter",
        )?,
        #[cfg(feature = "mode-survey")]
        EnvelopePayload::SurveyReport(value) => {
            ensure(
                base.sender == Actor::Judge && human_only,
                "invalid survey report sender",
            )?;
            value.report.validate(&value.map)?;
        }
        EnvelopePayload::ContestSetup(setup) => {
            ensure(
                setup.kind != ContestKind::Debate
                    && setup.settings.is_object()
                    && distinct_members(&setup.participants),
                "invalid generic setup",
            )?;
        }
        EnvelopePayload::ProbeSetup(v) => v.validate()?,
        EnvelopePayload::ProbeState(v) => v.validate()?,
        EnvelopePayload::ProbePreflight(v) => v.validate()?,
        EnvelopePayload::ProbeCheck(v) => {
            ensure(
                human_only && matches!(base.sender, Actor::Member(_)),
                "probe member output must be human-only",
            )?;
            generation(&v.completed.generation)?;
            if let Some(usage) = &v.completed.usage {
                validate_usage(usage)?;
            }
            v.data.validate()?;
        }
        EnvelopePayload::ProbeExplanation(v) => {
            ensure(
                human_only && matches!(base.sender, Actor::Member(_)),
                "probe member output must be human-only",
            )?;
            generation(&v.completed.generation)?;
            if let Some(usage) = &v.completed.usage {
                validate_usage(usage)?;
            }
        }
        EnvelopePayload::ProbePrediction(v) => {
            ensure(
                human_only && matches!(base.sender, Actor::Member(_)),
                "probe member output must be human-only",
            )?;
            generation(&v.completed.generation)?;
            if let Some(usage) = &v.completed.usage {
                validate_usage(usage)?;
            }
        }
        EnvelopePayload::ProbeReport(v) => {
            ensure(
                human_only && base.sender == Actor::Judge,
                "probe report must be judge-to-human",
            )?;
            generation(&v.completed.generation)?;
            if let Some(usage) = &v.completed.usage {
                validate_usage(usage)?;
            }
        }
        EnvelopePayload::ContestState(state) => match state {
            #[cfg(feature = "mode-survey")]
            ContestState::Survey(state) => state.validate()?,
            ContestState::Probe(state) => state.validate()?,
            ContestState::Debate(state) => state.validate()?,
            ContestState::Tournament(state) => state.validate()?,
            ContestState::Census(state) => state.validate()?,
            ContestState::Generic(state) => ensure(
                state.kind != ContestKind::Debate && unique(&state.artifact_event_ids),
                "duplicate artifact references",
            )?,
            ContestState::Selection(state) => ensure(
                distinct_members(&state.addressed)
                    && (1..=2).contains(&state.required_picks)
                    && state
                        .answers
                        .iter()
                        .all(|a| state.addressed.contains(&a.nick))
                    && unique(&state.answers.iter().map(|a| &a.nick).collect::<Vec<_>>()),
                "invalid selection state",
            )?,
        },
        EnvelopePayload::ContestTransition(submission) => {
            let kind = match &submission.transition {
                ContestTransition::Debate(_) => ContestKind::Debate,
                ContestTransition::Probe(_) => ContestKind::Probe,
                ContestTransition::Grill(_) => ContestKind::Grill,
                ContestTransition::Tournament(_) => ContestKind::Tournament,
                ContestTransition::Census(_) => ContestKind::Census,
                ContestTransition::Crosscheck(_) => ContestKind::Crosscheck,
                #[cfg(feature = "mode-survey")]
                ContestTransition::Survey(_) => ContestKind::Survey,
            };
            ensure(submission.kind == kind, "transition kind mismatch")?;
        }
        EnvelopePayload::ContestPreflight(preflight) => {
            preflight.estimate.validate()?;
            ensure(
                preflight.kind != ContestKind::Debate
                    && preflight.settings.is_object()
                    && distinct_members(&preflight.participants),
                "invalid generic preflight",
            )?;
        }
        EnvelopePayload::ContestPreflightResult(result) => {
            validate_preflight_result(&result.status, &result.confirmation, &result.error)?
        }
        EnvelopePayload::ContestCollecting(collection) => ensure(
            distinct_members(&collection.addressed) && !collection.question.trim().is_empty(),
            "invalid collection",
        )?,
        EnvelopePayload::ContestPickRequested(pick) => ensure(
            human_only
                && distinct_members(&pick.eligible)
                && (1..=2).contains(&pick.required_picks)
                && usize::from(pick.required_picks) <= pick.eligible.len(),
            "invalid pending pick",
        )?,
        EnvelopePayload::ContestPicked(pick) => ensure(
            pick.kind.is_built()
                && pick.kind != ContestKind::Survey
                && distinct_members(&pick.nicks)
                && unique(&pick.source_message_ids)
                && pick.nicks.len() == pick.source_message_ids.len()
                && match pick.kind {
                    ContestKind::Debate => pick.nicks.len() == 2,
                    ContestKind::Grill => pick.nicks.len() == 1,
                    _ => false,
                },
            "invalid picked selection",
        )?,
        EnvelopePayload::ContestStep(step) => {
            ensure(
                step.ordinal > 0 && unique(&step.inputs),
                "invalid step ordinal or references",
            )?;
            for assignment in &step.assignments {
                match &assignment.target {
                    Target::Member(nick) => ensure(
                        assignment.context_id.0 == nick.as_str()
                            || assignment.context_id.0
                                == format!("contest:{}:{}:{nick}", step.contest_id, step.step_id),
                        "invalid member assignment context",
                    )?,
                    Target::Judge => {
                        let case = assignment
                            .context_id
                            .0
                            .strip_prefix("judge:")
                            .and_then(|s| s.parse::<CaseId>().ok());
                        ensure(case.is_some(), "invalid judge assignment context")?;
                    }
                }
            }
            ensure(
                unique(
                    &step
                        .assignments
                        .iter()
                        .map(|a| &a.target)
                        .collect::<Vec<_>>(),
                ),
                "duplicate assignment target",
            )?;
        }
        EnvelopePayload::ContestArtifact(artifact) => ensure(
            artifact.data.is_object() && unique(&artifact.source_refs),
            "invalid artifact",
        )?,
        EnvelopePayload::ContestReport(report) => ensure(
            report.data.is_object()
                && unique(&report.artifact_event_ids)
                && unique(&report.skipped_work_ids),
            "invalid report",
        )?,
        EnvelopePayload::SystemNotice(notice) => same_contest(notice.debate_id, notice.contest_id)?,
        EnvelopePayload::TournamentSetup(_)
        | EnvelopePayload::TournamentState(_)
        | EnvelopePayload::TournamentPreflight(_)
        | EnvelopePayload::TournamentEntry(_)
        | EnvelopePayload::TournamentRanking(_)
        | EnvelopePayload::TournamentResult(_)
        | EnvelopePayload::TournamentChampion(_)
        | EnvelopePayload::TournamentJudgment(_)
        | EnvelopePayload::CaseFollowup(_)
        | EnvelopePayload::MessageSuperseded(_)
        | EnvelopePayload::ScoresChanged(_)
        | EnvelopePayload::RoomCleared(_) => {}
    }
    Ok(())
}
pub fn validate_usage(usage: &Usage) -> crate::validation::ValidationResult {
    crate::validation::ensure(money_optional(usage.cost_usd), "invalid cost")
}
fn money_optional(value: Option<f64>) -> bool {
    value.is_none_or(crate::validation::money)
}
pub fn valid_day(day: &str) -> bool {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == day)
}
fn distinct_members(nicks: &[MemberNick]) -> bool {
    !nicks.is_empty() && nicks.len() <= 12 && crate::validation::unique(nicks)
}
pub fn variable_value(name: &VariableName, value: u8) -> bool {
    match name {
        VariableName::Rounds => (constants::MIN_ROUNDS..=constants::MAX_ROUNDS).contains(&value),
        VariableName::GrillRounds => (1..=10).contains(&value),
        VariableName::CensusPasses => (1..=5).contains(&value),
    }
}
fn same_contest(
    debate: Option<DebateId>,
    contest: Option<ContestId>,
) -> crate::validation::ValidationResult {
    crate::validation::ensure(
        debate.zip(contest).is_none_or(|(d, c)| d == c),
        "contest and debate identities differ",
    )
}
fn generation(identity: &GenerationIdentity) -> crate::validation::ValidationResult {
    crate::validation::ensure(identity.attempt > 0, "generation attempt must be positive")
}
fn visible_content(content: &str, finish: &FinishReason) -> crate::validation::ValidationResult {
    crate::validation::ensure(
        !content.is_empty() || *finish == FinishReason::ContentFilter,
        "empty answer requires content_filter",
    )
}
fn completed(
    completed: &CompletedGeneration,
    content: &str,
) -> crate::validation::ValidationResult {
    generation(&completed.generation)?;
    visible_content(content, &completed.finish_reason)?;
    if let Some(usage) = &completed.usage {
        validate_usage(usage)?;
    }
    Ok(())
}
fn validate_error(error: &ErrorPayload) -> crate::validation::ValidationResult {
    same_contest(error.debate_id, error.contest_id)?;
    crate::validation::ensure(
        error
            .retry_action
            .as_ref()
            .is_none_or(|a| error.retryable && error.work_id == Some(a.work_id)),
        "retry action must identify retryable failed work",
    )
}
fn validate_preflight_result(
    status: &PreflightStatus,
    confirmation: &Option<Confirmation>,
    error: &Option<ErrorPayload>,
) -> crate::validation::ValidationResult {
    crate::validation::ensure(
        (*status == PreflightStatus::Confirmed) == confirmation.is_some()
            && (*status != PreflightStatus::Confirmed || error.is_none()),
        "invalid preflight result",
    )
}

impl UsageRecord {
    /// Validate provider accounting before a router-owned atomic upsert.
    pub fn validate(&self) -> crate::validation::ValidationResult {
        use crate::validation::ensure;
        ensure(self.attempt > 0, "accounting attempt must be positive")?;
        ensure(
            self.model.as_str().contains('/') && !self.model.as_str().starts_with('~'),
            "accounting requires concrete model",
        )?;
        ensure(
            self.ended_at.is_none_or(|end| end >= self.started_at),
            "invalid accounting time range",
        )?;
        ensure(
            self.day.len() == 10
                && chrono::NaiveDate::parse_from_str(&self.day, "%Y-%m-%d").is_ok(),
            "invalid accounting day",
        )?;
        for cost in [
            self.estimated_cost_usd,
            self.usage.as_ref().and_then(|u| u.cost_usd),
        ]
        .into_iter()
        .flatten()
        {
            ensure(cost.is_finite() && cost >= 0.0, "invalid accounting cost")?;
        }
        Ok(())
    }
}
