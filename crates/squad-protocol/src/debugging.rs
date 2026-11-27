//! debug contracts; enabled only after the mode implementation lands.
use crate::validation::{self, ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = true;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DebuggingStatus {
    Preflight,
    WaitingForParticipants,
    Hypothesizing,
    Merging,
    ProposingTests,
    Selecting,
    AwaitingHuman,
    Eliminating,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl DebuggingStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: DebuggingStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl DebuggingState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid debug state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round <= 6, "invalid debug round")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingSetup {
    pub brief: String,
    pub limits: String,
    pub participants: Vec<MemberNick>,
    pub rounds: u8,
    pub room_words: u32,
    pub plan_words: u32,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompt_refs: std::collections::BTreeMap<String, PromptRef>,
}
impl DebuggingSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.brief.trim().is_empty()
                && (1..=6).contains(&self.rounds)
                && (3..=12).contains(&self.participants.len())
                && unique(&self.participants),
            "invalid debug setup",
        )?;
        ensure(
            self.room_words > 0
                && self.plan_words > 0
                && self.models.contains_key("judge")
                && self
                    .participants
                    .iter()
                    .all(|n| self.models.contains_key(n.as_str()))
                && [
                    "debug.hypothesize",
                    "debug.merge",
                    "debug.test",
                    "debug.select",
                    "debug.report",
                ]
                .iter()
                .all(|n| self.prompt_refs.contains_key(*n)),
            "missing frozen debug input",
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DebuggingPrior {
    High,
    Medium,
    Low,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DebuggingEffort {
    Minutes,
    Hour,
    Day,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DebuggingSafety {
    ReadOnly,
    Reversible,
    Risky,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DebuggingHypothesisStatus {
    Live,
    Eliminated,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingHypothesis {
    pub local_id: String,
    pub cause: String,
    #[schemars(length(max = 8))]
    pub supporting: Vec<String>,
    #[schemars(length(max = 8))]
    pub contradicting: Vec<String>,
    pub prior: DebuggingPrior,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingHypotheses {
    #[schemars(length(min = 1, max = 4))]
    pub hypotheses: Vec<DebuggingHypothesis>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingMergedHypothesis {
    pub hypothesis_id: String,
    pub cause: String,
    #[schemars(length(min = 1, max = 48))]
    pub sources: Vec<String>,
    #[schemars(length(max = 384))]
    pub supporting: Vec<String>,
    #[schemars(length(max = 384))]
    pub contradicting: Vec<String>,
    #[schemars(length(min = 1, max = 3))]
    pub prior_spread: Vec<DebuggingPrior>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingDifferential {
    #[schemars(length(min = 1, max = 8))]
    pub hypotheses: Vec<DebuggingMergedHypothesis>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingPrediction {
    pub hypothesis_id: String,
    #[schemars(extend("x-required-nullable"=true))]
    pub outcome: Option<String>,
    #[schemars(extend("x-required-nullable"=true))]
    pub cannot_predict_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingTest {
    pub local_id: String,
    pub test: String,
    pub procedure: String,
    pub effort: DebuggingEffort,
    pub safety: DebuggingSafety,
    #[schemars(length(min = 2, max = 4))]
    pub outcomes: Vec<String>,
    #[schemars(length(min = 1, max = 20))]
    pub predictions: Vec<DebuggingPrediction>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingTestProposal {
    #[schemars(length(max = 2))]
    pub tests: Vec<DebuggingTest>,
    #[schemars(extend("x-required-nullable"=true))]
    pub new_hypothesis: Option<DebuggingHypothesis>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingOutcome {
    pub label: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingMergedPrediction {
    pub hypothesis_id: String,
    pub outcome: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingNotSelected {
    pub test_ref: String,
    pub why: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingSelection {
    #[schemars(extend("x-required-nullable"=true))]
    pub test_ref: Option<String>,
    pub instruction: String,
    #[schemars(length(max = 4))]
    pub outcomes: Vec<DebuggingOutcome>,
    #[schemars(length(max = 20))]
    pub predictions: Vec<DebuggingMergedPrediction>,
    #[schemars(length(max = 12))]
    pub added_hypotheses: Vec<DebuggingMergedHypothesis>,
    #[schemars(length(max = 24))]
    pub not_selected: Vec<DebuggingNotSelected>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingResult {
    pub round: u32,
    pub outcome: String,
    pub evidence: String,
    pub unexpected: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingLiveHypothesis {
    pub hypothesis: DebuggingMergedHypothesis,
    pub status: DebuggingHypothesisStatus,
    pub consistent_results: u32,
    #[schemars(extend("x-required-nullable"=true))]
    pub failed_prediction: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingElimination {
    pub round: u32,
    pub hypotheses: Vec<DebuggingLiveHypothesis>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingSummary {
    pub summary: String,
    #[schemars(extend("x-required-nullable"=true))]
    pub next_test: Option<String>,
    #[schemars(extend("x-required-nullable"=true))]
    pub patch: Option<String>,
    pub patch_rationale: String,
    pub risks: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingQuoteCheck {
    pub quote: String,
    pub unanchored: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingRound {
    pub round: u32,
    pub proposals: std::collections::BTreeMap<String, DebuggingTestProposal>,
    #[schemars(extend("x-required-nullable"=true))]
    pub selection: Option<DebuggingSelection>,
    #[schemars(extend("x-required-nullable"=true))]
    pub result: Option<DebuggingResult>,
    #[schemars(extend("x-required-nullable"=true))]
    pub elimination: Option<DebuggingElimination>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingReport {
    pub brief: String,
    pub limits: String,
    pub differential: Vec<DebuggingLiveHypothesis>,
    pub tables: std::collections::BTreeMap<String, DebuggingHypotheses>,
    pub provenance: std::collections::BTreeMap<String, MemberNick>,
    pub rounds: Vec<DebuggingRound>,
    pub quotes: Vec<DebuggingQuoteCheck>,
    pub skipped: Vec<String>,
    pub raw_answers: Vec<String>,
    pub reason: String,
    pub patch_allowed: bool,
    #[schemars(extend("x-required-nullable"=true))]
    pub narrative: Option<DebuggingSummary>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum DebuggingTransition {
    #[serde(rename = "debug")]
    Advance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", deny_unknown_fields)]
pub enum DebuggingCaseMaterial {
    #[serde(rename = "debug.merge", rename_all = "camelCase")]
    Merge {
        brief: String,
        limits: String,
        tables: std::collections::BTreeMap<String, DebuggingHypotheses>,
        quote_checks: Vec<DebuggingQuoteCheck>,
    },
    #[serde(rename = "debug.select", rename_all = "camelCase")]
    Select {
        brief: String,
        limits: String,
        differential: Vec<DebuggingLiveHypothesis>,
        ledger: Vec<DebuggingLedgerEntry>,
        proposals: std::collections::BTreeMap<String, DebuggingTestProposal>,
        allow_new_hypothesis: bool,
        quote_checks: Vec<DebuggingQuoteCheck>,
    },
    #[serde(rename = "debug.report", rename_all = "camelCase")]
    Report {
        brief: String,
        limits: String,
        differential: Vec<DebuggingLiveHypothesis>,
        ledger: Vec<DebuggingLedgerEntry>,
        patch_allowed: bool,
        quote_checks: Vec<DebuggingQuoteCheck>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DebuggingLedgerEntry {
    pub round: u32,
    pub test: String,
    pub outcome: String,
    pub evidence: String,
}

fn ids<'a>(v: impl Iterator<Item = &'a String>) -> bool {
    let v = v.collect::<Vec<_>>();
    unique(&v) && v.iter().all(|s| !s.trim().is_empty())
}
impl DebuggingHypotheses {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=4).contains(&self.hypotheses.len())
                && ids(self.hypotheses.iter().map(|h| &h.local_id))
                && self.hypotheses.iter().all(|h| !h.cause.trim().is_empty()),
            "invalid hypothesis table",
        )
    }
}
impl DebuggingDifferential {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=8).contains(&self.hypotheses.len())
                && ids(self.hypotheses.iter().map(|h| &h.hypothesis_id)),
            "invalid differential",
        )?;
        for h in &self.hypotheses {
            ensure(
                !h.cause.trim().is_empty()
                    && !h.sources.is_empty()
                    && ids(h.sources.iter())
                    && !h.prior_spread.is_empty()
                    && unique(&h.prior_spread),
                "invalid merged hypothesis",
            )?;
        }
        Ok(())
    }
}
impl DebuggingTestProposal {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.tests.len() <= 2 && ids(self.tests.iter().map(|t| &t.local_id)),
            "invalid test ids",
        )?;
        if let Some(h) = &self.new_hypothesis {
            DebuggingHypotheses {
                hypotheses: vec![h.clone()],
            }
            .validate()?;
        }
        for t in &self.tests {
            ensure(
                !t.test.trim().is_empty()
                    && !t.procedure.trim().is_empty()
                    && (2..=4).contains(&t.outcomes.len())
                    && ids(t.outcomes.iter())
                    && ids(t.predictions.iter().map(|p| &p.hypothesis_id)),
                "invalid proposed test",
            )?;
            for p in &t.predictions {
                ensure(
                    p.outcome.is_some() != p.cannot_predict_reason.is_some(),
                    "exactly one prediction field is required",
                )?;
                ensure(
                    p.outcome.as_ref().is_none_or(|o| t.outcomes.contains(o))
                        && p.cannot_predict_reason
                            .as_ref()
                            .is_none_or(|r| !r.trim().is_empty()),
                    "invalid prediction",
                )?;
            }
        }
        Ok(())
    }
}
impl DebuggingSelection {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            ids(self.outcomes.iter().map(|o| &o.label))
                && ids(self.predictions.iter().map(|p| &p.hypothesis_id))
                && ids(self.not_selected.iter().map(|p| &p.test_ref)),
            "duplicate selection reference",
        )?;
        if self.test_ref.is_some() {
            ensure(
                !self.instruction.trim().is_empty()
                    && (2..=4).contains(&self.outcomes.len())
                    && self.outcomes.iter().enumerate().all(|(i, o)| {
                        o.label == format!("O{}", i + 1) && !o.description.trim().is_empty()
                    }),
                "invalid outcome labels",
            )?;
        } else {
            ensure(
                self.predictions.is_empty()
                    && self.outcomes.is_empty()
                    && self.added_hypotheses.is_empty(),
                "null test has predictions",
            )?;
        }
        Ok(())
    }
}
impl DebuggingSummary {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.summary.trim().is_empty() && !self.patch_rationale.trim().is_empty(),
            "empty report",
        )
    }
}
pub fn composition(kind: &WorkKind) -> Option<&'static str> {
    Some(match kind {
        WorkKind::DebuggingHypothesize => "debug.hypothesize",
        WorkKind::DebuggingMerge => "debug.merge",
        WorkKind::DebuggingTest => "debug.test",
        WorkKind::DebuggingSelect => "debug.select",
        WorkKind::DebuggingReport => "debug.report",
        _ => return None,
    })
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::DebuggingHypothesize => validation::schema::<DebuggingHypotheses>(),
        WorkKind::DebuggingMerge => validation::schema::<DebuggingDifferential>(),
        WorkKind::DebuggingTest => validation::schema::<DebuggingTestProposal>(),
        WorkKind::DebuggingSelect => validation::schema::<DebuggingSelection>(),
        WorkKind::DebuggingReport => validation::schema::<DebuggingSummary>(),
        _ => return None,
    })
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "debug.hypothesize" => validation::decode::<DebuggingHypotheses>(body)?.validate(),
        "debug.merge" => validation::decode::<DebuggingDifferential>(body)?.validate(),
        "debug.test" => validation::decode::<DebuggingTestProposal>(body)?.validate(),
        "debug.select" => validation::decode::<DebuggingSelection>(body)?.validate(),
        "debug.result" => validation::decode::<DebuggingResult>(body).map(|_| ()),
        "debug.elimination" => validation::decode::<DebuggingElimination>(body).map(|_| ()),
        "debug.report" if body.get("brief").is_some() => {
            validation::decode::<DebuggingReport>(body).map(|_| ())
        }
        "debug.report" => validation::decode::<DebuggingSummary>(body)?.validate(),
        _ => ensure(false, "unknown debug artifact"),
    }
}
/// Count prose, excluding identifiers, enums, and router metadata.
pub fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::String(s) => s.split_whitespace().count(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::Object(o) => o
            .iter()
            .filter(|(k, _)| {
                !matches!(
                    k.as_str(),
                    "localId"
                        | "hypothesisId"
                        | "sources"
                        | "prior"
                        | "priorSpread"
                        | "effort"
                        | "safety"
                        | "testRef"
                        | "label"
                        | "outcome"
                )
            })
            .map(|(_, v)| prose_words(v))
            .sum(),
        _ => 0,
    }
}
