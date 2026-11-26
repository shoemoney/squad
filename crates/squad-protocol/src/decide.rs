//! decide contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
// Held until the integrator-owned judge dispatcher and D75 failure classifier land.
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecideStatus {
    Preflight,
    WaitingForParticipants,
    ProposingOptions,
    Shortlisting,
    ProposingCriteria,
    Freezing,
    Scoring,
    Computing,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl DecideStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: DecideStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl DecideState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid decide state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid decide round")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum DecideTransition {
    Advance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCaseMaterial {
    pub step: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideSetup {
    pub decision: String,
    pub options: Vec<DecideOption>,
    pub participants: Vec<MemberNick>,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompts: serde_json::Value,
    pub redactions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideOption {
    pub option_id: String,
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideProposedOption {
    pub local_id: String,
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideOptionProposal {
    #[schemars(length(min = 2, max = 2))]
    pub options: Vec<DecideProposedOption>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideSource {
    pub member_label: String,
    pub local_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideMergedOption {
    pub option_id: String,
    pub name: String,
    pub description: String,
    #[schemars(length(min = 1, max = 72))]
    pub sources: Vec<DecideSource>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideShortlist {
    #[schemars(length(min = 2, max = 24))]
    pub options: Vec<DecideMergedOption>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCriterion {
    pub local_id: String,
    pub name: String,
    pub definition: String,
    pub weight: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCriteria {
    #[schemars(length(min = 1, max = 6))]
    pub criteria: Vec<DecideCriterion>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCanonicalCriterion {
    pub criterion_id: String,
    pub name: String,
    pub definition: String,
    #[schemars(length(min = 1, max = 72))]
    pub sources: Vec<DecideSource>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideFreeze {
    #[schemars(length(min = 1, max = 72))]
    pub criteria: Vec<DecideCanonicalCriterion>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideWeightedCriterion {
    pub criterion: DecideCanonicalCriterion,
    pub support: usize,
    pub mean_weight: f64,
    pub weight: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideRubric {
    pub kept: Vec<DecideWeightedCriterion>,
    pub dropped: Vec<DecideWeightedCriterion>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCell {
    pub option_id: String,
    pub criterion_id: String,
    pub score: u8,
    pub evidence: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideScores {
    #[schemars(length(min = 1, max = 28))]
    pub cells: Vec<DecideCell>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideMedian {
    pub option_id: String,
    pub criterion_id: String,
    pub median: f64,
    pub contested: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideTotal {
    pub option_id: String,
    pub total: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideLeaveOut {
    pub criterion_id: String,
    pub ranking: Vec<DecideTotal>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideFlip {
    pub criterion_id: String,
    pub rival_option_id: String,
    pub threshold: f64,
    pub above: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideMatrix {
    pub cells: Vec<DecideMedian>,
    pub ranking: Vec<DecideTotal>,
    pub margin: f64,
    #[schemars(extend("x-required-nullable" = true))]
    pub winner: Option<String>,
    pub leave_one_out: Vec<DecideLeaveOut>,
    pub flips: Vec<DecideFlip>,
    pub robust: bool,
    pub member_support: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideFlipCondition {
    pub criterion_id: String,
    pub rival_option_id: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideCellRef {
    pub option_id: String,
    pub criterion_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideDissent {
    pub option_id: String,
    pub text: String,
    pub cell_refs: Vec<DecideCellRef>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideExplanation {
    #[schemars(length(max = 7))]
    pub flip_conditions: Vec<DecideFlipCondition>,
    #[schemars(length(max = 28))]
    pub dissents: Vec<DecideDissent>,
    #[schemars(length(max = 5))]
    pub evidence_gaps: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideReport {
    pub proposers: std::collections::BTreeMap<String, MemberNick>,
    pub options: Vec<DecideOption>,
    pub rubric: DecideRubric,
    pub matrix: DecideMatrix,
    #[schemars(extend("x-required-nullable" = true))]
    pub explanation: Option<DecideExplanation>,
    pub skipped: Vec<String>,
}

fn words(s: &str) -> usize {
    s.split_whitespace().count()
}
fn ids(values: impl Iterator<Item = String>) -> bool {
    let v: Vec<_> = values.collect();
    !v.is_empty() && v.iter().all(|s| !s.trim().is_empty() && s.len() <= 80) && unique(&v)
}
fn sources(v: &[DecideSource]) -> bool {
    !v.is_empty()
        && ids(v
            .iter()
            .map(|s| format!("{}:{}", s.member_label, s.local_id)))
}
impl DecideOptionProposal {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.options.len() == 2
                && ids(self.options.iter().map(|x| x.local_id.clone()))
                && self.options.iter().all(|x| {
                    !x.name.trim().is_empty()
                        && words(&x.name) <= 8
                        && !x.description.trim().is_empty()
                        && words(&x.description) <= 40
                }),
            "invalid option proposal",
        )
    }
}
impl DecideCriteria {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=6).contains(&self.criteria.len())
                && ids(self.criteria.iter().map(|x| x.local_id.clone()))
                && self
                    .criteria
                    .iter()
                    .map(|x| u64::from(x.weight))
                    .sum::<u64>()
                    == 100
                && self
                    .criteria
                    .iter()
                    .all(|x| !x.name.trim().is_empty() && !x.definition.trim().is_empty()),
            "invalid criteria or weights",
        )
    }
}
impl DecideShortlist {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (2..=24).contains(&self.options.len())
                && ids(self.options.iter().map(|x| x.option_id.clone()))
                && self.options.iter().all(|x| {
                    sources(&x.sources)
                        && !x.name.trim().is_empty()
                        && words(&x.name) <= 8
                        && !x.description.trim().is_empty()
                        && words(&x.description) <= 40
                }),
            "invalid shortlist",
        )
    }
}
impl DecideFreeze {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=72).contains(&self.criteria.len())
                && ids(self.criteria.iter().map(|x| x.criterion_id.clone()))
                && self.criteria.iter().all(|x| {
                    sources(&x.sources)
                        && !x.name.trim().is_empty()
                        && !x.definition.trim().is_empty()
                        && words(&x.name) + words(&x.definition) <= 60
                }),
            "invalid frozen criteria",
        )
    }
}
impl DecideScores {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (1..=28).contains(&self.cells.len())
                && ids(self
                    .cells
                    .iter()
                    .map(|x| format!("{}:{}", x.option_id, x.criterion_id)))
                && self.cells.iter().all(|x| {
                    x.score <= 10
                        && !x.evidence.trim().is_empty()
                        && !x.evidence.contains(['\n', '\r'])
                        && words(&x.evidence) <= 25
                }),
            "invalid score sheet",
        )
    }
    pub fn validate_against(&self, options: &[String], criteria: &[String]) -> ValidationResult {
        self.validate()?;
        ensure(
            self.cells.len() == options.len() * criteria.len()
                && self
                    .cells
                    .iter()
                    .all(|x| options.contains(&x.option_id) && criteria.contains(&x.criterion_id)),
            "missing or unknown score cell",
        )
    }
}
pub fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Object(o) => o
            .iter()
            .filter(|(k, _)| {
                !matches!(
                    k.as_str(),
                    "localId" | "optionId" | "criterionId" | "memberLabel" | "rivalOptionId"
                )
            })
            .map(|(_, v)| prose_words(v))
            .sum(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::String(s) => words(s),
        _ => 0,
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "decide.options" => validation::decode::<DecideOptionProposal>(body)?.validate(),
        "decide.shortlist" => validation::decode::<DecideShortlist>(body)?.validate(),
        "decide.criteria" => validation::decode::<DecideCriteria>(body)?.validate(),
        "decide.freeze" => validation::decode::<DecideFreeze>(body)?.validate(),
        "decide.score" => validation::decode::<DecideScores>(body)?.validate(),
        "decide.explain" => {
            let e = validation::decode::<DecideExplanation>(body)?;
            ensure(
                e.evidence_gaps.len() <= 5
                    && e.evidence_gaps.iter().all(|s| !s.trim().is_empty())
                    && e.flip_conditions.iter().all(|f| !f.text.trim().is_empty())
                    && e.dissents.iter().all(|d| !d.text.trim().is_empty()),
                "invalid explanation prose",
            )
        }
        "decide.matrix" => {
            validation::decode::<DecideMatrix>(body)?;
            Ok(())
        }
        "decide.rubric" => {
            validation::decode::<DecideRubric>(body)?;
            Ok(())
        }
        "decide.report" => {
            validation::decode::<DecideReport>(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown decide artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::DecideOptions => validation::schema::<DecideOptionProposal>(),
        WorkKind::DecideShortlist => validation::schema::<DecideShortlist>(),
        WorkKind::DecideCriteria => validation::schema::<DecideCriteria>(),
        WorkKind::DecideFreeze => validation::schema::<DecideFreeze>(),
        WorkKind::DecideScore => validation::schema::<DecideScores>(),
        WorkKind::DecideExplain => validation::schema::<DecideExplanation>(),
        _ => return None,
    })
}
/// Validate references against the exact authorized projection, never a model's assertions.
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    body: &serde_json::Value,
) -> ValidationResult {
    let name = kind.new_mode_composition().unwrap_or("");
    validate_artifact(name, body)?;
    let invalid = || validation::ValidationError {
        message: "invalid decide step references".into(),
    };
    match kind {
        WorkKind::DecideShortlist | WorkKind::DecideFreeze => {
            let rows = data["proposals"].as_array().ok_or_else(invalid)?;
            let field = if *kind == WorkKind::DecideFreeze {
                "criteria"
            } else {
                "options"
            };
            let mut supplied = std::collections::BTreeSet::new();
            for row in rows {
                for x in row[field].as_array().ok_or_else(invalid)? {
                    supplied.insert((
                        row["memberLabel"].as_str().ok_or_else(invalid)?.to_owned(),
                        x["localId"].as_str().ok_or_else(invalid)?.to_owned(),
                    ));
                }
            }
            let mut used = std::collections::BTreeSet::new();
            for row in body[field].as_array().ok_or_else(invalid)? {
                for src in row["sources"].as_array().ok_or_else(invalid)? {
                    let key = (
                        src["memberLabel"].as_str().ok_or_else(invalid)?.to_owned(),
                        src["localId"].as_str().ok_or_else(invalid)?.to_owned(),
                    );
                    ensure(
                        supplied.contains(&key) && used.insert(key),
                        "unknown or duplicate merge source",
                    )?;
                }
            }
            ensure(
                used == supplied,
                "merge must preserve every proposal source",
            )
        }
        WorkKind::DecideScore => {
            let os = data["options"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|v| v["optionId"].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>();
            let cs = data["criteria"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|v| v["criterionId"].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>();
            validation::decode::<DecideScores>(body)?.validate_against(&os, &cs)
        }
        WorkKind::DecideExplain => {
            let e = validation::decode::<DecideExplanation>(body)?;
            let m: DecideMatrix =
                serde_json::from_value(data["matrix"].clone()).map_err(|_| invalid())?;
            for f in e.flip_conditions {
                ensure(
                    m.flips.iter().any(|x| {
                        x.criterion_id == f.criterion_id && x.rival_option_id == f.rival_option_id
                    }) || m.leave_one_out.iter().any(|x| {
                        x.criterion_id == f.criterion_id
                            && x.ranking
                                .first()
                                .is_some_and(|t| t.option_id == f.rival_option_id)
                            && x.ranking.first().map(|t| &t.option_id)
                                != m.ranking.first().map(|t| &t.option_id)
                    }),
                    "unknown flip condition",
                )?;
            }
            for d in e.dissents {
                ensure(
                    m.ranking.iter().any(|x| x.option_id == d.option_id)
                        && !d.cell_refs.is_empty()
                        && d.cell_refs.iter().all(|r| {
                            m.cells.iter().any(|c| {
                                c.option_id == r.option_id && c.criterion_id == r.criterion_id
                            })
                        }),
                    "unknown dissent cell",
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
