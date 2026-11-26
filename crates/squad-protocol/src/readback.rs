//! Readback contracts. Provider outputs never select the human's intended meaning.
use crate::validation::{self, ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
// Enable after the integrator lands shared judge dispatch and D75 repair fixes.
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReadbackStatus {
    Preflight,
    WaitingForParticipants,
    Interpreting,
    Diffing,
    AwaitingHuman,
    Rewriting,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl ReadbackStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: ReadbackStatus,
    #[schemars(extend("x-required-nullable"=true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable"=true))]
    pub terminal_reason: Option<Text>,
}
impl ReadbackState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid readback state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(
            self.round <= 4 && (self.round > 0 || self.state == ReadbackStatus::Preflight),
            "invalid readback round",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackSetup {
    pub spec: String,
    pub participants: Vec<MemberNick>,
    pub rounds: u8,
    pub room_words: u32,
    pub plan_words: u32,
    pub models: BTreeMap<String, ExecutorBinding>,
    pub prompt_refs: BTreeMap<String, PromptRef>,
}
impl ReadbackSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.spec.trim().is_empty()
                && (1..=4).contains(&self.rounds)
                && (3..=12).contains(&self.participants.len())
                && unique(&self.participants),
            "invalid readback setup",
        )?;
        ensure(
            self.room_words > 0
                && self.plan_words > 0
                && self.models.contains_key("judge")
                && self
                    .participants
                    .iter()
                    .all(|n| self.models.contains_key(n.as_str())),
            "missing frozen readback binding",
        )?;
        ensure(
            ["readback.interpret", "readback.diff", "readback.rewrite"]
                .iter()
                .all(|name| self.prompt_refs.contains_key(*name)),
            "missing frozen readback prompt",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackBehaviour {
    pub local_id: String,
    pub situation: String,
    pub behaviour: String,
    pub spec_quote: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackEdgeCase {
    pub case: String,
    pub answer: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackAssumption {
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackInterpretation {
    pub build: String,
    #[schemars(length(max = 8))]
    pub behaviours: Vec<ReadbackBehaviour>,
    #[schemars(length(max = 6))]
    pub edge_cases: Vec<ReadbackEdgeCase>,
    #[schemars(length(max = 6))]
    pub assumptions: Vec<ReadbackAssumption>,
}
impl ReadbackInterpretation {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.build.trim().is_empty()
                && self.behaviours.len() <= 8
                && self.edge_cases.len() <= 6
                && self.assumptions.len() <= 6,
            "invalid interpretation limits",
        )?;
        ensure(
            unique(
                &self
                    .behaviours
                    .iter()
                    .map(|b| &b.local_id)
                    .collect::<Vec<_>>(),
            ),
            "duplicate behaviour id",
        )?;
        ensure(
            self.edge_cases
                .iter()
                .all(|e| !e.case.trim().is_empty() && !e.answer.trim().is_empty())
                && self.assumptions.iter().all(|a| !a.text.trim().is_empty()),
            "empty edge case or assumption",
        )?;
        ensure(
            self.behaviours.iter().all(|b| {
                [&b.local_id, &b.situation, &b.behaviour, &b.spec_quote]
                    .iter()
                    .all(|s| !s.trim().is_empty())
            }),
            "empty behaviour",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackReading {
    pub reading_id: String,
    pub text: String,
    #[schemars(length(min = 1, max = 12))]
    pub source_labels: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackEntry {
    pub entry_id: String,
    pub passage: String,
    pub question: String,
    #[schemars(length(min = 2, max = 4))]
    pub readings: Vec<ReadbackReading>,
    pub behaviour_change: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackLedger {
    #[schemars(length(max = 100))]
    pub entries: Vec<ReadbackEntry>,
}
impl ReadbackLedger {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.entries.len() <= 100
                && unique(&self.entries.iter().map(|e| &e.entry_id).collect::<Vec<_>>()),
            "invalid ledger ids or limit",
        )?;
        for e in &self.entries {
            ensure(
                [&e.entry_id, &e.passage, &e.question, &e.behaviour_change]
                    .iter()
                    .all(|s| !s.trim().is_empty())
                    && (2..=4).contains(&e.readings.len()),
                "invalid ledger entry",
            )?;
            ensure(
                unique(&e.readings.iter().map(|r| &r.reading_id).collect::<Vec<_>>()),
                "duplicate reading id",
            )?;
            for (i, r) in e.readings.iter().enumerate() {
                ensure(
                    !r.reading_id.is_empty()
                        && !r.text.trim().is_empty()
                        && !r.source_labels.is_empty()
                        && r.source_labels.len() <= 12
                        && unique(&r.source_labels),
                    "reading requires distinct sources",
                )?;
                let mut sources = r.source_labels.clone();
                sources.sort();
                ensure(
                    !e.readings[..i].iter().any(|old| {
                        let mut s = old.source_labels.clone();
                        s.sort();
                        s == sources && old.text == r.text
                    }),
                    "duplicate reading",
                )?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackChoices {
    pub answers: BTreeMap<String, String>,
    pub raw_answers: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackChange {
    pub entry_id: String,
    pub before: String,
    pub after: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackRewrite {
    pub spec: String,
    #[schemars(length(max = 10))]
    pub changes: Vec<ReadbackChange>,
}
impl ReadbackRewrite {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.spec.trim().is_empty()
                && self.changes.len() <= 10
                && unique(&self.changes.iter().map(|c| &c.entry_id).collect::<Vec<_>>()),
            "invalid rewrite",
        )?;
        ensure(
            self.changes.iter().all(|c| {
                [&c.entry_id, &c.before, &c.after]
                    .iter()
                    .all(|s| !s.trim().is_empty())
            }),
            "empty change",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackConvergence {
    pub round: u32,
    pub spec_version: u32,
    pub accepted_interpretations: u32,
    pub ledger_entries: u32,
    pub resolved: u32,
    pub open: u32,
    pub cut: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackRoundReport {
    pub spec: String,
    pub interpretations: BTreeMap<String, ReadbackInterpretation>,
    pub provenance: BTreeMap<String, MemberNick>,
    pub ledger: Option<ReadbackLedger>,
    pub choices: Option<ReadbackChoices>,
    pub rewrite: Option<ReadbackRewrite>,
    pub skipped: Vec<MemberNick>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackReport {
    pub rounds: Vec<ReadbackRoundReport>,
    pub raw_answers: Vec<String>,
    pub final_spec: String,
    pub reason: String,
    pub last_rewrite_not_read_back: bool,
    pub convergence: Vec<ReadbackConvergence>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadbackTransition {
    Advance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadbackCaseMaterial {
    pub spec: String,
    pub round: u32,
    pub data: serde_json::Value,
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "readback.interpret" => validation::decode::<ReadbackInterpretation>(body)?.validate(),
        "readback.diff" => validation::decode::<ReadbackLedger>(body)?.validate(),
        "readback.rewrite" => validation::decode::<ReadbackRewrite>(body)?.validate(),
        "readback.report" => {
            validation::decode::<ReadbackReport>(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown readback artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::ReadbackInterpret => validation::schema::<ReadbackInterpretation>(),
        WorkKind::ReadbackDiff => validation::schema::<ReadbackLedger>(),
        WorkKind::ReadbackRewrite => validation::schema::<ReadbackRewrite>(),
        _ => return None,
    })
}
pub fn composition(kind: &WorkKind) -> Option<&'static str> {
    Some(match kind {
        WorkKind::ReadbackInterpret => "readback.interpret",
        WorkKind::ReadbackDiff => "readback.diff",
        WorkKind::ReadbackRewrite => "readback.rewrite",
        _ => return None,
    })
}
/// Only prose consumes the frozen word budget.
pub fn prose_words(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(s) => s.split_whitespace().count(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::Object(o) => o
            .iter()
            .filter(|(k, _)| {
                !["localId", "entryId", "readingId", "sourceLabels"].contains(&k.as_str())
            })
            .map(|(_, v)| prose_words(v))
            .sum(),
        _ => 0,
    }
}
