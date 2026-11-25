//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WorkKind {
    #[serde(rename = "reply")]
    Reply,
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
        use crate::validation::require;
        require(self.attempt > 0, "executable attempt must be positive")?;
        require(self.replay.is_none_or(|r| r <= 1), "invalid replay index")?;
        if let (Some(debate), Some(contest)) = (self.debate_id, self.contest_id) {
            require(debate == contest, "debate/contest identity mismatch")?;
        }
        require(
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
        require(owner.owns(self), "invalid executor context")?;
        match self.kind {
            WorkKind::Reply => {
                require(
                    matches!(self.target, Target::Member(_)),
                    "reply requires member",
                )?;
                require(
                    self.debate_id.is_none() && self.case_id.is_none(),
                    "reply cannot carry debate or case",
                )?;
            }
            WorkKind::DebateTurn => {
                require(
                    matches!(self.target, Target::Member(_)),
                    "turn requires member",
                )?;
                require(
                    self.debate_id.is_some() && self.round.is_some_and(|r| r <= 20),
                    "turn requires debate and round",
                )?;
                require(
                    self.case_id.is_none() && self.step_id.is_none(),
                    "turn cannot carry case or step",
                )?;
            }
            WorkKind::Judge | WorkKind::JudgeRound => {
                require(
                    self.target == Target::Judge,
                    "judge work requires judge target",
                )?;
                if self.kind == WorkKind::JudgeRound {
                    require(
                        self.debate_id.is_some()
                            && self.round.is_some_and(|r| (1..=20).contains(&r)),
                        "judge.round requires numbered debate round",
                    )?;
                    require(
                        self.max_output_tokens
                            == Some(crate::constants::ROUND_JUDGE_VISIBLE_TOKENS),
                        "judge.round requires 512 visible tokens",
                    )?;
                    require(
                        self.prompt_ref
                            .as_ref()
                            .is_some_and(|p| p.template_id.as_str() == "judge.round.md"),
                        "judge.round requires judge.round.md",
                    )?;
                }
            }
        }
        let ceiling = match self.kind {
            WorkKind::Judge => 4096,
            WorkKind::JudgeRound => 512,
            _ => 2048,
        };
        require(
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
        crate::validation::require(
            executor.owns(self)
                && assignment.target == self.target
                && assignment.context_id == self.context_id
                && assignment.work_id == Some(self.work_id),
            "work is not owned by frozen assignment",
        )
    }
    pub fn validate_round_bound(&self, total_rounds: u32) -> crate::validation::ValidationResult {
        self.validate()?;
        crate::validation::require(
            (1..=20).contains(&total_rounds) && self.round.is_none_or(|r| r <= total_rounds),
            "round exceeds admitted rounds",
        )
    }
}
