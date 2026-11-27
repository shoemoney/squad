//! tune contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = true;
pub const MAX_PROPOSED_CASES: usize = 3;
pub const MAX_PROPOSED_CRITERIA: usize = 3;
pub const MIN_FROZEN_CASES: usize = 4;
pub const MAX_FROZEN_CASES: usize = 6;
pub const MAX_HELDOUT_CASES: usize = 3;
pub const MUTATORS_PER_GENERATION: usize = 3;
pub const MAX_FINALISTS: usize = 3;
pub const MAX_ORIGINAL_WORDS: usize = 1200;
pub const MAX_CANDIDATE_WORDS: usize = 1800;
pub const MAX_CASE_WORDS: usize = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TuneStatus {
    Preflight,
    WaitingForParticipants,
    BuildingCases,
    Freezing,
    Executing,
    Scoring,
    Mutating,
    HoldoutExecuting,
    HoldoutScoring,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl TuneStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: TuneStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl TuneState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid tune state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round <= 6, "invalid tune generation")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum TuneTransition {
    Stop,
    Fail,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneSetup {
    pub prompt: String,
    pub task: String,
    pub participants: Vec<MemberNick>,
    pub executor: ExecutorBinding,
    pub generations: u8,
    pub length_budget: usize,
    pub rotation: Vec<MemberNick>,
    pub seed: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneTestCase {
    pub local_id: String,
    pub kind: TuneCaseKind,
    pub input: String,
    pub expect: Vec<String>,
    pub must_not: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TuneCaseKind {
    Typical,
    Edge,
    Adversarial,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneCriterionProposal {
    pub local_id: String,
    pub name: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneCaseProposal {
    pub cases: Vec<TuneTestCase>,
    pub criteria: Vec<TuneCriterionProposal>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneRubricCriterion {
    pub name: String,
    pub description: String,
    pub source_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneDroppedCase {
    pub case_ref: String,
    pub why: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneFreeze {
    pub cases: Vec<String>,
    pub dropped: Vec<TuneDroppedCase>,
    pub rubric: Vec<TuneRubricCriterion>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneFrozenCase {
    pub label: String,
    pub source_ref: String,
    pub case: TuneTestCase,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneSuite {
    pub train: Vec<TuneFrozenCase>,
    pub holdout: Vec<TuneFrozenCase>,
    pub rubric: Vec<TuneRubricCriterion>,
    pub dropped: Vec<TuneDroppedCase>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneChange {
    pub what: String,
    pub why: String,
    pub targets: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneMutation {
    pub prompt: String,
    pub changes: Vec<TuneChange>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneCandidate {
    pub id: String,
    pub parent: Option<String>,
    pub generation: u32,
    pub author: Option<MemberNick>,
    pub prompt: String,
    pub changes: Vec<TuneChange>,
    pub incomplete: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneExecution {
    pub candidate_id: String,
    pub case_label: String,
    pub output: Option<String>,
    pub truncated: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneCriterionScore {
    pub criterion: String,
    pub score: u8,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneOutputScore {
    pub label: String,
    pub scores: Vec<TuneCriterionScore>,
    pub note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneCaseScores {
    pub case_label: String,
    pub outputs: Vec<TuneOutputScore>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneScoreSheet {
    pub cases: Vec<TuneCaseScores>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneDigestCase {
    pub case_label: String,
    pub kind: TuneCaseKind,
    pub scores: Vec<TuneCriterionScore>,
    pub note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneDigest {
    pub cases: Vec<TuneDigestCase>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneGeneration {
    pub generation: u32,
    pub scores: std::collections::BTreeMap<String, f64>,
    pub best: Option<String>,
    pub champion: String,
    pub promoted: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneHoldout {
    pub scores: std::collections::BTreeMap<String, f64>,
    pub finalists: Vec<String>,
    pub winner: String,
    pub dropped: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneReport {
    pub reason: String,
    pub result_line: String,
    pub winner: String,
    pub prompt: String,
    pub diff: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneLabelledProposal {
    pub label: String,
    pub proposal: TuneCaseProposal,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneScoringOutput {
    pub label: String,
    pub output: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuneScoringCase {
    pub case_label: String,
    pub case: TuneTestCase,
    pub outputs: Vec<TuneScoringOutput>,
}
/// Only the material authorized for one fresh invocation crosses the reader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "step",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TuneCaseMaterial {
    Cases {
        prompt: String,
        task: String,
    },
    Freeze {
        prompt: String,
        task: String,
        proposals: Vec<TuneLabelledProposal>,
    },
    Mutate {
        task: String,
        prompt: String,
        digest: TuneDigest,
        length_budget: usize,
        placeholders: usize,
    },
    Score {
        task: String,
        rubric: Vec<TuneRubricCriterion>,
        cases: Vec<TuneScoringCase>,
    },
    Execute {
        prompt: String,
        input: String,
    },
}
impl TuneCaseMaterial {
    pub fn kind(&self) -> WorkKind {
        match self {
            Self::Cases { .. } => WorkKind::TuneCases,
            Self::Freeze { .. } => WorkKind::TuneFreeze,
            Self::Mutate { .. } => WorkKind::TuneMutate,
            Self::Score { .. } => WorkKind::TuneScore,
            Self::Execute { .. } => WorkKind::TuneExecute,
        }
    }
    pub fn validate_output(&self, body: &serde_json::Value) -> ValidationResult {
        validate_artifact(work_name(&self.kind()), body)?;
        match self {
            Self::Mutate {
                prompt,
                length_budget,
                placeholders,
                digest,
                ..
            } => {
                let m: TuneMutation = validation::decode(body)?;
                ensure(
                    words(&m.prompt) <= *length_budget,
                    "mutation exceeds length budget",
                )?;
                ensure(
                    collapsed(&m.prompt) != collapsed(prompt),
                    "mutation equals parent",
                )?;
                ensure(
                    m.prompt.matches("{{input}}").count() == *placeholders,
                    "placeholder count changed",
                )?;
                for c in m.changes {
                    ensure(
                        c.targets
                            .iter()
                            .all(|t| digest.cases.iter().any(|c| &c.case_label == t)),
                        "unknown digest target",
                    )?;
                }
            }
            Self::Freeze { proposals, .. } => {
                let f: TuneFreeze = validation::decode(body)?;
                let mut members = std::collections::BTreeMap::new();
                let case_refs: Vec<_> = proposals
                    .iter()
                    .flat_map(|p| {
                        p.proposal
                            .cases
                            .iter()
                            .map(move |c| format!("{}:{}", p.label, c.local_id))
                    })
                    .collect();
                for r in &f.cases {
                    ensure(case_refs.contains(r), "invented case reference")?;
                    let owner = r.split(':').next().unwrap_or("");
                    *members.entry(owner).or_insert(0) += 1;
                }
                ensure(
                    members.values().all(|n| *n <= 2),
                    "more than two cases from one member",
                )?;
                let dropped: Vec<_> = f.dropped.iter().map(|d| d.case_ref.clone()).collect();
                ensure(
                    unique(&dropped)
                        && dropped
                            .iter()
                            .all(|r| case_refs.contains(r) && !f.cases.contains(r)),
                    "invalid dropped cases",
                )?;
                ensure(
                    f.cases.len() + dropped.len() == case_refs.len(),
                    "every unselected case needs a dropped reason",
                )?;
                let refs: Vec<_> = proposals
                    .iter()
                    .flat_map(|p| {
                        p.proposal
                            .criteria
                            .iter()
                            .map(move |c| format!("{}:{}", p.label, c.local_id))
                    })
                    .collect();
                for c in f.rubric {
                    ensure(
                        c.source_refs.iter().all(|r| refs.contains(r)),
                        "invented criterion reference",
                    )?;
                }
            }
            Self::Score { rubric, cases, .. } => {
                let sheet: TuneScoreSheet = validation::decode(body)?;
                ensure(sheet.cases.len() == cases.len(), "case coverage mismatch")?;
                for case in cases {
                    let row = sheet
                        .cases
                        .iter()
                        .find(|r| r.case_label == case.case_label)
                        .ok_or_else(|| validation::ValidationError {
                            message: "missing case".into(),
                        })?;
                    ensure(
                        row.outputs.len() == case.outputs.len(),
                        "output coverage mismatch",
                    )?;
                    for output in &case.outputs {
                        let row = row
                            .outputs
                            .iter()
                            .find(|r| r.label == output.label)
                            .ok_or_else(|| validation::ValidationError {
                                message: "missing output".into(),
                            })?;
                        ensure(
                            row.scores.len() == rubric.len()
                                && row
                                    .scores
                                    .iter()
                                    .all(|s| rubric.iter().any(|c| c.name == s.criterion)),
                            "criterion coverage mismatch",
                        )?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
pub fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
pub fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn length_budget(prompt: &str) -> usize {
    let w = words(prompt);
    1800.min((3 * w).div_ceil(2).max(w + 150))
}
fn bound(text: &str, max: usize) -> ValidationResult {
    ensure(
        !text.trim().is_empty() && words(text) <= max,
        "text exceeds word limit or is empty",
    )
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "tune.cases" => {
            let p: TuneCaseProposal = validation::decode(body)?;
            ensure(
                (1..=3).contains(&p.cases.len()) && (1..=3).contains(&p.criteria.len()),
                "proposal caps",
            )?;
            ensure(
                unique(&p.cases.iter().map(|c| &c.local_id).collect::<Vec<_>>())
                    && unique(&p.criteria.iter().map(|c| &c.local_id).collect::<Vec<_>>()),
                "duplicate local id",
            )?;
            for c in p.cases {
                bound(&c.local_id, 1)?;
                ensure(!c.local_id.contains(':'), "invalid local id")?;
                bound(&c.input, 150)?;
                ensure(
                    (1..=3).contains(&c.expect.len()) && c.must_not.len() <= 2,
                    "expectation caps",
                )?;
                for t in c.expect.iter().chain(&c.must_not) {
                    bound(t, 25)?;
                }
            }
            for c in p.criteria {
                bound(&c.local_id, 1)?;
                ensure(!c.local_id.contains(':'), "invalid local id")?;
                bound(&c.name, 6)?;
                bound(&c.description, 40)?;
            }
        }
        "tune.freeze" => {
            let f: TuneFreeze = validation::decode(body)?;
            ensure(
                (4..=6).contains(&f.cases.len())
                    && unique(&f.cases)
                    && (2..=4).contains(&f.rubric.len()),
                "freeze caps",
            )?;
            ensure(
                unique(&f.rubric.iter().map(|r| &r.name).collect::<Vec<_>>()),
                "duplicate criterion",
            )?;
            for c in f.rubric {
                bound(&c.name, 6)?;
                bound(&c.description, 40)?;
                ensure(
                    !c.source_refs.is_empty() && unique(&c.source_refs),
                    "criterion needs distinct sources",
                )?;
            }
            for d in f.dropped {
                bound(&d.why, 25)?;
            }
        }
        "tune.mutate" => {
            let m: TuneMutation = validation::decode(body)?;
            bound(&m.prompt, 1800)?;
            ensure((1..=5).contains(&m.changes.len()), "change caps")?;
            ensure(
                m.changes
                    .iter()
                    .map(|c| words(&c.what) + words(&c.why))
                    .sum::<usize>()
                    <= 150,
                "change word budget",
            )?;
            for c in m.changes {
                bound(&c.what, 60)?;
                bound(&c.why, 60)?;
                ensure(unique(&c.targets), "duplicate target")?;
            }
        }
        "tune.score" => {
            let s: TuneScoreSheet = validation::decode(body)?;
            ensure(
                (1..=3).contains(&s.cases.len())
                    && unique(&s.cases.iter().map(|c| &c.case_label).collect::<Vec<_>>()),
                "invalid score cases",
            )?;
            for c in s.cases {
                ensure(
                    (1..=5).contains(&c.outputs.len())
                        && unique(&c.outputs.iter().map(|o| &o.label).collect::<Vec<_>>()),
                    "invalid score outputs",
                )?;
                for o in c.outputs {
                    ensure(
                        (2..=4).contains(&o.scores.len())
                            && unique(&o.scores.iter().map(|s| &s.criterion).collect::<Vec<_>>())
                            && o.scores.iter().all(|s| s.score <= 4),
                        "invalid criterion scores",
                    )?;
                    ensure(
                        words(&o.note) <= 30
                            && (!o.scores.iter().any(|s| s.score <= 2)
                                || !o.note.trim().is_empty()),
                        "note missing or too long",
                    )?;
                }
            }
        }
        "tune.execute" => {
            let e: TuneExecution = validation::decode(body)?;
            ensure(
                e.output.as_ref().is_none_or(|s| !s.trim().is_empty()),
                "empty execution",
            )?;
        }
        "tune.suite" => {
            let _: TuneSuite = validation::decode(body)?;
        }
        "tune.candidate" => {
            let _: TuneCandidate = validation::decode(body)?;
        }
        "tune.digest" => {
            let _: TuneDigest = validation::decode(body)?;
        }
        "tune.generation" => {
            let _: TuneGeneration = validation::decode(body)?;
        }
        "tune.holdout" => {
            let _: TuneHoldout = validation::decode(body)?;
        }
        "tune.report" => {
            let _: TuneReport = validation::decode(body)?;
        }
        _ => return ensure(false, "unknown tune artifact"),
    }
    Ok(())
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::TuneCases => validation::schema::<TuneCaseProposal>(),
        WorkKind::TuneFreeze => validation::schema::<TuneFreeze>(),
        WorkKind::TuneMutate => validation::schema::<TuneMutation>(),
        WorkKind::TuneScore => validation::schema::<TuneScoreSheet>(),
        _ => return None,
    })
}

pub fn work_name(kind: &WorkKind) -> &'static str {
    match kind {
        WorkKind::TuneExecute => "tune.execute",
        _ => kind.new_mode_composition().expect("tune composition"),
    }
}
