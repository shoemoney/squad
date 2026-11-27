//! rank contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RankStatus {
    Preflight,
    WaitingForParticipants,
    Comparing,
    Aggregating,
    Explaining,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl RankStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: RankStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl RankState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid rank state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round == 0, "invalid rank round")
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum RankTransition {
    Advance,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankItem {
    pub item_id: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankPair {
    pub pair_id: String,
    pub left: usize,
    pub right: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankDeal {
    pub pair_id: String,
    pub a: usize,
    pub b: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankSetup {
    pub criterion: String,
    pub items: Vec<RankItem>,
    pub participants: Vec<MemberNick>,
    pub pairs: Vec<RankPair>,
    pub votes_per_pair: usize,
    pub deal: std::collections::BTreeMap<MemberNick, Vec<RankDeal>>,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompt_refs: std::collections::BTreeMap<String, PromptRef>,
    pub room_words: u32,
}
impl RankSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            !self.criterion.trim().is_empty() && (3..=40).contains(&self.items.len()),
            "rank requires a criterion and 3 to 40 items",
        )?;
        ensure(
            self.items
                .iter()
                .all(|i| !i.text.trim().is_empty() && i.text.split_whitespace().count() <= 60),
            "rank items must contain 1 to 60 words",
        )?;
        ensure(
            self.participants.len() >= 2 && unique(&self.participants),
            "rank requires distinct participants",
        )?;
        ensure(
            unique(&self.items.iter().map(|i| &i.item_id).collect::<Vec<_>>()),
            "duplicate item id",
        )?;
        let n = self.items.len();
        ensure(
            self.pairs.len() == (n * (n - 1) / 2).min(3 * n)
                && unique(&self.pairs.iter().map(|p| &p.pair_id).collect::<Vec<_>>()),
            "invalid pair design",
        )?;
        ensure(
            self.pairs.iter().all(|p| p.left < p.right && p.right < n),
            "invalid pair endpoint",
        )?;
        ensure(
            unique(
                &self
                    .pairs
                    .iter()
                    .map(|p| (p.left, p.right))
                    .collect::<Vec<_>>(),
            ),
            "duplicate pair",
        )?;
        let v = (60 * self.participants.len() / self.pairs.len())
            .clamp(1, 3)
            .min(self.participants.len());
        ensure(
            self.votes_per_pair == v && self.deal.len() == self.participants.len(),
            "invalid vote count",
        )?;
        let mut loads = vec![];
        for nick in &self.participants {
            let deals = self
                .deal
                .get(nick)
                .ok_or_else(|| validation::ValidationError {
                    message: "missing member deal".into(),
                })?;
            ensure(
                deals.len() <= 60 && unique(&deals.iter().map(|d| &d.pair_id).collect::<Vec<_>>()),
                "invalid member deal",
            )?;
            for d in deals {
                ensure(
                    self.pairs.iter().any(|p| {
                        p.pair_id == d.pair_id
                            && ((d.a, d.b) == (p.left, p.right) || (d.b, d.a) == (p.left, p.right))
                    }),
                    "deal endpoint mismatch",
                )?;
            }
            loads.push(deals.len());
        }
        ensure(
            loads.iter().max().unwrap() - loads.iter().min().unwrap() <= 1,
            "unbalanced deal",
        )?;
        for p in &self.pairs {
            ensure(
                self.deal
                    .values()
                    .filter(|ds| ds.iter().any(|d| d.pair_id == p.pair_id))
                    .count()
                    == v,
                "pair replication mismatch",
            )?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RankWinner {
    A,
    B,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankChoice {
    pub pair_id: String,
    pub winner: RankWinner,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankBatch {
    #[schemars(length(min = 1, max = 60))]
    pub choices: Vec<RankChoice>,
}
impl RankBatch {
    pub fn validate(&self, assigned: &[String]) -> ValidationResult {
        ensure(
            self.choices.len() == assigned.len()
                && unique(&self.choices.iter().map(|c| &c.pair_id).collect::<Vec<_>>()),
            "return exactly one choice for every assigned pair",
        )?;
        ensure(
            self.choices.iter().all(|c| {
                assigned.contains(&c.pair_id)
                    && !c.reason.trim().is_empty()
                    && c.reason.split_whitespace().count() <= 12
            }),
            "unknown pair or reason exceeds 12 words",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankGap {
    pub higher: usize,
    pub lower: usize,
    pub gap: f64,
    pub probability: f64,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankCycle {
    pub cycle_id: String,
    pub items: [usize; 3],
    pub margin: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankTally {
    pub wins: Vec<Vec<u32>>,
    pub strengths: Vec<f64>,
    pub scores: Vec<f64>,
    pub order: Vec<usize>,
    pub gaps: Vec<RankGap>,
    pub cycles: Vec<RankCycle>,
    pub cycle_count: usize,
    pub split_pairs: Vec<String>,
    pub components: Vec<Vec<usize>>,
    pub judgments: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankTop {
    pub item_id: String,
    pub why: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankCycleReading {
    pub cycle_id: String,
    pub reading: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankExplanation {
    #[schemars(length(min = 3, max = 5))]
    pub top: Vec<RankTop>,
    #[schemars(length(max = 10))]
    pub cycles: Vec<RankCycleReading>,
    pub caveats: String,
}
impl RankExplanation {
    pub fn validate(&self, top: &[String], cycles: &[String], words: usize) -> ValidationResult {
        ensure(
            self.top.len() == top.len()
                && unique(&self.top.iter().map(|x| &x.item_id).collect::<Vec<_>>())
                && self
                    .top
                    .iter()
                    .all(|x| top.contains(&x.item_id) && !x.why.trim().is_empty()),
            "explain every top item exactly once",
        )?;
        ensure(
            self.cycles.len() == cycles.len()
                && unique(&self.cycles.iter().map(|x| &x.cycle_id).collect::<Vec<_>>())
                && self
                    .cycles
                    .iter()
                    .all(|x| cycles.contains(&x.cycle_id) && !x.reading.trim().is_empty()),
            "explain every reported cycle exactly once",
        )?;
        ensure(
            prose_words(&serde_json::to_value(self).expect("explanation")) <= words,
            "explanation exceeds word budget",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankReport {
    pub setup: RankSetup,
    pub batches: std::collections::BTreeMap<MemberNick, RankBatch>,
    #[schemars(extend("x-required-nullable" = true))]
    pub tally: Option<RankTally>,
    #[schemars(extend("x-required-nullable" = true))]
    pub explanation: Option<RankExplanation>,
    pub skipped: Vec<MemberNick>,
    pub reason: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RankCaseMaterial {
    pub criterion: String,
    pub items: Vec<RankItem>,
    pub tally: RankTally,
}
pub fn composition(kind: &WorkKind) -> Option<&'static str> {
    match kind {
        WorkKind::RankCompare => Some("rank.compare"),
        WorkKind::RankExplain => Some("rank.explain"),
        _ => None,
    }
}
pub fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Object(m) => m
            .iter()
            .map(|(k, v)| {
                if matches!(k.as_str(), "reason" | "why" | "reading" | "caveats") {
                    v.as_str().map_or(0, |s| s.split_whitespace().count())
                } else {
                    prose_words(v)
                }
            })
            .sum(),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        _ => 0,
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    match kind {
        "rank.compare" => {
            let b: RankBatch = validation::decode(body)?;
            b.validate(
                &b.choices
                    .iter()
                    .map(|c| c.pair_id.clone())
                    .collect::<Vec<_>>(),
            )
        }
        "rank.explain" => {
            let e: RankExplanation = validation::decode(body)?;
            e.validate(
                &e.top.iter().map(|x| x.item_id.clone()).collect::<Vec<_>>(),
                &e.cycles
                    .iter()
                    .map(|x| x.cycle_id.clone())
                    .collect::<Vec<_>>(),
                usize::MAX, // Frozen room budget is checked against the assigned input.
            )
        }
        "rank.tally" => {
            let _: RankTally = validation::decode(body)?;
            Ok(())
        }
        "rank.report" => {
            let _: RankReport = validation::decode(body)?;
            Ok(())
        }
        _ => ensure(false, "unknown rank artifact"),
    }
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    match kind {
        WorkKind::RankCompare => Some(validation::schema::<RankBatch>()),
        WorkKind::RankExplain => Some(validation::schema::<RankExplanation>()),
        _ => None,
    }
}
