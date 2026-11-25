//! Survey's frozen sources, ballots and human-only claim map.
use crate::{
    validation::{ValidationResult, ensure},
    *,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurveyStatus {
    Preflight,
    WaitingForParticipants,
    Collecting,
    Extracting,
    Voting,
    Selecting,
    Revoting,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl SurveyStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveySetup {
    pub contest_id: ContestId,
    pub question_message_id: MessageId,
    pub question: Text,
    pub participants: Vec<MemberNick>,
    pub answers: Vec<ContestAnswer>,
    pub collect: bool,
}
impl SurveySetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=12).contains(&self.participants.len())
                && self.participants.iter().collect::<BTreeSet<_>>().len()
                    == self.participants.len(),
            "invalid survey participants",
        )?;
        ensure(
            self.answers
                .iter()
                .all(|a| self.participants.contains(&a.nick) && !a.text.trim().is_empty())
                && self
                    .answers
                    .iter()
                    .map(|a| &a.nick)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.answers.len()
                && self
                    .answers
                    .iter()
                    .map(|a| a.message_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.answers.len(),
            "invalid frozen answers",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyState {
    pub contest_id: ContestId,
    pub state: SurveyStatus,
    pub state_version: u64,
    pub step_id: Option<StepId>,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness: Option<SurveyCompleteness>,
}
impl SurveyState {
    pub fn validate(&self) -> ValidationResult {
        if let Some(c) = &self.completeness {
            c.validate()?;
            ensure(
                self.state == SurveyStatus::Complete,
                "survey completeness requires a completed report",
            )?;
        }
        ensure(
            self.state_version > 0
                && self
                    .artifact_event_ids
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.artifact_event_ids.len(),
            "invalid survey state references",
        )?;
        ensure(
            self.state.is_terminal() || self.terminal_reason.is_none(),
            "nonterminal survey has terminal reason",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyPreflight {
    pub base: ContestPreflight,
    pub source: SurveySetup,
    pub reserved_attempts: u32,
}
impl SurveyPreflight {
    pub fn counts(members: usize, collect: bool) -> (u32, u32) {
        let m = members as u32;
        let q = u32::from(collect);
        (q * m + 2 * m + 2, 2 * q * m + 4 * m + 6)
    }
    pub fn validate(&self) -> ValidationResult {
        self.source.validate()?;
        self.base.estimate.validate()?;
        let (calls, attempts) = Self::counts(self.source.participants.len(), self.source.collect);
        ensure(
            self.base.kind == ContestKind::Survey
                && self.base.contest_id == self.source.contest_id
                && self.base.participants == self.source.participants
                && self.base.estimate.model_calls == calls
                && self.reserved_attempts == attempts,
            "invalid survey preflight",
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurveyTransition {
    ParticipantsReady,
    CollectionLocked,
    ClaimsLocked,
    VotesLocked,
    SelectionLocked,
    RevotesLocked,
    ReportLocked,
    Stop,
    Fail,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyClaim {
    pub id: String,
    pub claim: String,
    pub source_answer_ids: Vec<MessageId>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyExtraction {
    pub claims: Vec<SurveyClaim>,
}
impl SurveyExtraction {
    pub fn validate(&self, answers: &[ContestAnswer]) -> ValidationResult {
        ensure(self.claims.len() <= 25, "too many survey claims")?;
        let mut ids = BTreeSet::new();
        let mut texts = BTreeSet::new();
        for c in &self.claims {
            ensure(
                !c.id.trim().is_empty()
                    && ids.insert(&c.id)
                    && !c.claim.trim().is_empty()
                    && texts.insert(
                        c.claim
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .to_lowercase(),
                    ),
                "duplicate or empty survey claim",
            )?;
            ensure(
                !c.source_answer_ids.is_empty()
                    && c.source_answer_ids.iter().collect::<BTreeSet<_>>().len()
                        == c.source_answer_ids.len()
                    && c.source_answer_ids
                        .iter()
                        .all(|id| answers.iter().any(|a| a.message_id == *id)),
                "invalid survey origins",
            )?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurveyChoice {
    Agree,
    Disagree,
    Unsure,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyBallotItem {
    pub claim_id: String,
    pub vote: SurveyChoice,
    pub reason: String,
    #[schemars(range(min = 1, max = 5))]
    pub confidence: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_evidence: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyBallot {
    pub votes: Vec<SurveyBallotItem>,
}
impl SurveyBallot {
    pub fn validate(&self, ids: &[String], second: bool) -> ValidationResult {
        ensure(
            self.votes.len() == ids.len()
                && self
                    .votes
                    .iter()
                    .map(|v| &v.claim_id)
                    .collect::<BTreeSet<_>>()
                    == ids.iter().collect::<BTreeSet<_>>()
                && ids.iter().collect::<BTreeSet<_>>().len() == ids.len(),
            "survey ballot coverage mismatch",
        )?;
        for v in &self.votes {
            ensure(
                (1..=5).contains(&v.confidence)
                    && !v.reason.trim().is_empty()
                    && !v.reason.contains(['\n', '\r']),
                "invalid survey reason or confidence",
            )?;
            ensure(
                if second {
                    v.change_evidence
                        .as_ref()
                        .is_some_and(|s| !s.trim().is_empty())
                } else {
                    v.change_evidence.is_none()
                },
                "invalid survey change evidence",
            )?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyClaims {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub completed: CompletedGeneration,
    pub extraction: SurveyExtraction,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyVote {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub completed: CompletedGeneration,
    pub voter: MemberNick,
    pub ballot: SurveyBallot,
}
pub type SurveyRevote = SurveyVote;
/// This is the complete voter projection. It cannot serialize provenance or tallies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyAnonymousClaim {
    pub claim_id: String,
    pub claim: String,
    pub agree_reasons: Vec<String>,
    pub disagree_reasons: Vec<String>,
    pub unsure_reasons: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveySelection {
    pub tallies: Vec<SurveyClaimMap>,
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub claims: Vec<SurveyAnonymousClaim>,
}
impl SurveySelection {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.claims.len() <= 5
                && self
                    .claims
                    .iter()
                    .map(|c| &c.claim_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == self.claims.len(),
            "invalid survey selection",
        )?;
        ensure(
            self.tallies
                .iter()
                .map(|t| &t.claim_id)
                .collect::<BTreeSet<_>>()
                .len()
                == self.tallies.len(),
            "duplicate survey tallies",
        )?;
        for c in &self.claims {
            ensure(
                !c.claim_id.trim().is_empty()
                    && !c.claim.trim().is_empty()
                    && self.tallies.iter().any(|t| t.claim_id == c.claim_id),
                "selection references unknown claim",
            )?;
            for reasons in [&c.agree_reasons, &c.disagree_reasons, &c.unsure_reasons] {
                ensure(
                    reasons.iter().all(|r| !r.trim().is_empty())
                        && reasons.iter().collect::<BTreeSet<_>>().len() == reasons.len(),
                    "duplicate or empty anonymous reason",
                )?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyTally {
    pub agree: u32,
    pub disagree: u32,
    pub unsure: u32,
    pub missing: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveySwitch {
    pub voter: MemberNick,
    pub from: SurveyChoice,
    pub to: SurveyChoice,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyClaimMap {
    pub claim_id: String,
    pub first: SurveyTally,
    pub final_tally: SurveyTally,
    pub not_revoted: Vec<MemberNick>,
    pub switches: Vec<SurveySwitch>,
    pub bands: Vec<SurveyBand>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SurveyBand {
    Consensus,
    Contested,
    LoneInsights,
    LoneDissents,
    RejectedOutliers,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyReportEntry {
    pub claim_id: String,
    pub explanation: String,
    pub settling_question: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyReportText {
    pub consensus: Vec<SurveyReportEntry>,
    pub contested: Vec<SurveyReportEntry>,
    pub lone_insights: Vec<SurveyReportEntry>,
    pub lone_dissents: Vec<SurveyReportEntry>,
    pub rejected_outliers: Vec<SurveyReportEntry>,
    pub limitations: String,
}
impl SurveyReportText {
    pub fn validate(&self, map: &[SurveyClaimMap]) -> ValidationResult {
        ensure(
            !self.limitations.trim().is_empty(),
            "missing survey limitations",
        )?;
        for (band, entries) in [
            (SurveyBand::Consensus, &self.consensus),
            (SurveyBand::Contested, &self.contested),
            (SurveyBand::LoneInsights, &self.lone_insights),
            (SurveyBand::LoneDissents, &self.lone_dissents),
            (SurveyBand::RejectedOutliers, &self.rejected_outliers),
        ] {
            let expected = map
                .iter()
                .filter(|m| m.bands.contains(&band))
                .map(|m| &m.claim_id)
                .collect::<BTreeSet<_>>();
            ensure(
                entries.len() == expected.len()
                    && entries.iter().map(|e| &e.claim_id).collect::<BTreeSet<_>>() == expected,
                "survey report band mismatch",
            )?;
            for e in entries {
                ensure(
                    !e.explanation.trim().is_empty()
                        && (band != SurveyBand::Contested
                            || !e.settling_question.trim().is_empty()),
                    "missing survey explanation or settling question",
                )?;
            }
        }
        Ok(())
    }
}
/// Application-owned participation summary; never generated by the report judge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyCompleteness {
    pub complete: bool,
    pub missing_answers: Vec<MemberNick>,
    pub missing_votes: Vec<MemberNick>,
    pub missing_revotes: Vec<MemberNick>,
}
impl SurveyCompleteness {
    /// Derive from authorized report inputs, including inputs frozen before the
    /// completeness field was added. No model-generated status is trusted.
    pub fn from_report_inputs(
        setup: &SurveySetup,
        claims: &SurveyExtraction,
        first: &[SurveyVote],
        map: &[SurveyClaimMap],
    ) -> Self {
        let mut result = Self {
            complete: false,
            missing_answers: setup
                .participants
                .iter()
                .filter(|n| !setup.answers.iter().any(|a| &a.nick == *n))
                .cloned()
                .collect(),
            missing_votes: setup
                .participants
                .iter()
                .filter(|n| {
                    claims.claims.iter().any(|c| {
                        !first.iter().any(|v| {
                            &v.voter == *n && v.ballot.votes.iter().any(|v| v.claim_id == c.id)
                        })
                    })
                })
                .cloned()
                .collect(),
            missing_revotes: setup
                .participants
                .iter()
                .filter(|n| map.iter().any(|m| m.not_revoted.contains(n)))
                .cloned()
                .collect(),
        };
        result.complete = result.is_complete();
        result
    }
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.complete == self.is_complete(),
            "inconsistent survey completeness",
        )?;
        for missing in [
            &self.missing_answers,
            &self.missing_votes,
            &self.missing_revotes,
        ] {
            ensure(
                missing.len() <= 12
                    && missing.iter().collect::<BTreeSet<_>>().len() == missing.len(),
                "invalid missing survey participants",
            )?;
        }
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.missing_answers.is_empty()
            && self.missing_votes.is_empty()
            && self.missing_revotes.is_empty()
    }
    pub fn summary(&self) -> String {
        let names = |nicks: &[MemberNick]| {
            nicks
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        if self.is_complete() {
            return "COMPLETE · all required survey participation accepted".into();
        }
        let mut parts = vec![];
        for (stage, missing) in [
            ("answers", &self.missing_answers),
            ("first votes", &self.missing_votes),
            (
                "revotes (first ballot retained when available)",
                &self.missing_revotes,
            ),
        ] {
            if !missing.is_empty() {
                parts.push(format!("missing {stage}: {}", names(missing)));
            }
        }
        format!("DEGRADED · {}", parts.join("; "))
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurveyReport {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub completed: CompletedGeneration,
    pub map: Vec<SurveyClaimMap>,
    pub report: SurveyReportText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completeness: Option<SurveyCompleteness>,
}
impl SurveyReport {
    /// Historical reports predate the participation summary; their map still
    /// identifies missing ballots and retained first ballots.
    pub fn participation_summary(&self) -> Option<String> {
        if let Some(c) = &self.completeness {
            return Some(c.summary());
        }
        self.map
            .iter()
            .any(|m| m.first.missing > 0 || m.final_tally.missing > 0 || !m.not_revoted.is_empty())
            .then(|| "DEGRADED · missing ballots or revotes; see claim participation below".into())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "stage", rename_all = "camelCase", deny_unknown_fields)]
pub enum SurveyCaseMaterial {
    Extraction {
        setup: SurveySetup,
    },
    Report {
        setup: SurveySetup,
        claims: SurveyExtraction,
        first: Vec<SurveyVote>,
        second: Vec<SurveyRevote>,
        map: Vec<SurveyClaimMap>,
    },
}
