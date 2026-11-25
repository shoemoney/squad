//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateEstimate {
    pub estimated_usd: f64,
    pub reserved_usd: f64,
    pub remaining_usd: f64,
    pub model_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebatePreflight {
    pub judge_only: bool,
    pub debate_id: DebateId,
    pub plan_id: PlanId,
    pub expires_at: DateTime,
    pub estimate: DebateEstimate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<Text>,
    pub confirmation_required: bool,
    pub debaters: [MemberNick; 2],
    pub rounds: u32,
    pub judge_requested: bool,
    pub opening_round_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum PreflightStatus {
    Confirmed,
    Cancelled,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum Confirmation {
    Automatic,
    Explicit,
    Preconfirmed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebatePreflightResult {
    pub debate_id: DebateId,
    pub plan_id: PlanId,
    pub status: PreflightStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateOpening {
    pub debate_id: DebateId,
    pub debaters: [MemberNick; 2],
    pub topic_message_id: MessageId,
    pub topic_text: Text,
    pub instruction: String,
    pub rounds: u32,
    pub round: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ForfeitReason {
    Timeout,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "status",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum DebateTurnOutcome {
    Completed {
        content: String,
        model: Text,
        finish_reason: FinishReason,
        #[schemars(extend("x-required-nullable" = true))]
        usage: Option<Usage>, // nullable
    },
    Forfeit {
        reason_code: ForfeitReason,
        text: Text,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateTurn {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub debate_id: DebateId,
    pub round: u32,
    pub speaker: MemberNick,
    pub outcome: DebateTurnOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateVerdict {
    #[serde(flatten)]
    pub completed: CompletedGeneration,
    pub debate_id: DebateId,
    pub case_id: CaseId,
    pub judge_invocation_id: EventId,
    pub rubric_version: Text,
    #[serde(flatten)]
    pub verdict: Verdict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoundScore {
    #[schemars(range(min = 1, max = 10))]
    pub score: u8,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateRoundScore {
    pub debate_id: DebateId,
    pub case_id: CaseId,
    pub round: u32,
    #[schemars(extend("x-required-nullable" = true))]
    pub scores: Option<std::collections::BTreeMap<MemberNick, RoundScore>>, // nullable
    pub tally: std::collections::BTreeMap<MemberNick, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", rename_all_fields = "camelCase")]
pub enum DebateStatus {
    Preparing,
    AwaitingConfirmation,
    WaitingForDebaters,
    RoundRunning,
    RoundCommitting,
    Judging,
    Stopping,
    Completed,
    CompleteUnjudged,
    Stopped,
    Failed,
    Rejected,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateState {
    pub debate_id: DebateId,
    pub state: DebateStatus,
    pub debaters: [MemberNick; 2],
    pub round: u32,
    pub total_rounds: u32,
    pub judge_model: String,
    pub judge_requested: bool,
    pub state_version: u64,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<String>, // nullable
    #[schemars(extend("x-required-nullable" = true))]
    pub case_id: Option<CaseId>, // nullable
    pub case_ids: Vec<CaseId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DebateTransition {
    Prepare,
    PlanReady,
    PlanRejected,
    Confirm,
    Cancel,
    Expire,
    WaitForDebaters,
    DebatersReady,
    RoundEntered,
    SlotResolved { outcome_event_id: EventId },
    RoundResolved,
    OpeningBarrierCommitted,
    NumberedBarrierCommitted,
    JudgedFinalBarrierCommitted,
    UnjudgedFinalBarrierCommitted,
    VerdictAccepted { verdict_event_id: EventId },
    FinalJudgingFailed,
    StopRequested,
    StopCommitted,
    UnrecoverableFailure,
    RoundScoreAccepted { score_event_id: EventId },
    RoundScoreExhausted { work_id: WorkId },
    RoundScoringFinished,
    JudgeOnlyRequested,
    JudgeOnlyPlanReady,
    JudgeOnlyPlanRejected,
    JudgeOnlyConfirmed,
    JudgeOnlyCancelled,
    JudgeOnlyExpired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateSolution {
    pub nick: MemberNick,
    pub message_id: MessageId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateSetup {
    pub debate_id: DebateId,
    pub topic_message_id: MessageId,
    pub topic_text: String,
    pub solutions: [DebateSolution; 2],
    pub instruction: String,
    pub rounds: u32,
    pub judge_model: String,
    pub judge_swapped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdjudicationTopic {
    pub message_id: MessageId,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RubricCriterion {
    pub criterion_id: CriterionId,
    pub weight_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrozenDebateRubric {
    pub version: String,
    pub criteria: Vec<RubricCriterion>,
    pub verdict_schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JudgeProvenance {
    pub model: String,
    pub prompt_pack_version: String,
    pub rendered_template_hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateScoringEvidence {
    pub round_scores: Vec<DebateRoundScore>,
    pub round_tally: BTreeMap<MemberNick, u32>,
    pub unscored_rounds: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DebateAdjudicationBundle {
    pub topic: AdjudicationTopic,
    pub solutions: [DebateSolution; 2],
    pub transcript: Vec<DebateTurn>,
    pub instruction: String,
    pub rubric: FrozenDebateRubric,
    pub judge: JudgeProvenance,
    pub round_scoring: DebateScoringEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CriterionId {
    Fit,
    Feasibility,
    Originality,
    SurvivedAttacks,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CriterionScore {
    pub criterion_id: CriterionId,
    #[schemars(range(min = 1, max = 10))]
    pub score: u8,
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct JudgeScore {
    pub nick: MemberNick,
    pub criteria: Vec<CriterionScore>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JudgeVerdict {
    pub winner: MemberNick,
    pub scores: Vec<JudgeScore>,
    pub reasoning: String,
    pub what_to_steal_from_loser: Vec<String>,
    #[schemars(extend("x-required-nullable" = true))]
    pub tally_override_reason: Option<String>, // nullable
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WeightedTotal {
    pub nick: MemberNick,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    #[serde(flatten)]
    pub verdict: JudgeVerdict,
    pub weighted_totals: Vec<WeightedTotal>,
    pub round_tally: BTreeMap<MemberNick, u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RoundJudgeResponse {
    pub scores: BTreeMap<MemberNick, RoundScore>,
}

impl DebateEstimate {
    pub fn validate(&self) -> crate::validation::ValidationResult {
        crate::validation::ensure(
            [self.estimated_usd, self.reserved_usd, self.remaining_usd]
                .into_iter()
                .all(crate::validation::money),
            "invalid debate estimate",
        )
    }
}
impl DebatePreflight {
    pub fn planned_calls(&self) -> u32 {
        if self.judge_only {
            return 1;
        }
        2 * (self.rounds + u32::from(self.opening_round_required))
            + if self.judge_requested {
                self.rounds + 1
            } else {
                0
            }
    }
    pub fn reserved_calls(&self) -> u32 {
        if self.judge_only {
            return u32::from(constants::FINAL_JUDGE_ATTEMPTS);
        }
        4 * (self.rounds + u32::from(self.opening_round_required))
            + if self.judge_requested {
                2 * self.rounds + 3
            } else {
                0
            }
    }
    pub fn validate(&self) -> crate::validation::ValidationResult {
        self.estimate.validate()?;
        crate::validation::ensure(
            self.debaters[0] != self.debaters[1]
                && (u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS))
                    .contains(&self.rounds),
            "invalid debate participants or rounds",
        )?;
        crate::validation::ensure(
            !self.judge_only || self.judge_requested,
            "judgeOnly requires judgeRequested",
        )?;
        // A judge-only plan has one final call and retains the admitted round snapshot.
        crate::validation::ensure(
            self.estimate.model_calls == self.planned_calls(),
            "invalid planned call count",
        )
    }
    pub fn validate_confirmation(
        &self,
        debate: DebateId,
        plan: PlanId,
        now: DateTime,
        unchanged: bool,
    ) -> Result<(), ErrorCode> {
        if self.debate_id != debate || self.plan_id != plan || !unchanged {
            return Err(ErrorCode::DEBATE_PLAN_STALE);
        }
        if now >= self.expires_at {
            return Err(ErrorCode::DEBATE_PLAN_EXPIRED);
        }
        Ok(())
    }
}
impl DebateState {
    pub fn validate(&self) -> crate::validation::ValidationResult {
        crate::validation::ensure(
            self.debaters[0] != self.debaters[1]
                && (u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS))
                    .contains(&self.total_rounds)
                && self.round <= self.total_rounds
                && crate::validation::unique(&self.case_ids)
                && self.case_id.is_none_or(|id| self.case_ids.contains(&id)),
            "invalid debate state",
        )
    }
}
impl DebateStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::CompleteUnjudged
                | Self::Stopped
                | Self::Failed
                | Self::Rejected
                | Self::Cancelled
                | Self::Expired
        )
    }
}
impl DebateSetup {
    pub fn validate(&self) -> crate::validation::ValidationResult {
        crate::validation::ensure(
            (u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS))
                .contains(&self.rounds)
                && self.solutions[0].nick != self.solutions[1].nick
                && !self.topic_text.trim().is_empty()
                && self.solutions.iter().all(|s| !s.text.trim().is_empty()),
            "invalid debate setup",
        )
    }
}

impl DebateVerdict {
    /// Router commit gate using admitted state and the authoritative accepted tally.
    pub fn validate_against(
        &self,
        state: &DebateState,
        tally: &BTreeMap<MemberNick, u32>,
    ) -> validation::ValidationResult {
        state.validate()?;
        validation::ensure(
            self.debate_id == state.debate_id && state.case_id == Some(self.case_id),
            "verdict debate/case differs from admitted state",
        )?;
        verdict::validate_committed_verdict(&self.verdict, &state.debaters, tally)
    }
}
impl DebateRoundScore {
    /// Router commit gate using the admitted participants, case and round limit.
    pub fn validate_against(&self, state: &DebateState) -> validation::ValidationResult {
        state.validate()?;
        validation::ensure(
            self.debate_id == state.debate_id && state.case_id == Some(self.case_id),
            "round score debate/case differs from admitted state",
        )?;
        verdict::validate_round_score(self, &state.debaters, state.total_rounds)
    }
}
