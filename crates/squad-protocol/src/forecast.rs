//! Frozen forecast contracts and strict generated-output validation.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
// Enable after the integrator repairs shared judge dispatch (see mode docs).
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ForecastStatus {
    Preflight,
    WaitingForParticipants,
    Framing,
    Forecasting,
    Revising,
    Aggregating,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl ForecastStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: ForecastStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl ForecastState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid forecast state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid forecast round")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum ForecastTransition { Advance }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all="lowercase")]
pub enum ForecastDirection { Up, Down }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastSetup {
pub contest_id: ContestId,
pub question: Text,
pub background: String,
pub participants: Vec<MemberNick>,
pub models: std::collections::BTreeMap<String, ExecutorBinding>,
pub prompt_refs: std::collections::BTreeMap<String, PromptRef>,
pub room_words: u32,
pub plan_words: u32,
pub redaction_tokens: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastFrame {
pub question: Text,
pub resolution_criteria: Text,
pub resolution_date: String,
pub resolution_source: Text,
pub void_rule: Text,
pub forecastable: bool,
pub reason: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastBaseRate {
pub reference_class: Text,
#[schemars(extend("x-required-nullable" = true))]
#[schemars(range(min = 0, max = 100))]
pub rate_pct: Option<u8>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastFactor {
pub direction: ForecastDirection,
pub text: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastEstimate {
#[schemars(range(min = 1, max = 99))]
pub probability_pct: u8,
pub base_rate: ForecastBaseRate,
#[schemars(length(min = 3, max = 3))]
pub factors: Vec<ForecastFactor>,
pub rationale: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastBundleEntry {
pub label: String,
pub reference_class: String,
pub factors: Vec<ForecastFactor>,
pub rationale: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastBundle {
pub recipient: MemberNick,
pub entries: Vec<ForecastBundleEntry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastRevision {
#[schemars(range(min = 1, max = 99))]
pub probability_pct: u8,
pub change_reason: Text,
#[schemars(length(max = 11))]
pub cited_labels: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastAggregate {
pub probabilities_pct: Vec<u8>,
pub sorted_log_odds: Vec<f64>,
pub k: usize,
pub trimmed_log_odds: Vec<f64>,
pub l: f64,
pub p: f64,
pub s: f64,
pub low: f64,
pub high: f64,
pub minimum_pct: u8,
pub maximum_pct: u8,
#[schemars(extend("x-required-nullable" = true))]
pub split: Option<ForecastSplit>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastCrux {
pub text: Text,
#[schemars(length(max = 12))]
pub higher_labels: Vec<String>,
#[schemars(length(max = 12))]
pub lower_labels: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastSignpost {
pub observable: Text,
pub direction: ForecastDirection,
pub by_when: Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastCruxes {
#[schemars(length(max = 5))]
pub cruxes: Vec<ForecastCrux>,
#[schemars(length(max = 6))]
pub signposts: Vec<ForecastSignpost>,
#[schemars(length(max = 5))]
pub shared_assumptions: Vec<Text>,
#[schemars(length(max = 5))]
pub weak_evidence: Vec<Text>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastMemberResult {
pub member: MemberNick,
pub label: String,
pub first: ForecastEstimate,
#[schemars(extend("x-required-nullable" = true))]
pub revision: Option<ForecastRevision>,
pub final_probability_pct: u8,
pub not_revised: bool,
pub uncited_move: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastReport {
pub contest_id: ContestId,
pub frame: ForecastFrame,
pub first_aggregate: ForecastAggregate,
pub final_aggregate: ForecastAggregate,
pub members: Vec<ForecastMemberResult>,
#[schemars(extend("x-required-nullable" = true))]
pub cruxes: Option<ForecastCruxes>,
pub skipped: Vec<MemberNick>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastCaseMaterial {
pub frame: ForecastFrame,
pub forecasts: serde_json::Value,
pub first_aggregate: ForecastAggregate,
pub final_aggregate: ForecastAggregate,
}

impl ForecastSetup {
    pub fn normal_calls(&self) -> u32 { 2 * self.participants.len() as u32 + 2 }
    pub fn reserved_attempts(&self) -> u32 { 4 * self.participants.len() as u32 + 6 }
    pub fn validate(&self) -> ValidationResult {
        ensure((3..=12).contains(&self.participants.len()) && unique(&self.participants), "forecast needs 3 to 12 distinct participants")?;
        ensure(self.models.contains_key("judge") && self.participants.iter().all(|n| self.models.contains_key(n.as_str())), "missing forecast binding")
    }
}
impl ForecastFrame {
    pub fn validate(&self) -> ValidationResult {
        let d = self.resolution_date.as_bytes();
        ensure(d.len()==10 && self.resolution_date.is_ascii() && d[4]==b'-' && d[7]==b'-', "resolutionDate must be YYYY-MM-DD")?;
        let year = self.resolution_date[..4].parse::<u32>().unwrap_or(0);
        let month = self.resolution_date[5..7].parse::<u32>().unwrap_or(0);
        let day = self.resolution_date[8..].parse::<u32>().unwrap_or(0);
        let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month { 2 => if leap {29} else {28}, 4|6|9|11 => 30, 1|3|5|7|8|10|12 =>31, _=>0 };
        ensure(year > 0 && day > 0 && day <= days, "invalid resolution date")
    }
}
impl ForecastEstimate {
    pub fn validate(&self) -> ValidationResult {
        ensure((1..=99).contains(&self.probability_pct), "probabilityPct must be 1 to 99")?;
        ensure(self.base_rate.rate_pct.is_none_or(|p| p<=100), "invalid base rate")?;
        ensure(self.factors.len()==3, "exactly three factors required")
    }
}
impl ForecastRevision {
    pub fn validate_against(&self, labels: &[String]) -> ValidationResult {
        ensure((1..=99).contains(&self.probability_pct), "probabilityPct must be 1 to 99")?;
        ensure(unique(&self.cited_labels) && self.cited_labels.iter().all(|l| labels.contains(l)), "unknown or repeated cited label")
    }
}
impl ForecastCruxes {
    pub fn validate_against(&self, labels: &[String]) -> ValidationResult {
        ensure(self.cruxes.len()<=5 && self.signposts.len()<=6 && self.shared_assumptions.len()<=5 && self.weak_evidence.len()<=5, "too many cruxes or signposts")?;
        for c in &self.cruxes {
            ensure(unique(&c.higher_labels) && unique(&c.lower_labels) && c.higher_labels.iter().chain(&c.lower_labels).all(|l| labels.contains(l)) && !c.higher_labels.iter().any(|l| c.lower_labels.contains(l)), "invalid crux labels")?;
        }
        Ok(())
    }
}
pub fn prose_words(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(s) => s.split_whitespace().count(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::Object(o) => o.iter().filter(|(k,_)| !["direction","citedLabels","higherLabels","lowerLabels","resolutionDate"].contains(&k.as_str())).map(|(_,v)|prose_words(v)).sum(),
        _=>0,
    }
}
pub fn validate_output(kind: &WorkKind, data: &serde_json::Value, output: &serde_json::Value) -> ValidationResult {
    let labels = data["labels"].as_array().map(|a|a.iter().filter_map(|v|v.as_str().map(str::to_owned)).collect::<Vec<_>>()).unwrap_or_default();
    let words = data["wordLimit"].as_u64().unwrap_or(500) as usize;
    ensure(prose_words(output)<=words, "forecast prose word budget exceeded")?;
    match kind {
        WorkKind::ForecastFrame => validation::decode::<ForecastFrame>(output)?.validate(),
        WorkKind::ForecastEstimate => validation::decode::<ForecastEstimate>(output)?.validate(),
        WorkKind::ForecastRevise => validation::decode::<ForecastRevision>(output)?.validate_against(&labels),
        WorkKind::ForecastCruxes => validation::decode::<ForecastCruxes>(output)?.validate_against(&labels),
        _=>ensure(false,"not forecast work"),
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "forecast.setup" => validation::decode::<ForecastSetup>(body)?.validate(),
        "forecast.frame" => validation::decode::<ForecastFrame>(body)?.validate(),
        "forecast.estimate" => validation::decode::<ForecastEstimate>(body)?.validate(),
        "forecast.revise" => { let r=validation::decode::<ForecastRevision>(body)?; r.validate_against(&r.cited_labels) },
        "forecast.cruxes" => { let c=validation::decode::<ForecastCruxes>(body)?; let labels=c.cruxes.iter().flat_map(|c|c.higher_labels.iter().chain(&c.lower_labels)).cloned().collect::<Vec<_>>(); c.validate_against(&labels) },
        "forecast.bundle" => { validation::decode::<ForecastBundle>(body)?; Ok(()) },
        "forecast.aggregate" => { validation::decode::<ForecastAggregate>(body)?; Ok(()) },
        "forecast.report" => { validation::decode::<ForecastReport>(body)?; Ok(()) },
        _=>ensure(false,"unknown forecast artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    Some(match kind {
        WorkKind::ForecastFrame => validation::schema::<ForecastFrame>(),
        WorkKind::ForecastEstimate => validation::schema::<ForecastEstimate>(),
        WorkKind::ForecastRevise => validation::schema::<ForecastRevision>(),
        WorkKind::ForecastCruxes => validation::schema::<ForecastCruxes>(),
        _=>return None,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ForecastSplit { pub lower:f64, pub higher:f64, pub cut:usize }
