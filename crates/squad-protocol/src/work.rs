//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WorkKind {
    #[serde(rename = "tune.cases")]
    TuneCases,
    #[serde(rename = "tune.freeze")]
    TuneFreeze,
    #[serde(rename = "tune.execute")]
    TuneExecute,
    #[serde(rename = "tune.score")]
    TuneScore,
    #[serde(rename = "tune.mutate")]
    TuneMutate,
    #[serde(rename = "bass_ackwards_prd.done")]
    BassAckwardsPrdDone,
    #[serde(rename = "bass_ackwards_prd.freeze")]
    BassAckwardsPrdFreeze,
    #[serde(rename = "bass_ackwards_prd.tree")]
    BassAckwardsPrdTree,
    #[serde(rename = "bass_ackwards_prd.merge")]
    BassAckwardsPrdMerge,
    #[serde(rename = "bass_ackwards_prd.review")]
    BassAckwardsPrdReview,
    #[serde(rename = "bass_ackwards_prd.revise")]
    BassAckwardsPrdRevise,
    #[serde(rename = "bass_ackwards_prd.must_have")]
    BassAckwardsPrdMustHave,
    #[serde(rename = "bass_ackwards_prd.finalize")]
    BassAckwardsPrdFinalize,
    #[serde(rename = "bass_ackwards_prd.apply")]
    BassAckwardsPrdApply,

    #[serde(rename = "forge.draft")]
    ForgeDraft,
    #[serde(rename = "forge.rank")]
    ForgeRank,
    #[serde(rename = "forge.fuse")]
    ForgeFuse,
    #[serde(rename = "forge.judge")]
    ForgeJudge,
    #[serde(rename = "forecast.frame")]
    ForecastFrame,
    #[serde(rename = "forecast.estimate")]
    ForecastEstimate,
    #[serde(rename = "forecast.revise")]
    ForecastRevise,
    #[serde(rename = "forecast.cruxes")]
    ForecastCruxes,
    #[serde(rename = "decide.options")]
    DecideOptions,
    #[serde(rename = "decide.shortlist")]
    DecideShortlist,
    #[serde(rename = "decide.criteria")]
    DecideCriteria,
    #[serde(rename = "decide.freeze")]
    DecideFreeze,
    #[serde(rename = "decide.score")]
    DecideScore,
    #[serde(rename = "decide.explain")]
    DecideExplain,
    #[serde(rename = "negotiate.map")]
    NegotiateMap,
    #[serde(rename = "negotiate.brief")]
    NegotiateBrief,
    #[serde(rename = "negotiate.package")]
    NegotiatePackage,
    #[serde(rename = "negotiate.move")]
    NegotiateMove,
    #[serde(rename = "negotiate.counter")]
    NegotiateCounter,
    #[serde(rename = "negotiate.playbook")]
    NegotiatePlaybook,
    #[serde(rename = "blow_it_up.attack")]
    BlowItUpAttack,
    #[serde(rename = "blow_it_up.challenge")]
    BlowItUpChallenge,
    #[serde(rename = "blow_it_up.rule")]
    BlowItUpRule,
    #[serde(rename = "readback.interpret")]
    ReadbackInterpret,
    #[serde(rename = "readback.diff")]
    ReadbackDiff,
    #[serde(rename = "readback.rewrite")]
    ReadbackRewrite,
    #[serde(rename = "debug.hypothesize")]
    DebuggingHypothesize,
    #[serde(rename = "debug.merge")]
    DebuggingMerge,
    #[serde(rename = "debug.test")]
    DebuggingTest,
    #[serde(rename = "debug.select")]
    DebuggingSelect,
    #[serde(rename = "debug.report")]
    DebuggingReport,
    #[serde(rename = "trace.extract")]
    TraceExtract,
    #[serde(rename = "trace.merge")]
    TraceMerge,
    #[serde(rename = "trace.verify")]
    TraceVerify,
    #[serde(rename = "trace.assemble")]
    TraceAssemble,
    #[serde(rename = "rank.compare")]
    RankCompare,
    #[serde(rename = "rank.explain")]
    RankExplain,

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
            WorkKind::TuneExecute => {
                ensure(
                    self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.in_reply_to_message_id.is_none()
                        && self.attempt <= 2
                        && self.prompt_ref.is_none()
                        && self.max_output_tokens == Some(constants::TUNE_EXEC_VISIBLE_TOKENS),
                    "invalid bare executor work",
                )?;
                if let Target::Member(nick) = &self.target {
                    ensure(
                        self.case_id.is_none()
                            && self.context_id.0
                                == format!(
                                    "contest:{}:{}:{nick}",
                                    self.contest_id.expect("checked"),
                                    self.step_id.expect("checked")
                                ),
                        "executor needs fresh context",
                    )?;
                }
            }

            WorkKind::TuneCases
            | WorkKind::TuneFreeze
            | WorkKind::TuneScore
            | WorkKind::TuneMutate
            | WorkKind::BassAckwardsPrdDone
            | WorkKind::BassAckwardsPrdFreeze
            | WorkKind::BassAckwardsPrdTree
            | WorkKind::BassAckwardsPrdMerge
            | WorkKind::BassAckwardsPrdReview
            | WorkKind::BassAckwardsPrdRevise
            | WorkKind::BassAckwardsPrdMustHave
            | WorkKind::BassAckwardsPrdFinalize
            | WorkKind::BassAckwardsPrdApply
            | WorkKind::ForgeDraft
            | WorkKind::ForgeRank
            | WorkKind::ForgeFuse
            | WorkKind::ForgeJudge
            | WorkKind::ForecastFrame
            | WorkKind::ForecastEstimate
            | WorkKind::ForecastRevise
            | WorkKind::ForecastCruxes
            | WorkKind::DecideOptions
            | WorkKind::DecideShortlist
            | WorkKind::DecideCriteria
            | WorkKind::DecideFreeze
            | WorkKind::DecideScore
            | WorkKind::DecideExplain
            | WorkKind::NegotiateMap
            | WorkKind::NegotiateBrief
            | WorkKind::NegotiatePackage
            | WorkKind::NegotiateMove
            | WorkKind::NegotiateCounter
            | WorkKind::NegotiatePlaybook
            | WorkKind::BlowItUpAttack
            | WorkKind::BlowItUpChallenge
            | WorkKind::BlowItUpRule
            | WorkKind::ReadbackInterpret
            | WorkKind::ReadbackDiff
            | WorkKind::ReadbackRewrite
            | WorkKind::DebuggingHypothesize
            | WorkKind::DebuggingMerge
            | WorkKind::DebuggingTest
            | WorkKind::DebuggingSelect
            | WorkKind::DebuggingReport
            | WorkKind::TraceExtract
            | WorkKind::TraceMerge
            | WorkKind::TraceVerify
            | WorkKind::TraceAssemble
            | WorkKind::RankCompare
            | WorkKind::RankExplain => {
                ensure(
                    self.contest_id.is_some()
                        && self.step_id.is_some()
                        && self.debate_id.is_none()
                        && self.in_reply_to_message_id.is_none(),
                    "new mode requires isolated step",
                )?;
                ensure(
                    self.kind.new_mode_judge() == (self.target == Target::Judge)
                        && self.attempt <= u32::from(constants::attempt_limit(&self.kind)),
                    "invalid mode owner or attempt",
                )?;
                ensure(
                    self.prompt_ref.as_ref().is_some_and(|p| {
                        Some(p.template_id.as_str()) == self.kind.new_mode_composition()
                    }),
                    "invalid mode composition",
                )?;
                if let Target::Member(nick) = &self.target {
                    ensure(
                        self.case_id.is_none()
                            && self.context_id.0
                                == format!(
                                    "contest:{}:{}:{nick}",
                                    self.contest_id.expect("checked"),
                                    self.step_id.expect("checked")
                                ),
                        "new mode requires fresh member context",
                    )?;
                }
            }

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
            WorkKind::TuneCases
            | WorkKind::TuneFreeze
            | WorkKind::TuneExecute
            | WorkKind::TuneScore
            | WorkKind::TuneMutate
            | WorkKind::BassAckwardsPrdDone
            | WorkKind::BassAckwardsPrdFreeze
            | WorkKind::BassAckwardsPrdTree
            | WorkKind::BassAckwardsPrdMerge
            | WorkKind::BassAckwardsPrdReview
            | WorkKind::BassAckwardsPrdRevise
            | WorkKind::BassAckwardsPrdMustHave
            | WorkKind::BassAckwardsPrdFinalize
            | WorkKind::BassAckwardsPrdApply => {
                self.kind.new_mode_visible_tokens().expect("new mode")
            }

            WorkKind::ForgeDraft
            | WorkKind::ForgeRank
            | WorkKind::ForgeFuse
            | WorkKind::ForgeJudge
            | WorkKind::ForecastFrame
            | WorkKind::ForecastEstimate
            | WorkKind::ForecastRevise
            | WorkKind::ForecastCruxes
            | WorkKind::DecideOptions
            | WorkKind::DecideShortlist
            | WorkKind::DecideCriteria
            | WorkKind::DecideFreeze
            | WorkKind::DecideScore
            | WorkKind::DecideExplain
            | WorkKind::NegotiateMap
            | WorkKind::NegotiateBrief
            | WorkKind::NegotiatePackage
            | WorkKind::NegotiateMove
            | WorkKind::NegotiateCounter
            | WorkKind::NegotiatePlaybook
            | WorkKind::BlowItUpAttack
            | WorkKind::BlowItUpChallenge
            | WorkKind::BlowItUpRule
            | WorkKind::ReadbackInterpret
            | WorkKind::ReadbackDiff
            | WorkKind::ReadbackRewrite
            | WorkKind::DebuggingHypothesize
            | WorkKind::DebuggingMerge
            | WorkKind::DebuggingTest
            | WorkKind::DebuggingSelect
            | WorkKind::DebuggingReport
            | WorkKind::TraceExtract
            | WorkKind::TraceMerge
            | WorkKind::TraceVerify
            | WorkKind::TraceAssemble
            | WorkKind::RankCompare
            | WorkKind::RankExplain => {
                if self.kind.new_mode_judge()
                    || matches!(
                        self.kind,
                        WorkKind::ForgeDraft
                            | WorkKind::ForgeFuse
                            | WorkKind::DecideScore
                            | WorkKind::TraceExtract
                            | WorkKind::RankCompare
                    )
                {
                    constants::PLAN_VISIBLE_TOKENS
                } else {
                    constants::REPLY_VISIBLE_TOKENS
                }
            }
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

impl WorkKind {
    pub fn new_mode_kind(&self) -> Option<ContestKind> {
        match self {
            Self::TuneCases
            | Self::TuneFreeze
            | Self::TuneExecute
            | Self::TuneScore
            | Self::TuneMutate => Some(ContestKind::Tune),
            Self::BassAckwardsPrdDone
            | Self::BassAckwardsPrdFreeze
            | Self::BassAckwardsPrdTree
            | Self::BassAckwardsPrdMerge
            | Self::BassAckwardsPrdReview
            | Self::BassAckwardsPrdRevise
            | Self::BassAckwardsPrdMustHave
            | Self::BassAckwardsPrdFinalize
            | Self::BassAckwardsPrdApply => Some(ContestKind::BassAckwardsPrd),

            Self::ForgeDraft | Self::ForgeRank | Self::ForgeFuse | Self::ForgeJudge => {
                Some(ContestKind::Forge)
            }
            Self::ForecastFrame
            | Self::ForecastEstimate
            | Self::ForecastRevise
            | Self::ForecastCruxes => Some(ContestKind::Forecast),
            Self::DecideOptions
            | Self::DecideShortlist
            | Self::DecideCriteria
            | Self::DecideFreeze
            | Self::DecideScore
            | Self::DecideExplain => Some(ContestKind::Decide),
            Self::NegotiateMap
            | Self::NegotiateBrief
            | Self::NegotiatePackage
            | Self::NegotiateMove
            | Self::NegotiateCounter
            | Self::NegotiatePlaybook => Some(ContestKind::Negotiate),
            Self::BlowItUpAttack | Self::BlowItUpChallenge | Self::BlowItUpRule => {
                Some(ContestKind::BlowItUp)
            }
            Self::ReadbackInterpret | Self::ReadbackDiff | Self::ReadbackRewrite => {
                Some(ContestKind::Readback)
            }
            Self::DebuggingHypothesize
            | Self::DebuggingMerge
            | Self::DebuggingTest
            | Self::DebuggingSelect
            | Self::DebuggingReport => Some(ContestKind::Debugging),
            Self::TraceExtract | Self::TraceMerge | Self::TraceVerify | Self::TraceAssemble => {
                Some(ContestKind::Trace)
            }
            Self::RankCompare | Self::RankExplain => Some(ContestKind::Rank),
            _ => None,
        }
    }
    pub fn new_mode_composition(&self) -> Option<&'static str> {
        match self {
            Self::TuneCases => Some("tune.cases"),
            Self::TuneFreeze => Some("tune.freeze"),
            Self::TuneScore => Some("tune.score"),
            Self::TuneMutate => Some("tune.mutate"),
            Self::BassAckwardsPrdDone => Some("bass_ackwards_prd.done"),
            Self::BassAckwardsPrdFreeze => Some("bass_ackwards_prd.freeze"),
            Self::BassAckwardsPrdTree => Some("bass_ackwards_prd.tree"),
            Self::BassAckwardsPrdMerge => Some("bass_ackwards_prd.merge"),
            Self::BassAckwardsPrdReview => Some("bass_ackwards_prd.review"),
            Self::BassAckwardsPrdRevise => Some("bass_ackwards_prd.revise"),
            Self::BassAckwardsPrdMustHave => Some("bass_ackwards_prd.must_have"),
            Self::BassAckwardsPrdFinalize => Some("bass_ackwards_prd.finalize"),
            Self::BassAckwardsPrdApply => Some("bass_ackwards_prd.apply"),

            Self::ForgeDraft => Some("forge.draft"),
            Self::ForgeRank => Some("forge.rank"),
            Self::ForgeFuse => Some("forge.fuse"),
            Self::ForgeJudge => Some("forge.judge"),
            Self::ForecastFrame => Some("forecast.frame"),
            Self::ForecastEstimate => Some("forecast.estimate"),
            Self::ForecastRevise => Some("forecast.revise"),
            Self::ForecastCruxes => Some("forecast.cruxes"),
            Self::DecideOptions => Some("decide.options"),
            Self::DecideShortlist => Some("decide.shortlist"),
            Self::DecideCriteria => Some("decide.criteria"),
            Self::DecideFreeze => Some("decide.freeze"),
            Self::DecideScore => Some("decide.score"),
            Self::DecideExplain => Some("decide.explain"),
            Self::NegotiateMap => Some("negotiate.map"),
            Self::NegotiateBrief => Some("negotiate.brief"),
            Self::NegotiatePackage => Some("negotiate.package"),
            Self::NegotiateMove => Some("negotiate.move"),
            Self::NegotiateCounter => Some("negotiate.counter"),
            Self::NegotiatePlaybook => Some("negotiate.playbook"),
            Self::BlowItUpAttack => Some("blow_it_up.attack"),
            Self::BlowItUpChallenge => Some("blow_it_up.challenge"),
            Self::BlowItUpRule => Some("blow_it_up.rule"),
            Self::ReadbackInterpret => Some("readback.interpret"),
            Self::ReadbackDiff => Some("readback.diff"),
            Self::ReadbackRewrite => Some("readback.rewrite"),
            Self::DebuggingHypothesize => Some("debug.hypothesize"),
            Self::DebuggingMerge => Some("debug.merge"),
            Self::DebuggingTest => Some("debug.test"),
            Self::DebuggingSelect => Some("debug.select"),
            Self::DebuggingReport => Some("debug.report"),
            Self::TraceExtract => Some("trace.extract"),
            Self::TraceMerge => Some("trace.merge"),
            Self::TraceVerify => Some("trace.verify"),
            Self::TraceAssemble => Some("trace.assemble"),
            Self::RankCompare => Some("rank.compare"),
            Self::RankExplain => Some("rank.explain"),
            _ => None,
        }
    }
    pub fn new_mode_judge(&self) -> bool {
        matches!(
            self,
            Self::TuneFreeze
                | Self::TuneScore
                | Self::BassAckwardsPrdFreeze
                | Self::BassAckwardsPrdMerge
                | Self::BassAckwardsPrdRevise
                | Self::BassAckwardsPrdFinalize
                | Self::BassAckwardsPrdApply
                | Self::ForgeJudge
                | Self::ForecastFrame
                | Self::ForecastCruxes
                | Self::DecideShortlist
                | Self::DecideFreeze
                | Self::DecideExplain
                | Self::NegotiateBrief
                | Self::NegotiatePlaybook
                | Self::BlowItUpRule
                | Self::ReadbackDiff
                | Self::ReadbackRewrite
                | Self::DebuggingMerge
                | Self::DebuggingSelect
                | Self::DebuggingReport
                | Self::TraceMerge
                | Self::TraceAssemble
                | Self::RankExplain
        )
    }
}

impl WorkKind {
    pub fn is_executor(&self) -> bool {
        matches!(self, Self::TuneExecute)
    }
    pub fn new_mode_visible_tokens(&self) -> Option<u32> {
        self.new_mode_kind()?;
        Some(if self.is_executor() {
            constants::TUNE_EXEC_VISIBLE_TOKENS
        } else if matches!(
            self,
            Self::BassAckwardsPrdMerge
                | Self::BassAckwardsPrdRevise
                | Self::BassAckwardsPrdFinalize
                | Self::BassAckwardsPrdApply
        ) {
            constants::PRD_VISIBLE_TOKENS
        } else if self.new_mode_judge()
            || matches!(
                self,
                Self::ForgeDraft
                    | Self::ForgeFuse
                    | Self::DecideScore
                    | Self::TraceExtract
                    | Self::RankCompare
                    | Self::TuneMutate
                    | Self::BassAckwardsPrdTree
            )
        {
            constants::PLAN_VISIBLE_TOKENS
        } else {
            constants::REPLY_VISIBLE_TOKENS
        })
    }
}
