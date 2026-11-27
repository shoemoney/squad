//! negotiate contracts; enabled only after the mode implementation lands.
use crate::validation::{ValidationResult, ensure, unique};
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const ENABLED: bool = false;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NegotiateStatus {
    Preflight,
    WaitingForParticipants,
    Mapping,
    Briefing,
    Packaging,
    Rehearsing,
    WritingPlaybook,
    Reporting,
    Complete,
    Stopped,
    Failed,
}
impl NegotiateStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateState {
    pub contest_id: ContestId,
    pub state_version: u64,
    pub state: NegotiateStatus,
    #[schemars(extend("x-required-nullable" = true))]
    pub step_id: Option<StepId>,
    pub round: u32,
    pub artifact_event_ids: Vec<EventId>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<Text>,
}
impl NegotiateState {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            self.state_version > 0 && unique(&self.artifact_event_ids),
            "invalid negotiate state",
        )?;
        ensure(
            self.state.is_terminal() == self.terminal_reason.is_some(),
            "terminal reason mismatch",
        )?;
        ensure(self.round <= 6, "invalid negotiate round")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NegotiateTransition {
    Advance,
    Stop,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateSetup {
    pub pack_version: String,
    pub our_side: String,
    pub counterpart: String,
    pub participants: Vec<MemberNick>,
    pub models: std::collections::BTreeMap<String, ExecutorBinding>,
    pub prompts: serde_json::Value,
    pub rounds: u32,
    pub tables: Vec<NegotiateTable>,
    pub room_words: u32,
    pub plan_words: u32,
    pub redaction_tokens: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateInterest {
    pub text: String,
    pub basis: NegotiateBasis,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateZopa {
    pub issue: String,
    pub our_limit: String,
    pub their_likely_limit: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateMap {
    #[schemars(length(max = 12))]
    pub our_interests: Vec<String>,
    pub our_batna: String,
    #[schemars(length(max = 12))]
    pub counterpart_interests: Vec<NegotiateInterest>,
    pub counterpart_batna: String,
    #[schemars(length(max = 12))]
    pub zopa: Vec<NegotiateZopa>,
    #[schemars(length(max = 12))]
    pub tactics: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateReservation {
    pub issue: String,
    pub limit: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateCounterpartBrief {
    pub persona: String,
    #[schemars(length(max = 12))]
    pub priorities: Vec<String>,
    #[schemars(length(max = 12))]
    pub reservations: Vec<NegotiateReservation>,
    pub hidden_constraint: String,
    pub walk_away_rule: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateBrief {
    pub strategy_sheet: String,
    pub public_scenario: String,
    #[schemars(length(max = 3))]
    pub counterpart_briefs: Vec<NegotiateCounterpartBrief>,
    #[schemars(length(max = 12))]
    pub assumptions_to_confirm: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiatePackage {
    pub opener: String,
    #[schemars(length(max = 12))]
    pub gives: Vec<String>,
    #[schemars(length(max = 12))]
    pub gets: Vec<String>,
    pub walk_away: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateMove {
    pub message: String,
    pub offer: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateCounter {
    pub message: String,
    pub response: NegotiateResponse,
    #[schemars(extend("x-required-nullable" = true))]
    pub counter_offer: Option<String>,
    #[schemars(extend("x-required-nullable" = true))]
    pub violated_interest: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateConcession {
    pub give: String,
    pub only_for: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateObjection {
    pub objection: String,
    pub response: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiatePlaybookZopa {
    pub issue: String,
    pub our_limit: String,
    pub their_modelled_limit: String,
    pub overlap: NegotiateOverlap,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateTableNote {
    pub table: u32,
    pub left_on_table: String,
    #[schemars(length(max = 12))]
    pub unrealistic_concessions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiatePlaybook {
    pub opening: String,
    #[schemars(length(max = 12))]
    pub anchors: Vec<String>,
    #[schemars(length(max = 12))]
    pub concession_ladder: Vec<NegotiateConcession>,
    #[schemars(length(max = 12))]
    pub objections: Vec<NegotiateObjection>,
    pub walk_away_line: String,
    #[schemars(length(max = 12))]
    pub zopa_map: Vec<NegotiatePlaybookZopa>,
    #[schemars(length(max = 12))]
    pub questions_to_ask: Vec<String>,
    #[schemars(length(max = 3))]
    pub table_notes: Vec<NegotiateTableNote>,
    #[schemars(length(max = 12))]
    pub depends_on_assumptions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateTurn {
    pub table: u32,
    pub round: u32,
    pub actor: MemberNick,
    pub counterpart: bool,
    pub body: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateTable {
    pub table: u32,
    pub negotiator: MemberNick,
    pub counterpart: MemberNick,
    pub rounds_used: u32,
    #[schemars(extend("x-required-nullable" = true))]
    pub outcome: Option<NegotiateOutcome>,
    pub turns: Vec<NegotiateTurn>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateReport {
    pub setup: NegotiateSetup,
    pub maps: std::collections::BTreeMap<MemberNick, NegotiateMap>,
    #[schemars(extend("x-required-nullable" = true))]
    pub brief: Option<NegotiateBrief>,
    pub packages: std::collections::BTreeMap<MemberNick, NegotiatePackage>,
    pub tables: Vec<NegotiateTable>,
    #[schemars(extend("x-required-nullable" = true))]
    pub playbook: Option<NegotiatePlaybook>,
    pub counts: std::collections::BTreeMap<String, u32>,
    pub skipped: Vec<String>,
    pub no_rehearsal_evidence: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NegotiateCaseMaterial {
    pub step: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NegotiateBasis {
    Supplied,
    Inferred,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NegotiateResponse {
    Accept,
    Reject,
    Counter,
    WalkAway,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NegotiateOverlap {
    Yes,
    No,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NegotiateOutcome {
    Deal,
    NoDeal,
    Unfinished,
    Abandoned,
}

pub fn table_count(m: usize) -> usize {
    3.min(m / 2)
}
pub fn calls(m: usize, r: u32) -> u32 {
    let p = table_count(m) as u32;
    2 * m as u32 - p + 2 * p * r + 2
}
pub fn attempts(m: usize, r: u32) -> u32 {
    2 * calls(m, r) + 2
}
pub fn words(s: &str) -> usize {
    s.split_whitespace().count()
}
fn prose_words(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::String(s) => words(s),
        serde_json::Value::Array(a) => a.iter().map(prose_words).sum(),
        serde_json::Value::Object(o) => o
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "response" | "basis" | "overlap"))
            .map(|(_, v)| prose_words(v))
            .sum(),
        _ => 0,
    }
}
pub fn validate_artifact(kind: &str, body: &serde_json::Value) -> ValidationResult {
    use crate::validation::decode;
    match kind {
        "negotiate.map" => {
            let v: NegotiateMap = decode(body)?;
            ensure(
                !v.our_batna.trim().is_empty() && !v.our_interests.is_empty(),
                "empty interest map",
            )?;
        }
        "negotiate.brief" => {
            let v: NegotiateBrief = decode(body)?;
            ensure(
                (2..=3).contains(&v.counterpart_briefs.len())
                    && !v.strategy_sheet.trim().is_empty()
                    && !v.public_scenario.trim().is_empty(),
                "invalid counterpart brief count or empty briefing",
            )?;
            ensure(
                words(&v.strategy_sheet) <= 600
                    && words(&v.public_scenario) <= 300
                    && v.counterpart_briefs.iter().all(|b| {
                        prose_words(&serde_json::to_value(b).expect("brief")) <= 300
                            && !b.reservations.is_empty()
                            && !b.walk_away_rule.trim().is_empty()
                    }),
                "brief projection budget or reservations invalid",
            )?;
        }
        "negotiate.package" => {
            let v: NegotiatePackage = decode(body)?;
            ensure(
                !v.opener.trim().is_empty() && !v.walk_away.trim().is_empty(),
                "empty package",
            )?;
        }
        "negotiate.move" => {
            let v: NegotiateMove = decode(body)?;
            ensure(
                !v.message.trim().is_empty() && !v.offer.trim().is_empty(),
                "empty move",
            )?;
        }
        "negotiate.counter" => {
            let v: NegotiateCounter = decode(body)?;
            ensure(
                !v.message.trim().is_empty()
                    && (v.response != NegotiateResponse::Counter
                        || v.counter_offer
                            .as_ref()
                            .is_some_and(|s| !s.trim().is_empty())),
                "counter needs message and counterOffer",
            )?;
        }
        "negotiate.playbook" => {
            let v: NegotiatePlaybook = decode(body)?;
            ensure(
                !v.opening.trim().is_empty()
                    && !v.walk_away_line.trim().is_empty()
                    && unique(&v.table_notes.iter().map(|n| n.table).collect::<Vec<_>>()),
                "empty playbook or duplicate table notes",
            )?;
        }
        "negotiate.turn" => {
            let v: NegotiateTurn = decode(body)?;
            ensure(
                (1..=3).contains(&v.table) && (1..=6).contains(&v.round),
                "invalid turn table or round",
            )?;
            validate_artifact(
                if v.counterpart {
                    "negotiate.counter"
                } else {
                    "negotiate.move"
                },
                &v.body,
            )?;
        }
        "negotiate.roles" => {
            let v: NegotiateSetup = decode(body)?;
            v.validate()?;
        }
        "negotiate.report" => {
            let _: NegotiateReport = decode(body)?;
        }
        _ => return ensure(false, "unknown negotiate artifact"),
    }
    Ok(())
}
impl NegotiateSetup {
    pub fn validate(&self) -> ValidationResult {
        ensure(
            (4..=12).contains(&self.participants.len())
                && unique(&self.participants)
                && (1..=6).contains(&self.rounds),
            "invalid negotiate participants or rounds",
        )?;
        let roles = self
            .tables
            .iter()
            .flat_map(|t| [t.negotiator.clone(), t.counterpart.clone()])
            .collect::<Vec<_>>();
        ensure(
            self.tables.len() == table_count(self.participants.len())
                && unique(&roles)
                && roles.iter().all(|n| self.participants.contains(n))
                && self
                    .tables
                    .iter()
                    .enumerate()
                    .all(|(i, t)| t.table == i as u32 + 1),
            "invalid role draw",
        )
    }
}
pub fn validate_output(
    kind: &WorkKind,
    data: &serde_json::Value,
    body: &serde_json::Value,
) -> ValidationResult {
    let name = match kind {
        WorkKind::NegotiateMap => "negotiate.map",
        WorkKind::NegotiateBrief => "negotiate.brief",
        WorkKind::NegotiatePackage => "negotiate.package",
        WorkKind::NegotiateMove => "negotiate.move",
        WorkKind::NegotiateCounter => "negotiate.counter",
        WorkKind::NegotiatePlaybook => "negotiate.playbook",
        _ => return ensure(false, "invalid negotiate work"),
    };
    validate_artifact(name, body)?;
    ensure(
        prose_words(body) <= data["wordLimit"].as_u64().unwrap_or(200) as usize,
        "response exceeds word budget",
    )?;
    if *kind == WorkKind::NegotiateBrief {
        ensure(
            body["counterpartBriefs"]
                .as_array()
                .is_some_and(|a| Some(a.len() as u64) == data["tableCount"].as_u64()),
            "wrong number of counterpart briefs",
        )?;
    }
    if *kind == WorkKind::NegotiatePlaybook {
        let p: NegotiatePlaybook = crate::validation::decode(body)?;
        let n = data["tableCount"].as_u64().unwrap_or(0) as u32;
        ensure(
            p.table_notes.len() == n as usize
                && p.table_notes.iter().all(|t| (1..=n).contains(&t.table)),
            "playbook must cover supplied tables exactly once",
        )?;
        if data["hasInferredInterests"] == true {
            ensure(
                !p.depends_on_assumptions.is_empty(),
                "inferred interests require assumption warnings",
            )?;
        }
    }
    Ok(())
}
pub fn output_schema(kind: &WorkKind) -> Option<serde_json::Value> {
    use crate::validation::schema;
    Some(match kind {
        WorkKind::NegotiateMap => schema::<NegotiateMap>(),
        WorkKind::NegotiateBrief => schema::<NegotiateBrief>(),
        WorkKind::NegotiatePackage => schema::<NegotiatePackage>(),
        WorkKind::NegotiateMove => schema::<NegotiateMove>(),
        WorkKind::NegotiateCounter => schema::<NegotiateCounter>(),
        WorkKind::NegotiatePlaybook => schema::<NegotiatePlaybook>(),
        _ => return None,
    })
}
impl NegotiateTurn {
    pub fn validate_against(&self, table: &NegotiateTable, max_rounds: u32) -> ValidationResult {
        ensure(
            self.table == table.table
                && (1..=max_rounds).contains(&self.round)
                && self.actor
                    == if self.counterpart {
                        table.counterpart.clone()
                    } else {
                        table.negotiator.clone()
                    },
            "turn table, round or role mismatch",
        )?;
        validate_artifact(
            if self.counterpart {
                "negotiate.counter"
            } else {
                "negotiate.move"
            },
            &self.body,
        )
    }
}
