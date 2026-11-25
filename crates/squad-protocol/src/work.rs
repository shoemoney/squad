//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WorkKind {
    #[serde(rename = "grill.question")]
    GrillQuestion,
    #[serde(rename = "grill.answer")]
    GrillAnswer,
    #[serde(rename = "grill.rebuild")]
    GrillRebuild,
    #[serde(rename = "reply")]
    Reply,
    #[serde(rename = "crosscheck.goal")]
    CrosscheckGoal,
    #[serde(rename = "crosscheck.inventory")]
    CrosscheckInventory,
    #[serde(rename = "crosscheck.normalize")]
    CrosscheckNormalize,
    #[serde(rename = "crosscheck.cell")]
    CrosscheckCell,
    #[serde(rename = "crosscheck.audit")]
    CrosscheckAudit,
    #[serde(rename = "crosscheck.rule")]
    CrosscheckRule,
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.extract")]
    SurveyExtract,
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.vote")]
    SurveyVote,
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.revote")]
    SurveyRevote,
    #[cfg(feature = "mode-survey")]
    #[serde(rename = "survey.report")]
    SurveyReport,
    #[serde(rename = "debate.turn")]
    DebateTurn,
    #[serde(rename = "judge")]
    Judge,
    #[serde(rename = "judge.round")]
    JudgeRound,
    #[serde(rename = "judge.tournament")]
    JudgeTournament,
    #[serde(rename = "census.hunt")]
    CensusHunt,
    #[serde(rename = "census.rate")]
    CensusRate,
    #[serde(rename = "census.merge")]
    CensusMerge,
    #[serde(rename = "census.final")]
    CensusFinal,
    #[serde(rename = "probe.explain")]
    ProbeExplain,
    #[serde(rename = "probe.check")]
    ProbeCheck,
    #[serde(rename = "probe.predict")]
    ProbePredict,
    #[serde(rename = "probe.report")]
    ProbeReport,
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
            WorkKind::CensusHunt
            | WorkKind::CensusRate
            | WorkKind::CensusMerge
            | WorkKind::CensusFinal => {
                ensure(
                    self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.round.is_none()
                        && self.in_reply_to_message_id.is_none(),
                    "census requires isolated step",
                )?;
                ensure(
                    self.attempt <= if self.kind.census_judge() { 3 } else { 2 },
                    "census attempt exceeds limit",
                )?;
                ensure(
                    self.kind.census_judge() == (self.target == Target::Judge),
                    "invalid census target",
                )?;
                ensure(
                    self.prompt_ref.as_ref().is_some_and(|p| {
                        p.template_id.as_str() == self.kind.census_composition().unwrap_or_default()
                    }),
                    "invalid census prompt",
                )?;
                if let Target::Member(nick) = &self.target {
                    ensure(
                        self.context_id.0
                            == format!(
                                "contest:{}:{}:{nick}",
                                self.contest_id.expect("checked"),
                                self.step_id.expect("checked")
                            )
                            && self.case_id.is_none(),
                        "census requires fresh member context",
                    )?;
                }
            }
            WorkKind::CrosscheckInventory
            | WorkKind::CrosscheckCell
            | WorkKind::CrosscheckAudit
            | WorkKind::CrosscheckGoal
            | WorkKind::CrosscheckNormalize
            | WorkKind::CrosscheckRule => {
                ensure(
                    self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.round.is_none(),
                    "crosscheck requires contest step",
                )?;
                ensure(
                    (self.kind.is_crosscheck_judge()) == (self.target == Target::Judge),
                    "crosscheck target mismatch",
                )?;
                if let Target::Member(nick) = &self.target {
                    ensure(
                        self.case_id.is_none()
                            && self.context_id.0
                                == format!(
                                    "contest:{}:{}:{}",
                                    self.contest_id.expect("checked contest"),
                                    self.step_id.expect("checked step"),
                                    nick
                                ),
                        "crosscheck member context must be fresh and step-bound",
                    )?;
                }
                ensure(
                    self.attempt
                        <= if self.kind.is_crosscheck_judge() {
                            3
                        } else {
                            2
                        },
                    "crosscheck attempt limit",
                )?;
            }
            #[cfg(feature = "mode-survey")]
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
                if !judge {
                    let Target::Member(nick) = &self.target else {
                        unreachable!()
                    };
                    ensure(
                        self.context_id.0
                            == format!(
                                "contest:{}:{}:{nick}",
                                self.contest_id.expect("checked"),
                                self.step_id.expect("checked")
                            ),
                        "survey ballot requires fresh context",
                    )?;
                }
                ensure(
                    self.attempt <= if judge { 3 } else { 2 },
                    "survey attempt limit",
                )?;
            }
            WorkKind::GrillQuestion | WorkKind::GrillAnswer | WorkKind::GrillRebuild => {
                ensure(
                    matches!(self.target, Target::Member(_))
                        && self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.case_id.is_none()
                        && self.round.is_some_and(|r| (1..=10).contains(&r))
                        && self.attempt <= 2,
                    "invalid grill work",
                )?;
                let Target::Member(nick) = &self.target else {
                    unreachable!("checked member")
                };
                ensure(
                    self.context_id.0
                        == format!(
                            "contest:{}:{}:{}",
                            self.contest_id.expect("checked contest"),
                            self.step_id.expect("checked step"),
                            nick
                        ),
                    "grill requires fresh step context",
                )?;
                let template = match self.kind {
                    WorkKind::GrillQuestion => "grill.question",
                    WorkKind::GrillAnswer => "grill.answer",
                    _ => "grill.rebuild",
                };
                ensure(
                    self.prompt_ref.as_ref().is_some_and(|p| {
                        p.template_id.as_str().trim_end_matches(".md") == template
                    }),
                    "grill prompt mismatch",
                )?;
            }
            WorkKind::ProbeExplain
            | WorkKind::ProbeCheck
            | WorkKind::ProbePredict
            | WorkKind::ProbeReport => {
                ensure(
                    self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.round.is_none(),
                    "probe requires contest and step",
                )?;
                ensure(
                    self.prompt_ref.as_ref().is_some_and(|p| {
                        Some(p.template_id.as_str()) == self.kind.probe_composition()
                    }),
                    "probe requires its owning composition",
                )?;
                ensure(
                    if self.kind == WorkKind::ProbeReport {
                        self.target == Target::Judge && self.attempt <= 3
                    } else {
                        matches!(self.target, Target::Member(_))
                            && self.case_id.is_none()
                            && self.attempt <= 2
                    },
                    "invalid probe owner or attempt",
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
            WorkKind::Judge | WorkKind::JudgeRound | WorkKind::JudgeTournament => {
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
        if self.kind == WorkKind::JudgeTournament
            || (self.kind == WorkKind::Reply && self.round.is_some())
        {
            ensure(
                self.contest_id.is_some()
                    && self.debate_id.is_none()
                    && self.round.is_some_and(|r| r > 0)
                    && self.replay.is_some(),
                "tournament work requires contest, round and replay",
            )?;
            ensure(
                self.prompt_ref.as_ref().is_some_and(|p| {
                    if self.kind == WorkKind::Reply {
                        p.template_id.as_str() == "tournament.entrant.md"
                    } else {
                        matches!(
                            p.template_id.as_str(),
                            "tournament.judge.md" | "judge.tournament"
                        )
                    }
                }),
                "invalid tournament template",
            )?;
        }
        if self.kind == WorkKind::JudgeTournament {
            ensure(
                self.attempt <= 3
                    && self.max_output_tokens == Some(constants::FINAL_JUDGE_VISIBLE_TOKENS),
                "invalid tournament judge allowance",
            )?;
        }
        let ceiling = match self.kind {
            WorkKind::Judge | WorkKind::JudgeTournament => constants::FINAL_JUDGE_VISIBLE_TOKENS,
            WorkKind::CensusMerge | WorkKind::CensusFinal => {
                constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            }
            WorkKind::CensusHunt | WorkKind::CensusRate => 2048,
            WorkKind::CrosscheckInventory
            | WorkKind::CrosscheckCell
            | WorkKind::CrosscheckAudit => 2048,
            WorkKind::CrosscheckGoal | WorkKind::CrosscheckNormalize | WorkKind::CrosscheckRule => {
                constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            }
            #[cfg(feature = "mode-survey")]
            WorkKind::SurveyExtract | WorkKind::SurveyReport => {
                constants::NEW_MODE_JUDGE_VISIBLE_TOKENS
            }
            #[cfg(feature = "mode-survey")]
            WorkKind::SurveyVote | WorkKind::SurveyRevote => 2048,
            WorkKind::GrillQuestion | WorkKind::GrillAnswer => {
                constants::DEBATE_TURN_VISIBLE_TOKENS
            }
            WorkKind::GrillRebuild => constants::PLAN_VISIBLE_TOKENS,
            WorkKind::ProbeExplain | WorkKind::ProbeCheck | WorkKind::ProbePredict => {
                constants::DEBATE_TURN_VISIBLE_TOKENS
            }
            WorkKind::ProbeReport => constants::FINAL_JUDGE_VISIBLE_TOKENS,
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

impl WorkKind {
    pub fn census_composition(&self) -> Option<&'static str> {
        match self {
            Self::CensusHunt => Some("census.hunt"),
            Self::CensusRate => Some("census.rate"),
            Self::CensusMerge => Some("census.merge"),
            Self::CensusFinal => Some("census.final"),
            _ => None,
        }
    }
    pub fn census_judge(&self) -> bool {
        matches!(self, Self::CensusMerge | Self::CensusFinal)
    }
    pub fn is_crosscheck_member(&self) -> bool {
        matches!(
            self,
            Self::CrosscheckInventory | Self::CrosscheckCell | Self::CrosscheckAudit
        )
    }
    pub fn is_crosscheck_judge(&self) -> bool {
        matches!(
            self,
            Self::CrosscheckGoal | Self::CrosscheckNormalize | Self::CrosscheckRule
        )
    }
    pub fn is_crosscheck(&self) -> bool {
        self.is_crosscheck_member() || self.is_crosscheck_judge()
    }
    pub fn is_probe(&self) -> bool {
        matches!(
            self,
            Self::ProbeExplain | Self::ProbeCheck | Self::ProbePredict | Self::ProbeReport
        )
    }
    pub fn probe_composition(&self) -> Option<&'static str> {
        match self {
            Self::ProbeExplain => Some("probe.explain"),
            Self::ProbeCheck => Some("probe.check"),
            Self::ProbePredict => Some("probe.predict"),
            Self::ProbeReport => Some("probe.report"),
            _ => None,
        }
    }
}

impl WorkKind {
    pub fn grill_composition(&self) -> Option<&'static str> {
        match self {
            Self::GrillQuestion => Some("grill.question"),
            Self::GrillAnswer => Some("grill.answer"),
            Self::GrillRebuild => Some("grill.rebuild"),
            _ => None,
        }
    }
}
