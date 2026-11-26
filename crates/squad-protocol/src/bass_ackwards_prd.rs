//! bass_ackwards_prd contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = true;
pub const MAX_DONE_ITEMS: usize = 8;
pub const MAX_FROZEN_DONE: usize = 12;
pub const MAX_TREE_NODES: usize = 20;
pub const MAX_TREE_DEPTH: usize = 4;
pub const MAX_MILESTONES: usize = 8;
pub const MAX_LINE_ITEMS: usize = 60;
pub const MAX_COMMENTS: usize = 4;
pub const MAX_ADDITIONS: usize = 3;
pub const PRD_WORD_LIMIT: usize = 2400;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BassAckwardsPrdStatus {
    Preflight,
    WaitingForParticipants,
    DefiningDone,
    FreezingDone,
    BuildingTrees,
    Merging,
    Reviewing,
    Revising,
    MustHaves,
    Finalizing,
    AwaitingHuman,
    Applying,
    Checking,
    Saving,
    Complete,
    Stopped,
    Failed,
}
impl BassAckwardsPrdStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: BassAckwardsPrdStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl BassAckwardsPrdState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid bass_ackwards_prd state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid bass_ackwards_prd round")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BassAckwardsPrdTransition {
    Advance,
    Stop,
    Fail,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdCaseMaterial {
    pub step: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdSetup {
    pub contest_id: ContestId,
    pub end_result: String,
    #[schemars(extend("x-required-nullable" = true))]
    pub today: Option<String>,
    pub participants: Vec<MemberNick>,
    pub human_review: bool,
    pub rotation: Vec<MemberNick>,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompt_refs: std::collections::BTreeMap<String, PromptRef>,
    pub room_words: u32,
    pub plan_words: u32,
    pub prd_words: u32,
    pub data_dir: String,
    pub redaction_tokens: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDoneItem {
    pub local_id: String,
    pub test: String,
    pub verified_by: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDoneList {
    #[schemars(length(min = 3, max = 8))]
    pub items: Vec<BassAckwardsPrdDoneItem>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdGroup {
    pub text: String,
    pub source_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdGrouping {
    pub groups: Vec<BassAckwardsPrdGroup>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDoneEntry {
    pub id: String,
    pub test: String,
    pub verified_by: String,
    pub support: usize,
    pub source_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDone {
    pub items: Vec<BassAckwardsPrdDoneEntry>,
    pub single_source: Vec<BassAckwardsPrdGroup>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdNode {
    pub node_id: String,
    #[schemars(extend("x-required-nullable" = true))]
    pub parent: Option<String>,
    pub precondition: String,
    pub deliverable: String,
    pub owner_role: String,
    pub risk: String,
    pub serves: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdTree {
    #[schemars(length(min = 3, max = 20))]
    pub nodes: Vec<BassAckwardsPrdNode>,
    pub reaches_today: bool,
    #[schemars(extend("x-required-nullable" = true))]
    pub gap_note: Option<String>,
    #[schemars(length(max = 5))]
    pub today_assumptions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdProvenance {
    pub source_labels: Vec<String>,
    pub note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdLineItem {
    #[schemars(extend("x-required-nullable" = true))]
    pub key: Option<String>,
    pub local_id: String,
    pub text: String,
    pub owner_role: String,
    pub precondition: String,
    pub risk: String,
    #[schemars(length(min = 1, max = 12))]
    pub serves: Vec<String>,
    #[schemars(length(max = 60))]
    pub depends_on: Vec<String>,
    #[schemars(length(min = 1))]
    pub sources: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdPhase {
    #[schemars(extend("x-required-nullable" = true))]
    pub key: Option<String>,
    pub local_id: String,
    pub title: String,
    #[schemars(length(min = 1, max = 8))]
    pub line_items: Vec<BassAckwardsPrdLineItem>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdMilestone {
    #[schemars(extend("x-required-nullable" = true))]
    pub key: Option<String>,
    pub local_id: String,
    pub title: String,
    pub goal: String,
    #[schemars(length(min = 1, max = 4))]
    pub phases: Vec<BassAckwardsPrdPhase>,
    pub provenance: BassAckwardsPrdProvenance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDocument {
    pub title: String,
    pub overview: String,
    #[schemars(length(min = 2, max = 8))]
    pub milestones: Vec<BassAckwardsPrdMilestone>,
    pub overview_provenance: BassAckwardsPrdProvenance,
    #[schemars(length(max = 8))]
    pub open_questions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdComment {
    pub local_id: String,
    pub quote: String,
    pub comment: String,
    #[schemars(extend("x-required-nullable" = true))]
    pub proposed: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdReview {
    #[schemars(extend("x-required-nullable" = true))]
    pub build: Option<String>,
    #[schemars(length(max = 4))]
    pub comments: Vec<BassAckwardsPrdComment>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdAddition {
    pub local_id: String,
    pub text: String,
    pub under: String,
    pub depends_on: Vec<String>,
    pub serves: Vec<String>,
    pub owner_role: String,
    pub risk: String,
    pub why: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdMustHaves {
    #[schemars(length(max = 3))]
    pub additions: Vec<BassAckwardsPrdAddition>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDecision {
    #[serde(rename = "ref")]
    pub reference: String,
    pub verdict: String,
    pub reason: String,
    pub applied_to: Vec<String>,
    pub removes: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdDecisions {
    pub decisions: Vec<BassAckwardsPrdDecision>,
    pub document: BassAckwardsPrdDocument,
}
/// Sparse wire output: unchanged PRD bytes never consume judge output tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum BassAckwardsPrdEdit {
    SetText {
        key: String,
        field: String,
        value: String,
    },
    SetList {
        key: String,
        field: String,
        values: Vec<String>,
    },
    AddMilestone {
        milestone: BassAckwardsPrdMilestone,
    },
    AddPhase {
        #[serde(rename = "milestoneKey")]
        milestone_key: String,
        phase: BassAckwardsPrdPhase,
    },
    AddLine {
        #[serde(rename = "phaseKey")]
        phase_key: String,
        line: BassAckwardsPrdLineItem,
    },
    Remove {
        key: String,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdPatchDecisions {
    pub decisions: Vec<BassAckwardsPrdDecision>,
    #[schemars(length(max = 120))]
    pub edits: Vec<BassAckwardsPrdEdit>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdPatchApplication {
    pub applied: Vec<BassAckwardsPrdApplied>,
    #[schemars(length(max = 120))]
    pub edits: Vec<BassAckwardsPrdEdit>,
}

/// Apply to either the frozen blind projection (validation) or stored PRD
/// (commit). Addressing is by permanent key; positions and identities cannot be set.
pub fn apply_edits(
    prior: &BassAckwardsPrdDocument,
    edits: &[BassAckwardsPrdEdit],
) -> Result<BassAckwardsPrdDocument, validation::ValidationError> {
    use serde_json::{Value, json};
    fn err() -> validation::ValidationError {
        validation::ValidationError {
            message: "invalid PRD edit target, field or operation".into(),
        }
    }
    fn find<'a>(v: &'a mut Value, key: &str) -> Option<&'a mut Value> {
        if v.get("key").and_then(Value::as_str) == Some(key) {
            return Some(v);
        }
        match v {
            Value::Object(o) => o.values_mut().find_map(|v| find(v, key)),
            Value::Array(a) => a.iter_mut().find_map(|v| find(v, key)),
            _ => None,
        }
    }
    fn remove(v: &mut Value, key: &str) -> bool {
        match v {
            Value::Array(a) => {
                if let Some(i) = a.iter().position(|v| v["key"].as_str() == Some(key)) {
                    a.remove(i);
                    true
                } else {
                    a.iter_mut().any(|v| remove(v, key))
                }
            }
            Value::Object(o) => o.values_mut().any(|v| remove(v, key)),
            _ => false,
        }
    }
    ensure(edits.len() <= 120, "too many PRD edits")?;
    let mut doc = serde_json::to_value(prior).map_err(|_| err())?;
    for edit in edits {
        match edit {
            BassAckwardsPrdEdit::SetText { key, field, value } => {
                ensure(
                    [
                        "title",
                        "overview",
                        "goal",
                        "text",
                        "ownerRole",
                        "precondition",
                        "risk",
                    ]
                    .contains(&field.as_str()),
                    "invalid text field",
                )?;
                let target = if key == "$document" {
                    &mut doc
                } else {
                    find(&mut doc, key).ok_or_else(err)?
                };
                ensure(
                    target.get(field).is_some_and(Value::is_string),
                    "invalid text target",
                )?;
                target[field] = json!(value);
            }
            BassAckwardsPrdEdit::SetList { key, field, values } => {
                ensure(
                    ["serves", "dependsOn", "sources", "openQuestions"].contains(&field.as_str()),
                    "invalid list field",
                )?;
                let target = if key == "$document" {
                    &mut doc
                } else {
                    find(&mut doc, key).ok_or_else(err)?
                };
                ensure(
                    target.get(field).is_some_and(Value::is_array),
                    "invalid list target",
                )?;
                target[field] = json!(values);
            }
            BassAckwardsPrdEdit::AddMilestone { milestone } => {
                ensure(
                    milestone.key.is_none()
                        && milestone.phases.iter().all(|p| {
                            p.key.is_none() && p.line_items.iter().all(|l| l.key.is_none())
                        }),
                    "new entities cannot carry keys",
                )?;
                doc["milestones"]
                    .as_array_mut()
                    .ok_or_else(err)?
                    .push(json!(milestone));
            }
            BassAckwardsPrdEdit::AddPhase {
                milestone_key,
                phase,
            } => {
                ensure(
                    phase.key.is_none() && phase.line_items.iter().all(|l| l.key.is_none()),
                    "new entities cannot carry keys",
                )?;
                find(&mut doc, milestone_key).ok_or_else(err)?["phases"]
                    .as_array_mut()
                    .ok_or_else(err)?
                    .push(json!(phase));
            }
            BassAckwardsPrdEdit::AddLine { phase_key, line } => {
                ensure(line.key.is_none(), "new entities cannot carry keys")?;
                find(&mut doc, phase_key).ok_or_else(err)?["lineItems"]
                    .as_array_mut()
                    .ok_or_else(err)?
                    .push(json!(line));
            }
            BassAckwardsPrdEdit::Remove { key } => {
                ensure(remove(&mut doc, key), "unknown removal key")?;
            }
        }
    }
    let doc: BassAckwardsPrdDocument = validation::decode(&doc)?;
    doc.validate()?;
    Ok(doc)
}

/// Normalize sparse output for the existing D96 integrity checks and durable logs.
pub fn expand_edits(
    data: &serde_json::Value,
    out: &serde_json::Value,
) -> Result<serde_json::Value, validation::ValidationError> {
    let edits: Vec<BassAckwardsPrdEdit> = validation::decode(&out["edits"])?;
    let prior: BassAckwardsPrdDocument = validation::decode(&data["document"])?;
    let mut expanded = out.clone();
    expanded
        .as_object_mut()
        .ok_or_else(|| validation::ValidationError {
            message: "invalid patch object".into(),
        })?
        .remove("edits");
    expanded["document"] =
        serde_json::to_value(apply_edits(&prior, &edits)?).expect("document serialization");
    Ok(expanded)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdApplied {
    pub note: String,
    pub applied_to: Vec<String>,
    #[schemars(extend("x-required-nullable" = true))]
    pub conflict: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdApplication {
    pub applied: Vec<BassAckwardsPrdApplied>,
    pub document: BassAckwardsPrdDocument,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdChecks {
    pub mapped: bool,
    pub covered: bool,
    pub dependencies: bool,
    pub acyclic: bool,
    pub uncovered: Vec<String>,
    pub levels: Vec<Vec<String>>,
    pub critical_path: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdSaved {
    pub filename: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BassAckwardsPrdReport {
    pub review_log: Vec<serde_json::Value>,
    pub must_have_log: Vec<serde_json::Value>,
    pub decisions: Vec<serde_json::Value>,
    pub human_notes: Vec<serde_json::Value>,
    pub human_application: Vec<BassAckwardsPrdApplied>,
    pub skipped_members: Vec<serde_json::Value>,
    pub dropped_quotes: usize,
    pub provenance: std::collections::BTreeMap<String, String>,
    pub markdown: String,
    pub checks: BassAckwardsPrdChecks,
    #[schemars(extend("x-required-nullable" = true))]
    pub path: Option<String>,
    #[schemars(extend("x-required-nullable" = true))]
    pub save_error: Option<String>,
}

pub fn composition(kind: &WorkKind) -> Option<&'static str> {
    Some(match kind {
        WorkKind::BassAckwardsPrdDone => "bass_ackwards_prd.done",
        WorkKind::BassAckwardsPrdFreeze => "bass_ackwards_prd.freeze",
        WorkKind::BassAckwardsPrdTree => "bass_ackwards_prd.tree",
        WorkKind::BassAckwardsPrdMerge => "bass_ackwards_prd.merge",
        WorkKind::BassAckwardsPrdReview => "bass_ackwards_prd.review",
        WorkKind::BassAckwardsPrdRevise => "bass_ackwards_prd.revise",
        WorkKind::BassAckwardsPrdMustHave => "bass_ackwards_prd.must_have",
        WorkKind::BassAckwardsPrdFinalize => "bass_ackwards_prd.finalize",
        WorkKind::BassAckwardsPrdApply => "bass_ackwards_prd.apply",
        _ => return None,
    })
}
pub fn judge(kind: &WorkKind) -> bool {
    matches!(
        kind,
        WorkKind::BassAckwardsPrdFreeze
            | WorkKind::BassAckwardsPrdMerge
            | WorkKind::BassAckwardsPrdRevise
            | WorkKind::BassAckwardsPrdFinalize
            | WorkKind::BassAckwardsPrdApply
    )
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    use crate::validation::schema;
    Some(match kind {
        WorkKind::BassAckwardsPrdDone => schema::<BassAckwardsPrdDoneList>(),
        WorkKind::BassAckwardsPrdFreeze => schema::<BassAckwardsPrdGrouping>(),
        WorkKind::BassAckwardsPrdTree => schema::<BassAckwardsPrdTree>(),
        WorkKind::BassAckwardsPrdMerge => schema::<BassAckwardsPrdDocument>(),
        WorkKind::BassAckwardsPrdReview => schema::<BassAckwardsPrdReview>(),
        WorkKind::BassAckwardsPrdMustHave => schema::<BassAckwardsPrdMustHaves>(),
        WorkKind::BassAckwardsPrdRevise | WorkKind::BassAckwardsPrdFinalize => {
            schema::<BassAckwardsPrdPatchDecisions>()
        }
        WorkKind::BassAckwardsPrdApply => schema::<BassAckwardsPrdPatchApplication>(),
        _ => return None,
    })
}
fn words(s: &str, n: usize) -> bool {
    !s.trim().is_empty() && s.split_whitespace().count() <= n
}
fn valid_ids(ids: &[String]) -> bool {
    unique(ids) && ids.iter().all(|s| !s.is_empty())
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    use validation::decode;
    match kind.rsplit('.').next().unwrap_or("") {
        "done" => {
            let v = decode::<BassAckwardsPrdDoneList>(body)?;
            ensure(
                (3..=8).contains(&v.items.len()),
                "done list requires 3 to 8 items",
            )?;
            ensure(
                valid_ids(
                    &v.items
                        .iter()
                        .map(|i| i.local_id.clone())
                        .collect::<Vec<_>>(),
                ) && v
                    .items
                    .iter()
                    .all(|i| words(&i.test, 30) && words(&i.verified_by, 15)),
                "invalid done item",
            )
        }
        "freeze" => {
            let v = decode::<BassAckwardsPrdGrouping>(body)?;
            ensure(
                !v.groups.is_empty()
                    && v.groups.iter().all(|g| {
                        !g.text.is_empty() && !g.source_refs.is_empty() && valid_ids(&g.source_refs)
                    }),
                "invalid grouping",
            )
        }
        "tree" => decode::<BassAckwardsPrdTree>(body)?.validate(),
        "merge" | "version" => decode::<BassAckwardsPrdDocument>(body)?.validate(),
        "review" => {
            let v = decode::<BassAckwardsPrdReview>(body)?;
            ensure(
                v.comments.len() <= 4
                    && v.build.as_ref().is_none_or(|b| words(b, 150))
                    && valid_ids(
                        &v.comments
                            .iter()
                            .map(|c| c.local_id.clone())
                            .collect::<Vec<_>>(),
                    )
                    && v.comments.iter().all(|c| {
                        words(&c.comment, 60) && c.proposed.as_ref().is_none_or(|p| words(p, 60))
                    }),
                "invalid review",
            )
        }
        "must_have" => {
            let v = decode::<BassAckwardsPrdMustHaves>(body)?;
            ensure(
                v.additions.len() <= 3
                    && valid_ids(
                        &v.additions
                            .iter()
                            .map(|c| c.local_id.clone())
                            .collect::<Vec<_>>(),
                    )
                    && v.additions.iter().all(|a| {
                        !a.text.is_empty()
                            && !a.under.is_empty()
                            && !a.serves.is_empty()
                            && words(&a.owner_role, 5)
                            && words(&a.risk, 30)
                    }),
                "invalid must-haves",
            )
        }
        "revise" | "finalize" => {
            let v = decode::<BassAckwardsPrdPatchDecisions>(body)?;
            ensure(v.edits.len() <= 120, "too many PRD edits")?;
            ensure(
                valid_ids(
                    &v.decisions
                        .iter()
                        .map(|d| d.reference.clone())
                        .collect::<Vec<_>>(),
                ) && v.decisions.iter().all(|d| {
                    ["accept", "reject"].contains(&d.verdict.as_str())
                        && words(&d.reason, 25)
                        && !d.reason.contains('\n')
                        && (d.verdict != "accept" || !d.applied_to.is_empty())
                }),
                "invalid decisions",
            )
        }
        "apply" => {
            let v = decode::<BassAckwardsPrdPatchApplication>(body)?;
            ensure(v.edits.len() <= 120, "too many PRD edits")
        }
        "checks" => {
            decode::<BassAckwardsPrdChecks>(body)?;
            Ok(())
        }
        "saved" => {
            decode::<BassAckwardsPrdSaved>(body)?;
            Ok(())
        }
        "report" => {
            decode::<BassAckwardsPrdReport>(body)?;
            Ok(())
        }
        "definition" => {
            decode::<BassAckwardsPrdDone>(body)?;
            Ok(())
        }
        "setup" => {
            decode::<BassAckwardsPrdSetup>(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown PRD artifact"),
    }
}
impl BassAckwardsPrdTree {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (3..=20).contains(&self.nodes.len()) && self.today_assumptions.len() <= 5,
            "invalid tree size",
        )?;
        ensure(
            self.reaches_today || self.gap_note.as_ref().is_some_and(|s| !s.trim().is_empty()),
            "missing today gap",
        )?;
        ensure(
            valid_ids(
                &self
                    .nodes
                    .iter()
                    .map(|n| n.node_id.clone())
                    .collect::<Vec<_>>(),
            ),
            "duplicate tree node",
        )?;
        for n in &self.nodes {
            ensure(
                words(&n.precondition, 40)
                    && words(&n.deliverable, 40)
                    && words(&n.owner_role, 5)
                    && words(&n.risk, 30)
                    && !n.serves.is_empty()
                    && valid_ids(&n.serves),
                "invalid node",
            )?;
            let mut seen = std::collections::BTreeSet::new();
            let mut current = Some(n.node_id.as_str());
            while let Some(id) = current {
                ensure(
                    seen.insert(id) && seen.len() <= 4,
                    "tree cycle or depth exceeds four",
                )?;
                current = self
                    .nodes
                    .iter()
                    .find(|p| p.node_id == id)
                    .ok_or_else(|| validation::ValidationError {
                        message: "unknown tree parent".into(),
                    })?
                    .parent
                    .as_deref();
            }
        }
        Ok(())
    }
}
impl BassAckwardsPrdDocument {
    pub fn lines(&self) -> Vec<&BassAckwardsPrdLineItem> {
        self.milestones
            .iter()
            .flat_map(|m| &m.phases)
            .flat_map(|p| &p.line_items)
            .collect()
    }
    pub fn entities(&self) -> std::collections::BTreeMap<String, serde_json::Value> {
        let mut out = std::collections::BTreeMap::new();
        out.insert("$document".into(), serde_json::json!({"title": self.title, "overview": self.overview, "overviewProvenance": self.overview_provenance, "openQuestions": self.open_questions}));
        for m in &self.milestones {
            let mut v = serde_json::to_value(m).expect("typed");
            v.as_object_mut().unwrap().remove("phases");
            out.insert(identity(&m.key, &m.local_id).into(), v);
            for p in &m.phases {
                let mut v = serde_json::to_value(p).expect("typed");
                v.as_object_mut().unwrap().remove("lineItems");
                out.insert(identity(&p.key, &p.local_id).into(), v);
                for l in &p.line_items {
                    out.insert(
                        identity(&l.key, &l.local_id).into(),
                        serde_json::to_value(l).expect("typed"),
                    );
                }
            }
        }
        out
    }
    pub fn order(&self) -> Vec<String> {
        self.milestones
            .iter()
            .flat_map(|m| {
                std::iter::once(identity(&m.key, &m.local_id).to_owned()).chain(
                    m.phases.iter().flat_map(|p| {
                        std::iter::once(identity(&p.key, &p.local_id).to_owned()).chain(
                            p.line_items
                                .iter()
                                .map(|l| identity(&l.key, &l.local_id).to_owned()),
                        )
                    }),
                )
            })
            .collect()
    }
    pub fn validate(&self) -> ValidationResult {
        ensure(
            words(&self.title, 100)
                && self.overview.split_whitespace().count() <= 200
                && (2..=8).contains(&self.milestones.len())
                && self.open_questions.len() <= 8
                && self.lines().len() <= 60,
            "invalid document size",
        )?;
        ensure(valid_ids(&self.order()), "duplicate document identities")?;
        ensure(
            !self.overview_provenance.source_labels.is_empty()
                && self.overview_provenance.note.split_whitespace().count() <= 30,
            "invalid overview provenance",
        )?;
        ensure(
            prose_words(&serde_json::to_value(self).expect("typed document")) <= PRD_WORD_LIMIT,
            "PRD prose exceeds 2400 words",
        )?;
        for (mi, m) in self.milestones.iter().enumerate() {
            ensure(
                (1..=4).contains(&m.phases.len())
                    && !m.provenance.source_labels.is_empty()
                    && m.provenance.note.split_whitespace().count() <= 30,
                "invalid milestone",
            )?;
            for p in &m.phases {
                ensure((1..=8).contains(&p.line_items.len()), "invalid phase")?;
                for l in &p.line_items {
                    ensure(
                        !l.serves.is_empty()
                            && valid_ids(&l.serves)
                            && !l.sources.is_empty()
                            && !l.text.is_empty(),
                        "unsourced or unmapped line",
                    )?;
                    for dep in &l.depends_on {
                        let location = self.milestones.iter().position(|m| {
                            m.phases
                                .iter()
                                .flat_map(|p| &p.line_items)
                                .any(|i| identity(&i.key, &i.local_id) == dep)
                        });
                        ensure(
                            location.is_some_and(|i| i <= mi),
                            "missing dependency or later milestone",
                        )?;
                    }
                }
            }
        }
        fn visit(
            d: &BassAckwardsPrdDocument,
            id: &str,
            seen: &mut Vec<String>,
            complete: &mut std::collections::BTreeSet<String>,
        ) -> ValidationResult {
            if complete.contains(id) {
                return Ok(());
            }
            ensure(!seen.iter().any(|s| s == id), "dependency cycle")?;
            seen.push(id.into());
            let l = d
                .lines()
                .into_iter()
                .find(|l| identity(&l.key, &l.local_id) == id)
                .ok_or_else(|| validation::ValidationError {
                    message: "missing dependency".into(),
                })?;
            for dep in &l.depends_on {
                visit(d, dep, seen, complete)?;
            }
            seen.pop();
            complete.insert(id.into());
            Ok(())
        }
        let mut complete = std::collections::BTreeSet::new();
        for l in self.lines() {
            visit(
                self,
                identity(&l.key, &l.local_id),
                &mut vec![],
                &mut complete,
            )?;
        }
        Ok(())
    }
}
pub fn identity<'a>(key: &'a Option<String>, local: &'a str) -> &'a str {
    key.as_deref().unwrap_or(local)
}
/// Validate against the exact frozen projection, never room history.
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    out: &serde_json::Value,
) -> ValidationResult {
    let name = composition(kind).ok_or_else(|| validation::ValidationError {
        message: "not PRD work".into(),
    })?;
    validate_artifact(name, out)?;
    let expanded;
    let out = if matches!(
        kind,
        WorkKind::BassAckwardsPrdRevise
            | WorkKind::BassAckwardsPrdFinalize
            | WorkKind::BassAckwardsPrdApply
    ) {
        expanded = expand_edits(data, out)?;
        &expanded
    } else {
        out
    };
    let done_ids = data["done"]["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["id"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if *kind == WorkKind::BassAckwardsPrdFreeze {
        let g = validation::decode::<BassAckwardsPrdGrouping>(out)?;
        let items = data["items"]
            .as_array()
            .ok_or_else(|| validation::ValidationError {
                message: "missing done proposals".into(),
            })?;
        let refs = g
            .groups
            .iter()
            .flat_map(|g| g.source_refs.clone())
            .collect::<Vec<_>>();
        ensure(
            unique(&refs)
                && refs.len() == items.len()
                && items
                    .iter()
                    .all(|i| refs.iter().any(|r| Some(r.as_str()) == i["ref"].as_str())),
            "every done proposal must appear exactly once",
        )?;
        for group in g.groups {
            ensure(
                group.source_refs.iter().any(|r| {
                    items
                        .iter()
                        .any(|i| i["ref"] == *r && i["test"] == group.text)
                }),
                "group text must be byte-identical to a source",
            )?;
        }
    }
    if *kind == WorkKind::BassAckwardsPrdTree {
        let t = validation::decode::<BassAckwardsPrdTree>(out)?;
        ensure(
            t.nodes
                .iter()
                .all(|n| n.serves.iter().all(|id| done_ids.contains(&id.as_str()))),
            "unknown DoD id",
        )?;
    }
    if *kind == WorkKind::BassAckwardsPrdReview {
        let r = validation::decode::<BassAckwardsPrdReview>(out)?;
        ensure(
            (data["lens"] == "READBACK") == r.build.is_some(),
            "readback build required only for READBACK",
        )?;
    }
    if *kind == WorkKind::BassAckwardsPrdMustHave {
        let v = validation::decode::<BassAckwardsPrdMustHaves>(out)?;
        let milestones = data["keys"]["milestones"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let line_keys = data["keys"]["lines"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|l| l["key"].as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ensure(
            v.additions.iter().all(|a| {
                milestones.iter().any(|m| m["key"] == a.under)
                    && a.serves.iter().all(|id| done_ids.contains(&id.as_str()))
                    && a.depends_on
                        .iter()
                        .all(|id| line_keys.contains(&id.as_str()))
            }),
            "invalid addition reference",
        )?;
    }

    if judge(kind) && *kind != WorkKind::BassAckwardsPrdFreeze {
        let doc = if *kind == WorkKind::BassAckwardsPrdMerge {
            validation::decode::<BassAckwardsPrdDocument>(out)?
        } else {
            validation::decode::<BassAckwardsPrdDocument>(&out["document"])?
        };
        ensure(
            doc.lines()
                .iter()
                .all(|l| l.serves.iter().all(|id| done_ids.contains(&id.as_str()))),
            "unknown DoD id",
        )?;
        let sources = data["sourceIds"].as_array().cloned().unwrap_or_default();
        let prior = data
            .get("document")
            .map(validation::decode::<BassAckwardsPrdDocument>)
            .transpose()?;
        let old = prior.as_ref().map(|d| d.entities()).unwrap_or_default();
        for m in &doc.milestones {
            for (key, local) in
                std::iter::once((&m.key, &m.local_id)).chain(m.phases.iter().flat_map(|p| {
                    std::iter::once((&p.key, &p.local_id))
                        .chain(p.line_items.iter().map(|l| (&l.key, &l.local_id)))
                }))
            {
                ensure(
                    key.as_ref().is_none_or(|k| old.contains_key(k)),
                    "judge cannot mint permanent keys",
                )?;
                ensure(!local.is_empty(), "empty local identity")?;
            }
        }

        let labels = data["sourceLabels"].as_array().cloned().unwrap_or_default();
        for m in &doc.milestones {
            ensure(
                m.provenance
                    .source_labels
                    .iter()
                    .all(|s| labels.contains(&serde_json::json!(s))),
                "unknown provenance label",
            )?;
        }
        ensure(
            doc.overview_provenance
                .source_labels
                .iter()
                .all(|s| labels.contains(&serde_json::json!(s))),
            "unknown overview provenance",
        )?;
        for l in doc.lines() {
            if !old.contains_key(identity(&l.key, &l.local_id)) {
                ensure(
                    l.sources
                        .iter()
                        .all(|s| sources.contains(&serde_json::json!(s))),
                    "invented line source",
                )?;
            }
        }
        if let Some(prior) = prior {
            let new = doc.entities();
            let mut touched = Vec::new();
            let mut removed = Vec::new();
            if *kind == WorkKind::BassAckwardsPrdApply {
                let applied = validation::decode::<BassAckwardsPrdApplication>(out)?.applied;
                let notes = data["notes"].as_array().cloned().unwrap_or_default();
                ensure(
                    applied.len() == notes.len()
                        && applied.iter().enumerate().all(|(i, a)| {
                            notes[i]["ref"] == a.note
                                && (!a.applied_to.is_empty()
                                    || a.conflict.as_ref().is_some_and(|s| !s.is_empty()))
                        }),
                    "every human note must be applied or conflicted",
                )?;
                for a in applied {
                    touched.extend(a.applied_to);
                }
            } else {
                let decisions = validation::decode::<BassAckwardsPrdDecisions>(out)?.decisions;
                ensure(
                    decisions.len() == sources.len()
                        && sources.iter().all(|s| {
                            decisions
                                .iter()
                                .any(|d| serde_json::json!(d.reference) == *s)
                        }),
                    "missing decision",
                )?;
                for d in decisions {
                    if d.verdict == "accept" {
                        ensure(
                            d.applied_to.iter().all(|id| {
                                new.contains_key(id)
                                    || (old.contains_key(id) && d.removes.contains(id))
                            }),
                            "decision target missing",
                        )?;
                        touched.extend(d.applied_to);
                        ensure(
                            d.removes
                                .iter()
                                .all(|id| old.contains_key(id) && !new.contains_key(id)),
                            "invalid removed identity",
                        )?;
                        removed.extend(d.removes);
                    }
                }
            }

            for l in doc.lines() {
                if !old.contains_key(identity(&l.key, &l.local_id)) {
                    let lid = identity(&l.key, &l.local_id);
                    ensure(
                        touched.iter().any(|id| id == lid),
                        "new line not named by an accepted decision",
                    )?;
                    if *kind != WorkKind::BassAckwardsPrdApply {
                        let decisions =
                            validation::decode::<BassAckwardsPrdDecisions>(out)?.decisions;
                        ensure(
                            l.sources.iter().all(|source| {
                                decisions.iter().any(|d| {
                                    d.reference == *source
                                        && d.verdict == "accept"
                                        && d.applied_to.iter().any(|id| id == lid)
                                })
                            }),
                            "new line has no accepted source",
                        )?;
                    }
                }
            }
            for (key, value) in &old {
                if *kind != WorkKind::BassAckwardsPrdFinalize
                    && !touched.contains(key)
                    && !removed.contains(key)
                {
                    ensure(new.get(key) == Some(value), "unapproved text changed")?;
                }
                ensure(
                    new.contains_key(key) || removed.contains(key),
                    "unapproved removal",
                )?;
            }
            if *kind == WorkKind::BassAckwardsPrdFinalize {
                ensure(removed.is_empty(), "final assembly cannot remove")?;
                let order = doc
                    .order()
                    .into_iter()
                    .filter(|k| old.contains_key(k))
                    .collect::<Vec<_>>();
                ensure(
                    order == prior.order(),
                    "final assembly reordered existing items",
                )?;
                for (key, value) in &old {
                    let mut next =
                        new.get(key)
                            .cloned()
                            .ok_or_else(|| validation::ValidationError {
                                message: "removed item".into(),
                            })?;
                    let mut prev = value.clone();
                    if let (Some(a), Some(b)) =
                        (prev.get_mut("dependsOn"), next.get_mut("dependsOn"))
                    {
                        let old_deps = a.as_array().unwrap().clone();
                        let new_deps = b.as_array().unwrap().clone();
                        ensure(
                            old_deps.iter().all(|d| new_deps.contains(d))
                                && new_deps
                                    .iter()
                                    .filter(|d| !old_deps.contains(d))
                                    .all(|d| !old.contains_key(d.as_str().unwrap_or(""))),
                            "final dependency mutation",
                        )?;
                        *a = serde_json::json!([]);
                        *b = serde_json::json!([]);
                    }
                    ensure(prev == next, "final assembly changed existing text")?;
                }
                let additions = data["additions"].as_array().cloned().unwrap_or_default();
                for m in &doc.milestones {
                    for l in m.phases.iter().flat_map(|p| &p.line_items) {
                        if !old.contains_key(identity(&l.key, &l.local_id)) {
                            ensure(
                                l.sources.iter().any(|s| {
                                    additions.iter().any(|a| {
                                        a["ref"] == *s && a["under"] == serde_json::json!(m.key)
                                    })
                                }),
                                "addition under wrong milestone",
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Count prose, excluding the identifiers and graph edges needed to reproduce it.
pub fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::String(s) => s.split_whitespace().count(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::Object(o) => o
            .iter()
            .filter(|(k, _)| !(k.as_str() == "note" && o.contains_key("conflict")))
            .filter(|(k, _)| {
                ![
                    "key",
                    "localId",
                    "nodeId",
                    "parent",
                    "serves",
                    "dependsOn",
                    "sources",
                    "sourceRefs",
                    "sourceLabels",
                    "ref",
                    "appliedTo",
                    "removes",
                    "under",
                    "verdict",
                    "lens",
                ]
                .contains(&k.as_str())
            })
            .map(|(_, v)| prose_words(v))
            .sum(),
        _ => 0,
    }
}
