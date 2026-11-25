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
            (4..=12).contains(&m) && m % 2 == 0 && unique(&self.participants),
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
impl ProbeReport {
    pub fn validate_against(&self, material: &ProbeCaseMaterial) -> ValidationResult {
        ensure(
            self.prediction_matrix == material.prediction_matrix,
            "report changed locked predictions",
        )?;
        let explanations: BTreeSet<_> = material.explanations.iter().map(|e| e.id).collect();
        let mut used = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut stack = vec![(self.decision_tree.root.as_str(), BTreeSet::new())];
        while let Some((id, mut path)) = stack.pop() {
            ensure(path.insert(id), "cyclic decision tree")?;
            ensure(visited.insert(id), "decision tree contains shared node")?;
            let node = self.decision_tree.nodes.get(id);
            ensure(node.is_some(), "unknown tree node")?;
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
                        "tree changed measurements or outcomes",
                    )?;
                    ensure(
                        matches!(self.decision_tree.nodes.get(unexpected.as_str()),Some(ProbeNode::Leaf{reassess:true,remaining,..}) if remaining.iter().copied().collect::<BTreeSet<_>>()==explanations),
                        "unexpected result must reopen brief",
                    )?;
                    used.insert(*check_id);
                    for next in branches.values().chain(std::iter::once(unexpected)) {
                        stack.push((next.as_str(), path.clone()));
                    }
                }
                ProbeNode::Leaf { remaining, .. } => {
                    ensure(
                        !remaining.is_empty()
                            && unique(remaining)
                            && remaining.iter().all(|e| explanations.contains(e)),
                        "invalid remaining explanations",
                    )?;
                }
            }
        }
        ensure(
            visited.len() == self.decision_tree.nodes.len(),
            "unreachable tree nodes",
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
                    && group
                        .explanations
                        .iter()
                        .all(|id| explanations.contains(id)),
                "invalid inseparable group references",
            )?;
        }
        for group in &self.inseparable_groups {
            ensure(
                group.explanations.len() >= 2
                    && unique(&group.explanations)
                    && group.explanations.iter().all(|e| explanations.contains(e)),
                "invalid inseparable group",
            )?;
        }
        // Missing and unpredictable rows must survive every outcome: neither is negative evidence.
        let uncertain: BTreeSet<_> = material
            .prediction_matrix
            .iter()
            .filter(|(_, p)| {
                p.as_ref()
                    .is_none_or(|p| p.predictions.iter().any(|p| !p.predictable))
            })
            .map(|(id, _)| *id)
            .collect();
        for node in self.decision_tree.nodes.values() {
            if let ProbeNode::Leaf { remaining, .. } = node {
                ensure(
                    uncertain.iter().all(|id| remaining.contains(id)),
                    "tree eliminated uncertain evidence",
                )?;
            }
        }
        for group in &self.inseparable_groups {
            ensure(
                group.explanations.len() >= 2
                    && unique(&group.explanations)
                    && group
                        .explanations
                        .iter()
                        .all(|id| explanations.contains(id)),
                "invalid inseparable explanation references",
            )?;
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
