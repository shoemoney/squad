//! forge contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
// Kept disabled until the integrator fixes the shared outer judge dispatch gate.
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForgeStatus {
    Preflight,
    WaitingForParticipants,
    Drafting,
    Ranking,
    Fusing,
    Judging,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl ForgeStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: ForgeStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl ForgeState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid forge state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid forge round")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ForgeTransition {
    Advance,
    Stop,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeSetup {
    pub pack_version: String,
    pub brief: String,
    pub constraints: String,
    pub participants: Vec<MemberNick>,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompts: serde_json::Value,
    pub redaction_tokens: Vec<String>,
    pub room_words: u32,
    pub plan_words: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeDraft {
    pub body: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeNote {
    pub label: String,
    pub steal: String,
    #[schemars(extend("x-required-nullable"=true))]
    pub fatal_flaw: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeBallot {
    #[schemars(length(min = 2, max = 11))]
    pub ranking: Vec<String>,
    #[schemars(length(min = 2, max = 11))]
    pub notes: Vec<ForgeNote>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeProvenance {
    pub element: String,
    #[schemars(length(min = 1, max = 12))]
    pub source_labels: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeHandledFlaw {
    pub label: String,
    pub flaw: String,
    pub handling: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeFusion {
    pub body: String,
    #[schemars(length(max = 24))]
    pub provenance: Vec<ForgeProvenance>,
    #[schemars(length(max = 24))]
    pub handled_flaws: Vec<ForgeHandledFlaw>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeReason {
    pub label: String,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeSeam {
    pub label: String,
    pub issue: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeVerdict {
    #[schemars(length(min = 2, max = 3))]
    pub ranking: Vec<String>,
    #[schemars(length(min = 2, max = 3))]
    pub reasons: Vec<ForgeReason>,
    #[schemars(length(max = 12))]
    pub seam_issues: Vec<ForgeSeam>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeScore {
    pub author: MemberNick,
    pub score: f64,
    pub first_places: u32,
    pub flaw_voters: u32,
    pub tie_key: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeTally {
    pub scores: Vec<ForgeScore>,
    pub best_original: MemberNick,
    pub fusers: Vec<MemberNick>,
    pub max_words: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgeReport {
    pub winner: MemberNick,
    pub reason: String,
    pub body: String,
    pub tally: ForgeTally,
    pub provenance: Vec<ForgeProvenance>,
    pub sources: std::collections::BTreeMap<String, MemberNick>,
    pub handled_flaws: Vec<ForgeHandledFlaw>,
    pub notes: Vec<ForgeNote>,
    pub verdict: Option<ForgeVerdict>,
    pub skipped: Vec<String>,
    pub ballot_maps: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeCaseMaterial {
    pub brief: String,
    pub constraints: String,
    pub candidates: std::collections::BTreeMap<String, String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgeLabelMap {
    pub scope: String,
    pub labels: std::collections::BTreeMap<String, MemberNick>,
}
pub fn words(s: &str) -> usize {
    s.split_whitespace().count()
}
fn labels(actual: &[String], supplied: &[String]) -> ValidationResult {
    ensure(
        unique(actual)
            && actual.len() == supplied.len()
            && actual.iter().all(|l| supplied.contains(l)),
        "ranking must cover supplied labels exactly once",
    )
}
impl ForgeBallot {
    pub fn validate_against(&self, supplied: &[String]) -> ValidationResult {
        labels(&self.ranking, supplied)?;
        labels(
            &self
                .notes
                .iter()
                .map(|n| n.label.clone())
                .collect::<Vec<_>>(),
            supplied,
        )?;
        ensure(
            self.notes.iter().all(|n| {
                !n.steal.trim().is_empty()
                    && words(&n.steal) <= 40
                    && n.fatal_flaw
                        .as_ref()
                        .is_none_or(|f| !f.trim().is_empty() && words(f) <= 40)
            }),
            "note exceeds 40 words or is empty",
        )
    }
}
impl ForgeFusion {
    pub fn validate_against(&self, supplied: &[String], max: usize) -> ValidationResult {
        ensure(
            !self.body.trim().is_empty() && words(&self.body) <= max,
            "fusion body exceeds Lmax or is empty",
        )?;
        ensure(
            self.provenance.iter().all(|p| {
                !p.element.trim().is_empty()
                    && !p.source_labels.is_empty()
                    && unique(&p.source_labels)
                    && p.source_labels.iter().all(|l| supplied.contains(l))
            }) && self.handled_flaws.iter().all(|f| {
                supplied.contains(&f.label)
                    && !f.flaw.trim().is_empty()
                    && !f.handling.trim().is_empty()
            }),
            "unknown fusion source label or empty provenance",
        )
    }
}
impl ForgeVerdict {
    pub fn validate_against(&self, supplied: &[String]) -> ValidationResult {
        labels(&self.ranking, supplied)?;
        labels(
            &self
                .reasons
                .iter()
                .map(|r| r.label.clone())
                .collect::<Vec<_>>(),
            supplied,
        )?;
        ensure(
            self.reasons.iter().all(|r| !r.reason.trim().is_empty())
                && self
                    .seam_issues
                    .iter()
                    .all(|r| supplied.contains(&r.label) && !r.issue.trim().is_empty()),
            "invalid judge reasons or seams",
        )
    }
}
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    body: &serde_json::Value,
) -> ValidationResult {
    let supplied = data["labels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match kind {
        WorkKind::ForgeDraft => {
            let d = validation::decode::<ForgeDraft>(body)?;
            ensure(
                !d.body.trim().is_empty()
                    && words(&d.body) <= data["wordLimit"].as_u64().unwrap_or(1200) as usize,
                "draft word budget exceeded or empty",
            )
        }
        WorkKind::ForgeRank => validation::decode::<ForgeBallot>(body)?.validate_against(&supplied),
        WorkKind::ForgeFuse => {
            let f = validation::decode::<ForgeFusion>(body)?;
            f.validate_against(&supplied, data["maxWords"].as_u64().unwrap_or(0) as usize)?;
            ensure(
                f.handled_flaws.iter().all(|h| {
                    data["flaws"].as_array().is_some_and(|flaws| {
                        flaws.iter().any(|v| {
                            v["label"].as_str() == Some(&h.label)
                                && v["text"].as_str() == Some(&h.flaw)
                        })
                    })
                }),
                "handled flaw must reference a supplied flaw",
            )
        }
        WorkKind::ForgeJudge => {
            validation::decode::<ForgeVerdict>(body)?.validate_against(&supplied)
        }
        _ => ensure(false, "invalid forge work"),
    }?;
    ensure(
        prose_words(body) <= data["wordLimit"].as_u64().unwrap_or(1200) as usize,
        "step prose budget exceeded",
    )
}
pub fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Object(m) => m
            .iter()
            .filter(|(k, _)| !["label", "ranking", "sourceLabels"].contains(&k.as_str()))
            .map(|(_, v)| prose_words(v))
            .sum(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::String(s) => words(s),
        _ => 0,
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "forge.draft" => {
            let d = validation::decode::<ForgeDraft>(body)?;
            ensure(!d.body.trim().is_empty(), "invalid draft")
        }
        "forge.rank" => {
            let b = validation::decode::<ForgeBallot>(body)?;
            b.validate_against(&b.ranking)
        }
        "forge.fuse" => {
            validation::decode::<ForgeFusion>(body)?;
            Ok(())
        }
        "forge.judge" => {
            let v = validation::decode::<ForgeVerdict>(body)?;
            v.validate_against(&v.ranking)
        }
        "forge.labels" => {
            validation::decode::<ForgeLabelMap>(body)?;
            Ok(())
        }
        "forge.tally" => {
            validation::decode::<ForgeTally>(body)?;
            Ok(())
        }
        "forge.report" => {
            validation::decode::<ForgeReport>(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown forge artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::ForgeDraft => validation::schema::<ForgeDraft>(),
        WorkKind::ForgeRank => validation::schema::<ForgeBallot>(),
        WorkKind::ForgeFuse => validation::schema::<ForgeFusion>(),
        WorkKind::ForgeJudge => validation::schema::<ForgeVerdict>(),
        _ => return None,
    })
}
