//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WorkKind {
    #[serde(rename = "reply")]
    Reply,
    #[serde(rename = "survey.extract")]
    SurveyExtract,
    #[serde(rename = "survey.vote")]
    SurveyVote,
    #[serde(rename = "survey.revote")]
    SurveyRevote,
    #[serde(rename = "survey.report")]
    SurveyReport,
    #[serde(rename = "debate.turn")]
    DebateTurn,
    #[serde(rename = "judge")]
    Judge,
    #[serde(rename = "judge.round")]
    JudgeRound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    pub work_id: WorkId,
    pub message_id: MessageId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_reply_to_message_id: Option<MessageId>,
    pub kind: WorkKind,
    pub target: Target,
    pub context_id: ContextId,
    pub trigger_event_id: EventId,
    pub history_through: HistoryPosition,
    pub attempt: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_id: Option<DebateId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<CaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_ref: Option<PromptRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contest_id: Option<ContestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<StepId>,
}

impl Work {
    /// Validate executable work. Ownership must also match its frozen assignment at allocation/read time.
    pub fn validate(&self) -> crate::validation::ValidationResult {
        use crate::validation::ensure;
        ensure(self.attempt > 0, "executable attempt must be positive")?;
        ensure(self.replay.is_none_or(|r| r <= 1), "invalid replay index")?;
        ensure(
            self.debate_id.is_none() || self.contest_id.is_some(),
            "debateId requires contestId",
        )?;
        ensure(
            self.round.is_none() || self.contest_id.is_some(),
            "round requires contestId",
        )?;
        if let (Some(debate), Some(contest)) = (self.debate_id, self.contest_id) {
            ensure(debate == contest, "debate/contest identity mismatch")?;
        }
        ensure(
            self.step_id.is_none() || self.contest_id.is_some(),
            "step requires contest",
        )?;
        let owner = match &self.target {
            Target::Member(nick) => ExecutorKey::Member(nick.clone()),
            Target::Judge => ExecutorKey::Case(self.case_id.ok_or_else(|| {
                crate::validation::ValidationError {
                    message: "judge requires case".into(),
                }
            })?),
        };
        ensure(owner.owns(self), "invalid executor context")?;
        match self.kind {
            WorkKind::SurveyExtract
            | WorkKind::SurveyReport
            | WorkKind::SurveyVote
            | WorkKind::SurveyRevote => {
                ensure(
                    self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.round.is_none(),
                    "survey requires isolated step",
                )?;
                let judge = matches!(self.kind, WorkKind::SurveyExtract | WorkKind::SurveyReport);
                ensure(
                    judge == matches!(self.target, Target::Judge)
                        && judge == self.case_id.is_some(),
                    "survey actor mismatch",
                )?;
                ensure(
                    self.attempt <= if judge { 3 } else { 2 },
                    "survey attempt limit",
                )?;
            }
            WorkKind::Reply => {
                ensure(
                    self.in_reply_to_message_id.is_some(),
                    "reply requires inReplyToMessageId",
                )?;
                ensure(
                    matches!(self.target, Target::Member(_)),
                    "reply requires member",
                )?;
                ensure(
                    self.debate_id.is_none() && self.case_id.is_none(),
                    "reply cannot carry debate or case",
                )?;
            }
            WorkKind::DebateTurn => {
                ensure(
                    matches!(self.target, Target::Member(_)),
                    "turn requires member",
                )?;
                ensure(
                    self.debate_id.is_some()
                        && self
                            .round
                            .is_some_and(|r| r <= u32::from(constants::MAX_ROUNDS)),
                    "turn requires debate and round",
                )?;
                ensure(
                    self.case_id.is_none() && self.step_id.is_none(),
                    "turn cannot carry case or step",
                )?;
            }
            WorkKind::Judge | WorkKind::JudgeRound => {
                ensure(
                    self.target == Target::Judge,
                    "judge work requires judge target",
                )?;
                if self.kind == WorkKind::JudgeRound {
                    ensure(
                        self.attempt <= u32::from(constants::ROUND_JUDGE_ATTEMPTS),
                        "judge.round attempt exceeds limit",
                    )?;
                    ensure(
                        self.debate_id.is_some()
                            && self.round.is_some_and(|r| {
                                (u32::from(constants::MIN_ROUNDS)
                                    ..=u32::from(constants::MAX_ROUNDS))
                                    .contains(&r)
                            }),
                        "judge.round requires numbered debate round",
                    )?;
                    ensure(
                        self.max_output_tokens
                            == Some(crate::constants::ROUND_JUDGE_VISIBLE_TOKENS),
                        "judge.round requires 512 visible tokens",
                    )?;
                    ensure(
                        self.prompt_ref
                            .as_ref()
                            .is_some_and(|p| p.template_id.as_str() == "judge.round.md"),
                        "judge.round requires judge.round.md",
                    )?;
                }
            }
        }
        let ceiling = match self.kind {
            WorkKind::SurveyExtract | WorkKind::SurveyReport => 4096,
            WorkKind::SurveyVote | WorkKind::SurveyRevote => 2048,
            WorkKind::Judge => constants::FINAL_JUDGE_VISIBLE_TOKENS,
            WorkKind::JudgeRound => constants::ROUND_JUDGE_VISIBLE_TOKENS,
            WorkKind::Reply => constants::REPLY_VISIBLE_TOKENS,
            WorkKind::DebateTurn => constants::DEBATE_TURN_VISIBLE_TOKENS,
        };
        ensure(
            self.max_output_tokens.is_none_or(|n| n > 0 && n <= ceiling),
            "invalid visible token ceiling",
        )
    }
    pub fn validate_assignment(
        &self,
        executor: &ExecutorKey,
        assignment: &ContestAssignment,
    ) -> crate::validation::ValidationResult {
        self.validate()?;
        crate::validation::ensure(
            executor.owns(self)
                && assignment.target == self.target
                && assignment.context_id == self.context_id
                && assignment.work_id == Some(self.work_id),
            "work is not owned by frozen assignment",
        )
    }
    pub fn validate_round_bound(&self, total_rounds: u32) -> crate::validation::ValidationResult {
        self.validate()?;
        crate::validation::ensure(
            (u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS))
                .contains(&total_rounds)
                && self.round.is_none_or(|r| r <= total_rounds),
            "round exceeds admitted rounds",
        )
    }
}
