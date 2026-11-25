//! Typed elimination tournament contracts. Label maps are never judge material.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TournamentStatus {
    Preflight,
    WaitingForEntrants,
    Answering,
    Judging,
    Eliminating,
    Complete,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentSetup {
    pub contest_id: ContestId,
    pub prompt_message_id: MessageId,
    pub prompt: String,
    pub entrants: Vec<MemberNick>,
    pub judge_model: String,
    pub total_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentState {
    pub contest_id: ContestId,
    pub state: TournamentStatus,
    pub state_version: u64,
    pub round: u32,
    pub replay: u8,
    pub survivors: Vec<MemberNick>,
    pub eliminated: Vec<MemberNick>,
    #[schemars(extend("x-required-nullable" = true))]
    pub terminal_reason: Option<String>, // nullable
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentEntry {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub contest_id: ContestId,
    pub round: u32,
    pub replay: u8,
    pub entrant: MemberNick,
    pub outcome: DebateTurnOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TournamentJudgeEntry {
    pub label: String,
    pub score: u8,
    pub critique: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TournamentJudgeResponse {
    pub ranking: Vec<TournamentJudgeEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentRank {
    pub nick: MemberNick,
    pub rank: u32,
    #[schemars(extend("x-required-nullable" = true))]
    pub score: Option<u8>, // nullable
    #[schemars(extend("x-required-nullable" = true))]
    pub critique: Option<String>, // nullable
    pub forfeited: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentRanking {
    pub contest_id: ContestId,
    pub case_id: CaseId,
    pub round: u32,
    pub replay: u8,
    pub ranking: Vec<TournamentRank>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentResult {
    pub contest_id: ContestId,
    pub round: u32,
    pub entrant: MemberNick,
    pub rank: u32,
    pub remaining_before: u32,
    #[schemars(extend("x-required-nullable" = true))]
    pub score: Option<u8>, // nullable
    #[schemars(extend("x-required-nullable" = true))]
    pub critique: Option<String>, // nullable
    pub survived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentPlacement {
    pub nick: MemberNick,
    pub placement: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentChampion {
    pub trophies: u64,
    pub contest_id: ContestId,
    pub champion: MemberNick,
    pub placements: Vec<TournamentPlacement>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TournamentCaseMaterial {
    pub prompt: String,
    pub round: u32,
    pub entries: Vec<TournamentBlindEntry>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TournamentBlindEntry {
    pub label: String,
    pub answer: Option<String>,
}
/// Executor output contains labels only. The router decodes identities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TournamentJudgment {
    #[serde(flatten)]
    pub generation: GenerationIdentity,
    pub contest_id: ContestId,
    pub case_id: CaseId,
    pub round: u32,
    pub replay: u8,
    pub response: TournamentJudgeResponse,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TournamentPreflight {
    pub base: ContestPreflight,
    pub total_rounds: u32,
    pub entry_attempts: u8,
    pub judge_attempts: u8,
    pub replays: u8,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TournamentTransition {
    EntrantsDrained,
    EntriesResolved,
    RankingAccepted,
    EliminationCommitted,
    JudgeExhausted,
    Stop,
}
impl TournamentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
impl TournamentJudgeResponse {
    pub fn validate(&self, labels: &[String]) -> validation::ValidationResult {
        use validation::ensure;
        ensure(
            self.ranking.len() == labels.len() && validation::unique(labels),
            "ranking must include every label",
        )?;
        let supplied: Vec<_> = self.ranking.iter().map(|e| e.label.clone()).collect();
        ensure(
            validation::unique(&supplied) && supplied.iter().all(|l| labels.contains(l)),
            "ranking must be a permutation",
        )?;
        for e in &self.ranking {
            ensure(
                (1..=10).contains(&e.score),
                "score must be an integer from 1 to 10",
            )?;
            ensure(
                !e.critique.trim().is_empty() && !e.critique.contains(['\n', '\r']),
                "critique must be one nonempty line",
            )?;
        }
        Ok(())
    }
    pub fn from_json(raw: &str, labels: &[String]) -> Result<Self, validation::ValidationError> {
        let value = serde_json::from_str(raw).map_err(|_| validation::ValidationError {
            message: "invalid ranking JSON".into(),
        })?;
        let response: Self = validation::decode(&value)?;
        response.validate(labels)?;
        Ok(response)
    }
}
impl TournamentState {
    pub fn validate(&self) -> validation::ValidationResult {
        use validation::ensure;
        ensure(
            self.round > 0
                && self.replay <= 1
                && validation::unique(&self.survivors)
                && validation::unique(&self.eliminated),
            "invalid tournament state",
        )?;
        ensure(
            self.survivors.iter().all(|n| !self.eliminated.contains(n)),
            "survivors and eliminated must be disjoint",
        )?;
        ensure(
            self.survivors.len() + self.eliminated.len() >= 3,
            "tournament needs at least three entrants",
        )?;
        ensure(
            self.state != TournamentStatus::Complete || self.survivors.len() == 1,
            "complete tournament requires one champion",
        )
    }
}
