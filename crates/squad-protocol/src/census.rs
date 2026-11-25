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
pub const CENSUS_NOTHING_FOUND: &str = "Nothing found yet.";
pub const CENSUS_QUIET: &str = "Census is quiet. Good place to stop.";
pub const CENSUS_RANGE_NOTE: &str =
    "Plausible range: Chao (1987) log-normal interval around the Chao1 estimate.";
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
    pub redaction_tokens: Vec<String>,
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
    #[schemars(length(max = 96))]
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
pub enum CensusLedgerStage {
    Merged,
    Finalized,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CensusLedger {
    pub stage: CensusLedgerStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_output: Option<CensusFinalOutput>,
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
    #[schemars(length(max = 96))]
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
    #[schemars(length(max = 96))]
    pub confirmed: Vec<CensusFinalGap>,
    #[schemars(length(max = 96))]
    pub covered: Vec<Text>,
    #[schemars(length(max = 96))]
    pub trivial: Vec<Text>,
    #[schemars(length(max = 96))]
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
    pub finder_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Text>,
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
    #[schemars(length(max = 96))]
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
                && self
                    .gaps
                    .iter()
                    .all(|g| unique(&g.finders) && g.finder_count == g.finders.len() as u32),
            "invalid authoritative census tally",
        )?;
        let expected = census_range(f1, f2, self.found());
        ensure(
            self.estimate_low == expected.map(|r| r.0)
                && self.estimate_high == expected.map(|r| r.1),
            "invalid census estimate range",
        )
    }
    /// Confirmed gaps found this pass: confirmed ledger gaps plus rating-stage additions.
    pub fn found(&self) -> u32 {
        self.gaps
            .iter()
            .filter(|g| g.disposition == CensusDisposition::Confirmed)
            .count() as u32
            + self.additions.len() as u32
    }
    /// `Found {n}; estimated {lo} to {hi} still unfound.`, or `Nothing found yet.`
    pub fn headline(&self) -> String {
        match (self.estimate_low, self.estimate_high) {
            (Some(lo), Some(hi)) if self.found() > 0 => format!(
                "Found {}; estimated {lo:.1} to {hi:.1} still unfound.",
                self.found()
            ),
            _ => CENSUS_NOTHING_FOUND.into(),
        }
    }
    /// Quiet only when the upper bound is below 2, the pass found no new confirmed
    /// gap, every required participant finished, and no rating evidence is unresolved.
    pub fn quiet(&self) -> bool {
        let no_new_gap = self.discovery.last().is_some_and(|d| d.new_confirmed == 0);
        self.found() > 0
            && self.estimate_high.is_some_and(|hi| hi < 2.0)
            && no_new_gap
            && self.missing.is_empty()
            && !self
                .gaps
                .iter()
                .any(|g| g.disposition == CensusDisposition::Unresolved)
    }
}
/// Chao (1987) log-normal plausible range around the Chao1 unseen estimate.
/// `None` when nothing is confirmed (`s_obs == 0`); `(0, 0)` when `U == 0`.
pub fn census_range(f1: u32, f2: u32, s_obs: u32) -> Option<(f64, f64)> {
    if s_obs == 0 {
        return None;
    }
    let (u, _) = census_chao1(f1, f2);
    if u <= 0.0 {
        return Some((0.0, 0.0));
    }
    let (a, b) = (f64::from(f1), f64::from(f2));
    let var = if f2 > 0 {
        let r = a / b;
        b * (0.5 * r.powi(2) + r.powi(3) + 0.25 * r.powi(4))
    } else {
        let s_hat = f64::from(s_obs) + u;
        a * (a - 1.0) / 2.0 + a * (2.0 * a - 1.0).powi(2) / 4.0 - a.powi(4) / (4.0 * s_hat)
    }
    .max(0.0);
    let k = (1.96 * (1.0 + var / (u * u)).ln().sqrt()).exp();
    Some((u / k, u * k))
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
    let invalid = || validation::ValidationError {
        message: "invalid final inputs".into(),
    };
    let expected = data["outcomes"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|g| {
            let id = validation::decode::<Text>(&g["gap"]["id"])?;
            let disposition = serde_json::from_value::<CensusDisposition>(g["disposition"].clone())
                .map_err(|_| invalid())?;
            Ok((id, disposition))
        })
        .collect::<Result<Vec<_>, validation::ValidationError>>()?;
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
    for (id, disposition) in &expected {
        let valid = match disposition {
            CensusDisposition::Covered => v.covered.contains(id),
            CensusDisposition::Trivial => v.trivial.contains(id),
            CensusDisposition::Confirmed | CensusDisposition::Unresolved => {
                v.confirmed.iter().any(|r| &r.gap_id == id)
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

/// Only generated prose consumes the step budget, never identifiers or enums.
pub fn census_prose_words(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                if [
                    "omission",
                    "consequence",
                    "reason",
                    "location",
                    "quote",
                    "priority",
                ]
                .contains(&k.as_str())
                {
                    v.as_str().map_or(0, |s| s.split_whitespace().count())
                } else {
                    census_prose_words(v)
                }
            })
            .sum(),
        serde_json::Value::Array(items) => items.iter().map(census_prose_words).sum(),
        _ => 0,
    }
}

impl CensusPreflight {
    pub fn validate(&self) -> validation::ValidationResult {
        self.base.estimate.validate()?;
        ensure(
            self.base.kind == ContestKind::Census
                && (1..=5).contains(&self.passes)
                && (3..=12).contains(&self.base.participants.len())
                && unique(&self.base.participants),
            "invalid census preflight participants or passes",
        )?;
        ensure(
            self.plan.contest_id == self.base.contest_id
                && self.plan.version > 0
                && unique(&self.plan.source_event_ids)
                && self.base.estimate.model_calls
                    == u32::from(self.passes) * (2 * self.base.participants.len() as u32 + 2),
            "invalid census preflight sources or calls",
        )
    }
}
