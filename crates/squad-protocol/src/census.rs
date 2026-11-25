//! Census artifacts. Provider responses cannot supply application-owned tallies.
use crate::{
    validation::{ValidationResult, ensure, unique},
    *,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CENSUS_LENSES: [&str; 6] = [
    "stakeholders",
    "failure modes",
    "dependencies and sequencing",
    "hidden assumptions",
    "external and legal factors",
    "the day after success",
];
pub const CENSUS_RANGE_UNAVAILABLE: &str = "Estimate range unavailable; range rule unresolved.";
pub const CENSUS_CAVEAT: &str =
    "Models share blind spots; the true number still unfound is at least this estimate.";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CensusStatus {
    Preflight,
    WaitingForParticipants,
    Hunting,
    Merging,
    Rating,
    Finalizing,
    WaitingForPlan,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl CensusStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusPlan {
    pub contest_id: ContestId,
    pub version: u32,
    pub text: Text,
    pub source_event_ids: Vec<EventId>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusSetup {
    pub contest_id: ContestId,
    pub plan: CensusPlan,
    pub participants: Vec<MemberNick>,
    pub passes: u8,
    pub room_words: u32,
    pub plan_words: u32,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompt_refs: std::collections::BTreeMap<String, PromptRef>,
}
impl CensusSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.plan.contest_id == self.contest_id && self.plan.version == 1,
            "invalid initial plan",
        )?;
        ensure(
            (3..=12).contains(&self.participants.len()) && unique(&self.participants),
            "census requires at least three distinct members",
        )?;
        ensure(
            (1..=5).contains(&self.passes) && self.room_words > 0 && self.plan_words > 0,
            "invalid census settings",
        )?;
        ensure(
            self.models.contains_key("judge")
                && self
                    .participants
                    .iter()
                    .all(|n| self.models.contains_key(n.as_str())),
            "missing census model",
        )
    }
    pub fn normal_calls(&self) -> u32 {
        u32::from(self.passes) * (2 * self.participants.len() as u32 + 2)
    }
    pub fn reserved_calls(&self) -> u32 {
        u32::from(self.passes) * (4 * self.participants.len() as u32 + 6)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub status: CensusStatus,
    pub pass: u8,
    pub plan_version: u32,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub artifact_event_ids: Vec<EventId>,
    pub unfinished_work_ids: Vec<WorkId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl CensusState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0
                && (1..=5).contains(&self.pass)
                && self.plan_version > 0
                && unique(&self.artifact_event_ids)
                && unique(&self.unfinished_work_ids),
            "invalid census state",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusPreflight {
    pub base: ContestPreflight,
    pub passes: u8,
    pub plan: CensusPlan,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CensusTransition {
    ParticipantsReady,
    HuntBarrier,
    MergeAccepted,
    RatingBarrier,
    FinalAccepted,
    Continue,
    ReportCommitted,
    Stop,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusFind {
    pub local_id: Text,
    pub omission: Text,
    pub consequence: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusHuntOutput {
    #[schemars(length(max = 8))]
    pub gaps: Vec<CensusFind>,
}
impl CensusHuntOutput {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.gaps.len() <= 8
                && unique(&self.gaps.iter().map(|g| &g.local_id).collect::<Vec<_>>()),
            "invalid hunt local IDs",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusHunt {
    pub contest_id: ContestId,
    pub pass: u8,
    pub plan_version: u32,
    pub work_id: WorkId,
    pub member: MemberNick,
    pub lens: Text,
    pub output: CensusHuntOutput,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusGap {
    pub id: Text,
    pub omission: Text,
    pub consequence: Text,
    pub lenses: Vec<Text>,
    pub source_find_ids: Vec<Text>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusMergeOutput {
    pub ledger: Vec<CensusGap>,
}
impl CensusMergeOutput {
    pub fn validate_against(&self, source_ids: &[String]) -> ValidationResult {
        let refs = self
            .ledger
            .iter()
            .flat_map(|g| g.source_find_ids.iter().map(Text::as_str))
            .collect::<Vec<_>>();
        ensure(
            unique(&self.ledger.iter().map(|g| &g.id).collect::<Vec<_>>())
                && self
                    .ledger
                    .iter()
                    .all(|g| !g.source_find_ids.is_empty() && !g.lenses.is_empty())
                && unique(&refs)
                && refs.len() == source_ids.len()
                && refs.iter().all(|r| source_ids.iter().any(|id| id == r)),
            "merge must preserve every source exactly once",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusLedger {
    pub contest_id: ContestId,
    pub pass: u8,
    pub plan_version: u32,
    pub work_id: WorkId,
    pub output: CensusMergeOutput,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CensusVote {
    Real,
    Covered,
    Trivial,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusCitation {
    pub location: Text,
    pub quote: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusGapRating {
    pub gap_id: Text,
    pub rating: CensusVote,
    pub reason: Text,
    #[schemars(extend("x-required-nullable" = true))]
    pub citation: Option<CensusCitation>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusAddition {
    pub local_id: Text,
    pub omission: Text,
    pub consequence: Text,
    pub lens: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusRatingOutput {
    pub ratings: Vec<CensusGapRating>,
    #[schemars(length(max = 8))]
    pub additions: Vec<CensusAddition>,
}
impl CensusRatingOutput {
    pub fn validate_against(&self, ledger: &[CensusGap], plan: &str) -> ValidationResult {
        ensure(
            self.ratings.len() == ledger.len()
                && unique(&self.ratings.iter().map(|r| &r.gap_id).collect::<Vec<_>>())
                && self
                    .ratings
                    .iter()
                    .all(|r| ledger.iter().any(|g| g.id == r.gap_id)),
            "rating must cover assigned ledger exactly",
        )?;
        ensure(
            self.additions.len() <= 8
                && unique(
                    &self
                        .additions
                        .iter()
                        .map(|a| &a.local_id)
                        .collect::<Vec<_>>(),
                ),
            "invalid additions",
        )?;
        ensure(
            self.ratings.iter().all(|r| match (&r.rating, &r.citation) {
                (CensusVote::Covered, Some(c)) => plan.contains(c.quote.as_str()),
                (CensusVote::Covered, None) => false,
                (_, None) => true,
                _ => false,
            }),
            "covered requires an exact plan citation",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusRating {
    pub contest_id: ContestId,
    pub pass: u8,
    pub plan_version: u32,
    pub work_id: WorkId,
    pub member: MemberNick,
    pub output: CensusRatingOutput,
}
/// Final output refers to canonical gaps; authoritative votes and finders stay app-owned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusFinalGap {
    pub gap_id: Text,
    pub priority: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusFinalAddition {
    pub id: Text,
    pub omission: Text,
    pub consequence: Text,
    pub source_addition_ids: Vec<Text>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusFinalOutput {
    pub confirmed: Vec<CensusFinalGap>,
    pub covered: Vec<Text>,
    pub trivial: Vec<Text>,
    pub additions: Vec<CensusFinalAddition>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusGapOutcome {
    pub gap: CensusGap,
    pub disposition: CensusDisposition,
    pub votes: u32,
    pub covered: u32,
    pub trivial: u32,
    pub finders: Vec<MemberNick>,
    pub citations: Vec<CensusCitation>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CensusDisposition {
    Confirmed,
    Covered,
    Trivial,
    Unresolved,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusMissing {
    pub member: MemberNick,
    pub step: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusDiscovery {
    pub pass: u8,
    pub plan_version: u32,
    pub new_confirmed: u32,
    pub cumulative_confirmed: u32,
    pub singletons: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusReport {
    pub contest_id: ContestId,
    pub pass: u8,
    pub plan_version: u32,
    pub gaps: Vec<CensusGapOutcome>,
    pub additions: Vec<CensusFinalAddition>,
    pub missing: Vec<CensusMissing>,
    pub input_gap_ids: Vec<Text>,
    pub f1: u32,
    pub f2: u32,
    pub formula_branch: Text,
    pub unseen: f64,
    #[schemars(extend("x-required-nullable" = true))]
    pub estimate_low: Option<f64>,
    #[schemars(extend("x-required-nullable" = true))]
    pub estimate_high: Option<f64>,
    pub discovery: Vec<CensusDiscovery>,
}
impl CensusReport {
    pub fn validate(&self) -> ValidationResult {
        let inputs = self
            .gaps
            .iter()
            .filter(|g| g.disposition == CensusDisposition::Confirmed && !g.finders.is_empty())
            .collect::<Vec<_>>();
        let f1 = inputs.iter().filter(|g| g.finders.len() == 1).count() as u32;
        let f2 = inputs.iter().filter(|g| g.finders.len() == 2).count() as u32;
        let (value, branch) = census_chao1(f1, f2);
        ensure(
            self.f1 == f1
                && self.f2 == f2
                && self.unseen == value
                && self.formula_branch.as_str() == branch
                && self.input_gap_ids
                    == inputs.iter().map(|g| g.gap.id.clone()).collect::<Vec<_>>()
                && self.gaps.iter().all(|g| unique(&g.finders)),
            "invalid authoritative census tally",
        )?;
        ensure(
            self.estimate_low.is_none() && self.estimate_high.is_none(),
            "estimate range rule is unresolved",
        )
    }
    pub fn quiet(&self) -> bool {
        false
    }
}
pub fn census_chao1(f1: u32, f2: u32) -> (f64, &'static str) {
    if f2 > 0 {
        (f64::from(f1).powi(2) / (2.0 * f64::from(f2)), "doubletons")
    } else {
        (
            f64::from(f1) * f64::from(f1.saturating_sub(1)) / 2.0,
            "zero_doubletons",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusCaseMaterial {
    pub contest_id: ContestId,
    pub pass: u8,
    pub plan_version: u32,
    pub step: Text,
    pub data: serde_json::Value,
}

pub fn census_validate_final(
    v: &CensusFinalOutput,
    data: &serde_json::Value,
) -> validation::ValidationResult {
    use validation::{ensure, unique};
    let expected: Vec<CensusGapOutcome> = serde_json::from_value(data["outcomes"].clone())
        .map_err(|_| validation::ValidationError {
            message: "invalid final inputs".into(),
        })?;
    let ids = v
        .confirmed
        .iter()
        .map(|g| &g.gap_id)
        .chain(v.covered.iter())
        .chain(v.trivial.iter())
        .collect::<Vec<_>>();
    ensure(
        ids.len() == expected.len() && unique(&ids),
        "every ledger gap needs one disposition",
    )?;
    for g in &expected {
        let valid = match g.disposition {
            CensusDisposition::Covered => v.covered.contains(&g.gap.id),
            CensusDisposition::Trivial => v.trivial.contains(&g.gap.id),
            CensusDisposition::Confirmed | CensusDisposition::Unresolved => {
                v.confirmed.iter().any(|r| r.gap_id == g.gap.id)
            }
        };
        ensure(valid, "judge changed application majority")?;
    }
    let sources: Vec<String> =
        serde_json::from_value(data["additionIds"].clone()).map_err(|_| {
            validation::ValidationError {
                message: "invalid addition inputs".into(),
            }
        })?;
    let refs = v
        .additions
        .iter()
        .flat_map(|a| a.source_addition_ids.iter().map(Text::as_str))
        .collect::<Vec<_>>();
    ensure(
        unique(&v.additions.iter().map(|a| &a.id).collect::<Vec<_>>())
            && unique(&refs)
            && refs.len() == sources.len()
            && refs.iter().all(|r| sources.iter().any(|s| s == r))
            && v.additions
                .iter()
                .all(|a| !a.source_addition_ids.is_empty()),
        "invalid addition provenance",
    )
}
