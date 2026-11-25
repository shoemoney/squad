//! Canonical parser AST.
use crate::{ContestId, DebateId, ErrorCode, EventId, MessageId, Nick};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
} // UTF-8 byte offsets into raw; end exclusive.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VariableName {
    #[serde(rename = "ROUNDS")]
    Rounds,
    #[serde(rename = "GRILL_ROUNDS")]
    GrillRounds,
    #[serde(rename = "CENSUS_PASSES")]
    CensusPasses,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableUpdate {
    pub name: VariableName,
    pub value: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationalInput {
    pub raw: String,
    #[serde(default)]
    pub quote: Vec<EventId>,
    #[serde(default)]
    pub variable_updates: Vec<VariableUpdate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportScope {
    Room,
    Debate,
    Grill,
    Tournament,
    Census,
    Crosscheck,
    Survey,
    Probe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JudgeRecipient {
    #[serde(rename = "judge")]
    Judge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionFirstMode {
    Debate,
    Grill,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlanSource {
    Pasted { text: String },
    Quoted { event_id: EventId },
    LatestReply { nick: Nick },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SurveySource {
    Question { text: String },
    LatestParallelAnswers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingPick {
    pub contest_id: ContestId,
    pub mode: QuestionFirstMode,
    pub eligible: Vec<Nick>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "name",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum SlashAction {
    Set {
        variable: VariableName,
        value: u8,
    },
    Stop {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nick: Option<Nick>,
    },
    Judge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debate_id: Option<DebateId>,
    },
    Regen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<MessageId>,
    },
    Export {
        scope: ExportScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    Clear,
    Who,
    Help,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ParsedInput {
    Message {
        #[serde(flatten)]
        input: ConversationalInput,
        recipients: Vec<Nick>,
        body: String,
    },
    Debate {
        #[serde(flatten)]
        input: ConversationalInput,
        debaters: [Nick; 2],
        rounds: u8,
        instruction: String,
        judge_requested: bool,
        #[serde(default)]
        reply_to: Vec<EventId>,
    },
    Judge {
        #[serde(flatten)]
        input: ConversationalInput,
        recipients: [JudgeRecipient; 1],
        question: String,
    },
    Assignment {
        raw: String,
        updates: Vec<VariableUpdate>,
    },
    Slash {
        raw: String,
        action: SlashAction,
    },
    Tournament {
        #[serde(flatten)]
        input: ConversationalInput,
        entrants: Vec<Nick>,
        prompt: String,
    },
    Grill {
        #[serde(flatten)]
        input: ConversationalInput,
        leader: Nick,
        grillers: Vec<Nick>,
        rounds: u8,
        focus: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_selection: Option<EventId>,
    },
    QuestionFirst {
        #[serde(flatten)]
        input: ConversationalInput,
        mode: QuestionFirstMode,
        candidates: Vec<Nick>,
        question: String,
        rounds: u8,
        judge_requested: bool,
    },
    ContestPick {
        raw: String,
        contest_id: ContestId,
        nicks: Vec<Nick>,
    },
    Census {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<Nick>,
        plan: PlanSource,
        passes: u8,
    },
    Crosscheck {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<Nick>,
        plan: PlanSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal_constraints: Option<String>,
    },
    Survey {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<Nick>,
        source: SurveySource,
    },
    Probe {
        #[serde(flatten)]
        input: ConversationalInput,
        participants: Vec<Nick>,
        brief: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseError {
    pub code: ErrorCode,
    pub message: String,
    pub span: Span,
}

pub type ParseResult = Result<ParsedInput, ParseError>;

/// Effective stored values (or environment seeds when a value is not stored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomVariables {
    pub rounds: u8,
    pub grill_rounds: u8,
    pub census_passes: u8,
}
impl Default for RoomVariables {
    fn default() -> Self {
        Self {
            rounds: 10,
            grill_rounds: 2,
            census_passes: 2,
        }
    }
}
impl VariableName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rounds => "ROUNDS",
            Self::GrillRounds => "GRILL_ROUNDS",
            Self::CensusPasses => "CENSUS_PASSES",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "ROUNDS" => Some(Self::Rounds),
            "GRILL_ROUNDS" => Some(Self::GrillRounds),
            "CENSUS_PASSES" => Some(Self::CensusPasses),
            _ => None,
        }
    }
    pub fn value(&self, vars: &RoomVariables) -> u8 {
        match self {
            Self::Rounds => vars.rounds,
            Self::GrillRounds => vars.grill_rounds,
            Self::CensusPasses => vars.census_passes,
        }
    }
    pub fn set(&self, vars: &mut RoomVariables, value: u8) {
        match self {
            Self::Rounds => vars.rounds = value,
            Self::GrillRounds => vars.grill_rounds = value,
            Self::CensusPasses => vars.census_passes = value,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubmissionMetadata {
    pub quote: Vec<EventId>,
    pub reply_to: Vec<EventId>,
}

impl ParsedInput {
    pub fn raw(&self) -> &str {
        match self {
            Self::Assignment { raw, .. }
            | Self::Slash { raw, .. }
            | Self::ContestPick { raw, .. } => raw,
            _ => &self.conversational().unwrap().raw,
        }
    }
    pub fn conversational(&self) -> Option<&ConversationalInput> {
        match self {
            Self::Message { input, .. }
            | Self::Debate { input, .. }
            | Self::Judge { input, .. }
            | Self::Tournament { input, .. }
            | Self::Grill { input, .. }
            | Self::QuestionFirst { input, .. }
            | Self::Census { input, .. }
            | Self::Crosscheck { input, .. }
            | Self::Survey { input, .. }
            | Self::Probe { input, .. } => Some(input),
            _ => None,
        }
    }
    pub fn is_contest(&self) -> bool {
        matches!(
            self,
            Self::Debate { .. }
                | Self::Tournament { .. }
                | Self::Grill { .. }
                | Self::QuestionFirst { .. }
                | Self::Census { .. }
                | Self::Crosscheck { .. }
                | Self::Survey { .. }
                | Self::Probe { .. }
        )
    }
}

impl ParsedInput {
    /// Bind a pick control request to the caller's independently allocated request identity.
    pub fn contest_pick_request(&self, request_id: EventId) -> Option<crate::Request> {
        match self {
            Self::ContestPick {
                contest_id, nicks, ..
            } => Some(crate::Request::ContestPick {
                request_id,
                contest_id: *contest_id,
                nicks: nicks.clone(),
            }),
            _ => None,
        }
    }
}

// This table is the single registration point for recognized mode verbs.
macro_rules! verbs {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Verb { $($variant),+ }
        impl Verb {
            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}
verbs! {
    Debate => "debate", Tournament => "tournament", Grill => "grill",
    Census => "census", Crosscheck => "crosscheck", Survey => "survey", Probe => "probe",
    Review => "review", Reveal => "reveal", Vote => "vote", Summarize => "summarize",
}
impl Verb {
    pub fn reserved(self) -> bool {
        matches!(
            self,
            Self::Review | Self::Reveal | Self::Vote | Self::Summarize
        )
    }
}
