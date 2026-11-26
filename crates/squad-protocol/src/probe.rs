//! Probe's durable contracts. Provider outputs never carry authoritative identities.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProbeStatus {
    Preflight,
    WaitingForParticipants,
    Proposing,
    Predicting,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl ProbeStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeBrief {
    pub observation: Text,
    pub ruled_out: String,
    pub inspectable: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeSetup {
    pub contest_id: ContestId,
    pub brief: ProbeBrief,
    pub participants: Vec<MemberNick>,
    pub explainers: Vec<MemberNick>,
    pub checkers: Vec<MemberNick>,
    #[schemars(extend("x-required-nullable" = true))]
    pub sitting_out: Option<MemberNick>,
}
impl ProbeSetup {
    pub fn validate(&self) -> ValidationResult {
        let m = self.participants.len();
        ensure(
            (4..=12).contains(&m) && m.is_multiple_of(2) && unique(&self.participants),
            "probe needs distinct even participants",
        )?;
        ensure(
            self.explainers.len() == m / 2
                && self.checkers.len() == m / 2
                && unique(&self.explainers)
                && unique(&self.checkers)
                && self
                    .explainers
                    .iter()
                    .all(|n| self.participants.contains(n) && !self.checkers.contains(n))
                && self.checkers.iter().all(|n| self.participants.contains(n))
                && self
                    .sitting_out
                    .as_ref()
                    .is_none_or(|n| !self.participants.contains(n)),
            "invalid probe split",
        )
    }
    pub fn calls(&self) -> u32 {
        (self.participants.len() * 3 / 2 + 1) as u32
    }
    pub fn attempts(&self) -> u32 {
        (self.participants.len() * 3 + 3) as u32
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeState {
    pub contest_id: ContestId,
    pub state: ProbeStatus,
    pub state_version: u64,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub artifact_event_ids: Vec<EventId>,
    pub skipped_work_ids: Vec<WorkId>,
    pub skipped_members: Vec<MemberNick>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl ProbeState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0
                && unique(&self.artifact_event_ids)
                && unique(&self.skipped_work_ids)
                && unique(&self.skipped_members),
            "invalid probe state",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbePreflight {
    pub base: ContestPreflight,
    pub setup: ProbeSetup,
    pub reserved_attempts: u32,
}
impl ProbePreflight {
    pub fn validate(&self) -> ValidationResult {
        self.setup.validate()?;
        self.base.estimate.validate()?;
        ensure(
            self.base.kind == ContestKind::Probe
                && self.base.contest_id == self.setup.contest_id
                && self.base.participants == self.setup.participants
                && self.base.estimate.model_calls == self.setup.calls()
                && self.reserved_attempts == self.setup.attempts(),
            "invalid probe preflight",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProbeTransition {
    ParticipantsReady,
    ProposalsLocked,
    PredictionsLocked,
    ReportAccepted,
    Stop,
    Fail,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeExplanation {
    pub explanation: Text,
    pub key_assumption: Text,
    pub limitations: Vec<Text>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeOutcome {
    pub id: Text,
    pub description: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeCheck {
    pub check: Text,
    pub procedure: Text,
    pub measurement: Text,
    pub outcomes: Vec<ProbeOutcome>,
    pub effort: Text,
    pub safety_notes: Vec<Text>,
}
impl ProbeCheck {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.outcomes.len() >= 2
                && unique(&self.outcomes.iter().map(|v| &v.id).collect::<Vec<_>>())
                && !self.safety_notes.is_empty(),
            "check needs distinct outcomes and safety limitations",
        )
    }
}
/// UUIDv4 items have no contributor, event, timestamp or roster-order metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeItem<T> {
    pub id: CellId,
    pub data: T,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbePredictionItem {
    pub check_id: CellId,
    pub predictable: bool,
    pub expected_outcome_ids: Vec<Text>,
    #[schemars(extend("x-required-nullable" = true))]
    pub expected_observation: Option<Text>,
    pub reason: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbePrediction {
    pub predictions: Vec<ProbePredictionItem>,
}
impl ProbePrediction {
    pub fn validate_against(&self, checks: &[ProbeItem<ProbeCheck>]) -> ValidationResult {
        ensure(
            self.predictions.len() == checks.len()
                && unique(
                    &self
                        .predictions
                        .iter()
                        .map(|p| p.check_id)
                        .collect::<Vec<_>>(),
                ),
            "prediction coverage differs",
        )?;
        for p in &self.predictions {
            let c = checks.iter().find(|c| c.id == p.check_id);
            ensure(c.is_some(), "unknown check")?;
            let c = &c.expect("checked").data;
            ensure(
                unique(&p.expected_outcome_ids)
                    && p.expected_outcome_ids
                        .iter()
                        .all(|id| c.outcomes.iter().any(|o| &o.id == id))
                    && if p.predictable {
                        p.expected_observation.is_some() && !p.expected_outcome_ids.is_empty()
                    } else {
                        p.expected_observation.is_none() && p.expected_outcome_ids.is_empty()
                    },
                "invalid prediction or inability",
            )?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeCaseMaterial {
    pub brief: ProbeBrief,
    pub explanations: Vec<ProbeItem<ProbeExplanation>>,
    pub checks: Vec<ProbeItem<ProbeCheck>>,
    pub prediction_matrix: BTreeMap<CellId, Option<ProbePrediction>>,
    pub missing_rows: Vec<CellId>,
    pub skipped_members: Vec<MemberNick>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProbeNode {
    Check {
        check_id: CellId,
        measurement: Text,
        branches: BTreeMap<String, Text>,
        unexpected: Text,
    },
    Leaf {
        remaining: Vec<CellId>,
        next_action: Text,
        reassess: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeTree {
    pub root: Text,
    pub nodes: BTreeMap<String, ProbeNode>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeDiscardedCheck {
    pub check_id: CellId,
    pub reason: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeInseparableGroup {
    pub explanations: Vec<CellId>,
    pub reason: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeReport {
    pub decision_tree: ProbeTree,
    pub prediction_matrix: BTreeMap<CellId, Option<ProbePrediction>>,
    pub discarded_checks: Vec<ProbeDiscardedCheck>,
    pub inseparable_groups: Vec<ProbeInseparableGroup>,
    pub limitations: Vec<Text>,
}
impl ProbeCaseMaterial {
    pub fn validate(&self) -> ValidationResult {
        let ids = self
            .explanations
            .iter()
            .map(|e| e.id)
            .collect::<BTreeSet<_>>();
        ensure(
            !ids.is_empty()
                && ids.len() == self.explanations.len()
                && !self.checks.is_empty()
                && unique(&self.checks.iter().map(|c| c.id).collect::<Vec<_>>()),
            "invalid probe evidence identities",
        )?;
        ensure(
            self.prediction_matrix
                .keys()
                .copied()
                .collect::<BTreeSet<_>>()
                == ids,
            "matrix explanation coverage differs",
        )?;
        let missing = self
            .prediction_matrix
            .iter()
            .filter_map(|(id, p)| p.is_none().then_some(*id))
            .collect::<BTreeSet<_>>();
        ensure(
            unique(&self.missing_rows)
                && self.missing_rows.iter().copied().collect::<BTreeSet<_>>() == missing,
            "missing rows differ from matrix",
        )?;
        for c in &self.checks {
            c.data.validate()?;
        }
        for row in self.prediction_matrix.values().flatten() {
            row.validate_against(&self.checks)?;
        }
        Ok(())
    }
}
impl ProbeReport {
    pub fn validate_against(&self, material: &ProbeCaseMaterial) -> ValidationResult {
        material.validate()?;
        ensure(
            self.prediction_matrix == material.prediction_matrix,
            "report changed locked predictions",
        )?;
        let explanations = material
            .explanations
            .iter()
            .map(|e| e.id)
            .collect::<BTreeSet<_>>();
        let prediction = |explanation: &CellId, check: CellId| {
            material
                .prediction_matrix
                .get(explanation)
                .and_then(Option::as_ref)
                .and_then(|r| r.predictions.iter().find(|p| p.check_id == check))
        };
        let separates = |a: &CellId, b: &CellId, check: CellId| match (
            prediction(a, check),
            prediction(b, check),
        ) {
            (Some(a), Some(b)) if a.predictable && b.predictable => {
                a.expected_outcome_ids
                    .iter()
                    .map(Text::as_str)
                    .collect::<BTreeSet<_>>()
                    != b.expected_outcome_ids
                        .iter()
                        .map(Text::as_str)
                        .collect::<BTreeSet<_>>()
            }
            _ => false,
        };
        let mut used = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut stack = vec![(
            self.decision_tree.root.as_str(),
            BTreeSet::new(),
            explanations.clone(),
        )];
        while let Some((id, mut path, possible)) = stack.pop() {
            ensure(
                path.insert(id),
                format!("decision node {id} is its own ancestor; the tree must be acyclic"),
            )?;
            ensure(
                visited.insert(id),
                format!(
                    "decision node {id} is referenced by more than one branch; give every branch its own node with a distinct id, even when two leaves say the same thing"
                ),
            )?;
            let node = self.decision_tree.nodes.get(id);
            ensure(node.is_some(), "unknown decision node")?;
            match node.expect("checked") {
                ProbeNode::Check {
                    check_id,
                    measurement,
                    branches,
                    unexpected,
                } => {
                    let check = material.checks.iter().find(|c| c.id == *check_id);
                    ensure(check.is_some(), "unknown tree check")?;
                    let check = &check.expect("checked").data;
                    ensure(
                        measurement == &check.measurement
                            && branches.len() == check.outcomes.len()
                            && check
                                .outcomes
                                .iter()
                                .all(|o| branches.contains_key(o.id.as_str())),
                        "tree changed measurement or outcomes",
                    )?;
                    ensure(
                        possible.iter().any(|a| {
                            possible
                                .iter()
                                .any(|b| a != b && separates(a, b, *check_id))
                        }),
                        format!(
                            "check {check_id} at node {id} separates none of the explanations still possible on its path; move it to a node where two of them predict different outcomes for it, or list it in discardedChecks"
                        ),
                    )?;
                    ensure(
                        matches!(self.decision_tree.nodes.get(unexpected.as_str()),Some(ProbeNode::Leaf{reassess:true,remaining,..}) if remaining.iter().copied().collect::<BTreeSet<_>>()==explanations),
                        "unexpected result must reopen brief",
                    )?;
                    used.insert(*check_id);
                    for (outcome, next) in branches {
                        let mut compatible = possible
                            .iter()
                            .filter(|e| {
                                prediction(e, *check_id).is_none_or(|p| {
                                    !p.predictable
                                        || p.expected_outcome_ids
                                            .iter()
                                            .any(|id| id.as_str() == outcome)
                                })
                            })
                            .copied()
                            .collect::<BTreeSet<_>>();
                        if compatible.is_empty() {
                            compatible = explanations.clone();
                        }
                        stack.push((next.as_str(), path.clone(), compatible));
                    }
                    stack.push((unexpected.as_str(), path, explanations.clone()));
                }
                ProbeNode::Leaf { remaining, .. } => {
                    let still_possible = possible
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    ensure(
                        !remaining.is_empty(),
                        format!(
                            "leaf {id} has an empty remaining list; when no explanation predicts the outcome, keep every explanation still possible on its path ({still_possible}) and say in nextAction that the predictions failed"
                        ),
                    )?;
                    ensure(
                        unique(remaining) && remaining.iter().all(|e| explanations.contains(e)),
                        format!(
                            "leaf {id} repeats an explanation or names one that was not supplied"
                        ),
                    )?;
                    let dropped = possible
                        .iter()
                        .filter(|e| !remaining.contains(e))
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>();
                    ensure(
                        dropped.is_empty(),
                        format!(
                            "leaf {id} eliminated compatible, missing or uncertain predictions; it must also keep explanations {}",
                            dropped.join(", ")
                        ),
                    )?
                }
            }
        }
        ensure(
            visited.len() == self.decision_tree.nodes.len(),
            "unreachable decision node",
        )?;
        ensure(
            unique(
                &self
                    .discarded_checks
                    .iter()
                    .map(|c| c.check_id)
                    .collect::<Vec<_>>(),
            ),
            "duplicate discarded check",
        )?;
        for c in &self.discarded_checks {
            ensure(
                !used.contains(&c.check_id) && material.checks.iter().any(|x| x.id == c.check_id),
                "invalid discarded check",
            )?;
            used.insert(c.check_id);
        }
        ensure(used.len() == material.checks.len(), "unaccounted check")?;
        for group in &self.inseparable_groups {
            ensure(
                group.explanations.len() >= 2
                    && unique(&group.explanations)
                    && group.explanations.iter().all(|e| explanations.contains(e)),
                "invalid inseparable group references",
            )?;
            ensure(
                !group.explanations.iter().any(|a| {
                    group
                        .explanations
                        .iter()
                        .any(|b| a != b && material.checks.iter().any(|c| separates(a, b, c.id)))
                }),
                "group contains distinguishable explanations",
            )?;
        }
        for a in &explanations {
            for b in explanations.range(*a..).filter(|b| *b != a) {
                if !material.checks.iter().any(|c| separates(a, b, c.id)) {
                    ensure(
                        self.inseparable_groups
                            .iter()
                            .any(|g| g.explanations.contains(a) && g.explanations.contains(b)),
                        "missing inseparable explanation group",
                    )?;
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeOutput<T> {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub completed: CompletedGeneration,
    pub data: T,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "stage",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProbeInput {
    Propose {
        brief: ProbeBrief,
    },
    Predict {
        brief: ProbeBrief,
        explanation: ProbeItem<ProbeExplanation>,
        checks: Vec<ProbeItem<ProbeCheck>>,
    },
    Report {
        material: ProbeCaseMaterial,
    },
}
impl ProbeInput {
    pub fn validate_output(&self, kind: &WorkKind, value: &serde_json::Value) -> ValidationResult {
        use validation::decode;
        match (self, kind) {
            (Self::Propose { .. }, WorkKind::ProbeExplain) => {
                let _: ProbeExplanation = decode(value)?;
            }
            (Self::Propose { .. }, WorkKind::ProbeCheck) => {
                decode::<ProbeCheck>(value)?.validate()?
            }
            (Self::Predict { checks, .. }, WorkKind::ProbePredict) => {
                decode::<ProbePrediction>(value)?.validate_against(checks)?
            }
            (Self::Report { material }, WorkKind::ProbeReport) => {
                decode::<ProbeReport>(value)?.validate_against(material)?
            }
            _ => ensure(false, "probe input and work mismatch")?,
        }
        Ok(())
    }
}
