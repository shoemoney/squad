//! Typed shared-floor grill contracts.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GrillStatus {
    Preflight,
    WaitingForParticipants,
    Questioning,
    Answering,
    Rebuilding,
    Complete,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillSetup {
    pub contest_id: ContestId,
    pub leader: MemberNick,
    pub grillers: Vec<MemberNick>,
    pub source_message_id: MessageId,
    pub initial_plan: String,
    pub focus: String,
    pub total_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillState {
    pub contest_id: ContestId,
    pub state: GrillStatus,
    pub state_version: u64,
    pub round: u32,
    pub total_rounds: u32,
    pub leader: MemberNick,
    pub griller_order: Vec<MemberNick>,
    pub turn: u32,
    pub plan_version: u32,
    pub questions_asked: u32,
    pub passed: u32,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<String>, // nullable
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillQuestion {
    pub contest_id: ContestId,
    pub round: u32,
    pub turn: u32,
    pub griller: MemberNick,
    pub leader: MemberNick,
    pub plan_version: u32,
    pub angle: String,
    pub questions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillAnswerItem {
    pub question_index: u8,
    pub answer: String,
    pub plan_changes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillAnswer {
    pub contest_id: ContestId,
    pub round: u32,
    pub turn: u32,
    pub leader: MemberNick,
    pub question_event_id: EventId,
    pub answers: Vec<GrillAnswerItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrillPassReason {
    Pass,
    Timeout,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillPass {
    pub contest_id: ContestId,
    pub round: u32,
    pub turn: u32,
    pub griller: MemberNick,
    pub reason: GrillPassReason,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillPlan {
    pub contest_id: ContestId,
    pub round: u32,
    pub version: u32,
    pub leader: MemberNick,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillAppendixEntry {
    pub question_event_id: EventId,
    pub question_index: u8,
    pub griller: MemberNick,
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_changes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GrillComplete {
    pub contest_id: ContestId,
    pub final_plan_event_id: EventId,
    pub final_plan_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub appendix: Vec<GrillAppendixEntry>,
}

/// Settings frozen inside the shared contest.preflight carrier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrillPreflight {
    pub setup: GrillSetup,
    pub answer_words: u32,
    pub plan_words: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum GrillTransition {
    ParticipantsReady,
    QuestionsCommitted { question_event_id: EventId },
    AnswerCommitted { answer_event_id: EventId },
    PassCommitted { pass_event_id: EventId },
    PlanCommitted { plan_event_id: EventId },
    Stop,
    LeaderFailed,
}
impl GrillStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preflight => "PREFLIGHT",
            Self::WaitingForParticipants => "WAITING_FOR_PARTICIPANTS",
            Self::Questioning => "QUESTIONING",
            Self::Answering => "ANSWERING",
            Self::Rebuilding => "REBUILDING",
            Self::Complete => "COMPLETE",
            Self::Stopped => "STOPPED",
            Self::Failed => "FAILED",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
impl GrillSetup {
    pub fn validate(&self) -> validation::ValidationResult {
        use validation::ensure;
        ensure(
            (1..=10).contains(&self.total_rounds),
            "invalid grill rounds",
        )?;
        ensure(!self.initial_plan.trim().is_empty(), "missing grill plan")?;
        ensure(
            !self.grillers.is_empty() && self.grillers.len() < constants::MAX_MEMBERS,
            "invalid grillers",
        )?;
        let mut seen = std::collections::BTreeSet::new();
        ensure(
            self.grillers
                .iter()
                .all(|n| n != &self.leader && seen.insert(n)),
            "duplicate grill participant",
        )
    }
    pub fn participants(&self) -> Vec<MemberNick> {
        std::iter::once(self.leader.clone())
            .chain(self.grillers.clone())
            .collect()
    }
    pub fn normal_calls(&self) -> u32 {
        self.total_rounds * (self.grillers.len() as u32 * 2 + 1)
    }
}
impl GrillState {
    pub fn validate(&self) -> validation::ValidationResult {
        use validation::ensure;
        ensure(
            self.state_version > 0 && (1..=10).contains(&self.total_rounds),
            "invalid grill version or rounds",
        )?;
        ensure(
            (1..=self.total_rounds).contains(&self.round)
                && self.plan_version > 0
                && self.plan_version <= self.total_rounds + 1,
            "invalid grill progress",
        )?;
        let mut seen = std::collections::BTreeSet::new();
        ensure(
            !self.griller_order.is_empty()
                && self.griller_order.len() < constants::MAX_MEMBERS
                && self
                    .griller_order
                    .iter()
                    .all(|n| n != &self.leader && seen.insert(n)),
            "invalid grill floor",
        )?;
        ensure(
            (self.turn as usize) < self.griller_order.len(),
            "invalid grill turn",
        )
    }
}
impl GrillQuestion {
    pub fn validate(&self) -> validation::ValidationResult {
        use validation::ensure;
        ensure(
            (1..=10).contains(&self.round) && self.plan_version > 0 && self.griller != self.leader,
            "invalid grill question identity",
        )?;
        ensure(
            (1..=3).contains(&self.questions.len())
                && self
                    .questions
                    .iter()
                    .all(|q| !q.trim().is_empty() && q != "PASS"),
            "invalid grill questions",
        )?;
        let mut seen = std::collections::BTreeSet::new();
        ensure(
            self.questions.iter().all(|q| seen.insert(q.trim())),
            "duplicate grill question",
        )?;
        ensure(
            self.questions
                .iter()
                .map(|q| q.split_whitespace().count())
                .sum::<usize>()
                <= constants::GRILL_QUESTION_WORDS as usize,
            "grill question word limit",
        )
    }
}
impl GrillAnswer {
    pub fn validate_against(
        &self,
        question: &GrillQuestion,
        question_id: EventId,
        word_limit: u32,
    ) -> validation::ValidationResult {
        use validation::ensure;
        question.validate()?;
        ensure(
            self.contest_id == question.contest_id
                && self.round == question.round
                && self.turn == question.turn
                && self.leader == question.leader
                && self.question_event_id == question_id,
            "grill answer reference mismatch",
        )?;
        let mut seen = std::collections::BTreeSet::new();
        ensure(
            self.answers.len() == question.questions.len()
                && self.answers.iter().all(|a| {
                    (a.question_index as usize) < question.questions.len()
                        && seen.insert(a.question_index)
                        && !a.answer.trim().is_empty()
                        && !a.plan_changes.trim().is_empty()
                }),
            "incomplete grill answer coverage",
        )?;
        ensure(
            self.answers
                .iter()
                .map(|a| {
                    a.answer.split_whitespace().count() + a.plan_changes.split_whitespace().count()
                })
                .sum::<usize>()
                <= word_limit as usize,
            "grill answer word limit",
        )
    }
}
impl GrillPlan {
    pub fn validate(&self, word_limit: u32) -> validation::ValidationResult {
        validation::ensure(
            self.version > 0
                && self.version <= 11
                && self.round <= 10
                && !self.text.trim().is_empty()
                && self.text.split_whitespace().count() <= word_limit as usize,
            "invalid grill plan",
        )
    }
}
