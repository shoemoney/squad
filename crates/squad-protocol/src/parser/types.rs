//! Canonical parser AST.
use crate::{CellId, ContestId, DebateId, ErrorCode, EventId, MessageId, Nick};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
} // UTF-8 byte offsets into raw; end exclusive.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VariableName {
#[serde(rename = "NEGOTIATE_ROUNDS")]
NegotiateRounds,
#[serde(rename = "READBACK_ROUNDS")]
ReadbackRounds,
#[serde(rename = "DEBUG_ROUNDS")]
DebugRounds,

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
#[serde(rename = "forge")]
Forge,
#[serde(rename = "forecast")]
Forecast,
#[serde(rename = "decide")]
Decide,
#[serde(rename = "negotiate")]
Negotiate,
#[serde(rename = "blow_it_up")]
BlowItUp,
#[serde(rename = "readback")]
Readback,
#[serde(rename = "debug")]
Debugging,
#[serde(rename = "trace")]
Trace,
#[serde(rename = "rank")]
Rank,

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
    Answer { text: String },
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
        id: Option<ContestId>,
    },
    Continue {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan: Option<String>,
    },
    Accept {
        cell_ids: Vec<CellId>,
        reason: String,
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
#[serde(rename = "forge")]
Forge { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, brief: String, constraints: Option<String> },
#[serde(rename = "forecast")]
Forecast { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, question: String, background: Option<String> },
#[serde(rename = "decide")]
Decide { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, decision: String, options: Vec<String> },
#[serde(rename = "negotiate")]
Negotiate { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, our_side: String, counterpart: Option<String>, rounds: u8 },
#[serde(rename = "blow_it_up")]
BlowItUp { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, source: PlanSource, scope: Option<String> },
#[serde(rename = "readback")]
Readback { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, source: PlanSource, rounds: u8 },
#[serde(rename = "debug")]
Debugging { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, brief: String, limits: Option<String>, rounds: u8 },
#[serde(rename = "trace")]
Trace { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, question: String, sources: Vec<TraceSource> },
#[serde(rename = "rank")]
Rank { #[serde(flatten)] input: ConversationalInput, participants: Vec<Nick>, criterion: String, items: Vec<String> },
HumanChoices { raw: String, contest_id: ContestId, text: String },

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
pub negotiate_rounds: u8,
pub readback_rounds: u8,
pub debug_rounds: u8,

    pub rounds: u8,
    pub grill_rounds: u8,
    pub census_passes: u8,
}
impl Default for RoomVariables {
    fn default() -> Self {
        Self {
            negotiate_rounds: 3,
            readback_rounds: 2,
            debug_rounds: 3,
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
Self::NegotiateRounds => "NEGOTIATE_ROUNDS",
Self::ReadbackRounds => "READBACK_ROUNDS",
Self::DebugRounds => "DEBUG_ROUNDS",

            Self::GrillRounds => "GRILL_ROUNDS",
            Self::CensusPasses => "CENSUS_PASSES",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "ROUNDS" => Some(Self::Rounds),
"NEGOTIATE_ROUNDS" => Some(Self::NegotiateRounds),
"READBACK_ROUNDS" => Some(Self::ReadbackRounds),
"DEBUG_ROUNDS" => Some(Self::DebugRounds),

            "GRILL_ROUNDS" => Some(Self::GrillRounds),
            "CENSUS_PASSES" => Some(Self::CensusPasses),
            _ => None,
        }
    }
    pub fn value(&self, vars: &RoomVariables) -> u8 {
        match self {
            Self::Rounds => vars.rounds,
Self::NegotiateRounds => vars.negotiate_rounds,
Self::ReadbackRounds => vars.readback_rounds,
Self::DebugRounds => vars.debug_rounds,

            Self::GrillRounds => vars.grill_rounds,
            Self::CensusPasses => vars.census_passes,
        }
    }
    pub fn set(&self, vars: &mut RoomVariables, value: u8) {
        match self {
            Self::Rounds => vars.rounds = value,
Self::NegotiateRounds => vars.negotiate_rounds = value,
Self::ReadbackRounds => vars.readback_rounds = value,
Self::DebugRounds => vars.debug_rounds = value,

            Self::GrillRounds => vars.grill_rounds = value,
            Self::CensusPasses => vars.census_passes = value,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubmissionMeta {
    pub quote: Vec<EventId>,
    pub reply_to: Vec<EventId>,
}

impl ParsedInput {
    pub fn raw(&self) -> &str {
        match self {
            Self::Assignment { raw, .. }
            | Self::Slash { raw, .. }
            | Self::ContestPick { raw, .. } | Self::HumanChoices { raw, .. } => raw,
            _ => &self.conversational().unwrap().raw,
        }
    }
    pub fn conversational(&self) -> Option<&ConversationalInput> {
        match self {
            Self::Forge { input, .. } |
            Self::Forecast { input, .. } |
            Self::Decide { input, .. } |
            Self::Negotiate { input, .. } |
            Self::BlowItUp { input, .. } |
            Self::Readback { input, .. } |
            Self::Debugging { input, .. } |
            Self::Trace { input, .. } |
            Self::Rank { input, .. } |
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
            Self::Forge { .. } |
            Self::Forecast { .. } |
            Self::Decide { .. } |
            Self::Negotiate { .. } |
            Self::BlowItUp { .. } |
            Self::Readback { .. } |
            Self::Debugging { .. } |
            Self::Trace { .. } |
            Self::Rank { .. } |
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
    Forge => "forge",
    Forecast => "forecast",
    Decide => "decide",
    Negotiate => "negotiate",
    BlowItUp => "blow_it_up",
    Readback => "readback",
    Debugging => "debug",
    Trace => "trace",
    Rank => "rank",
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

/// Compatibility spelling for existing request consumers.
pub type SubmissionMetadata = SubmissionMeta;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TraceSource { Pasted { text: String }, Quoted { event_id: EventId } }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingHuman {
 pub contest_id: ContestId,
 pub kind: crate::ContestKind,
 pub state_version: u64,
 pub step_id: crate::StepId,
 pub prompt: String,
 pub items: Vec<String>,
 pub choices: Vec<Vec<String>>,
 pub outcome_labels: Vec<String>,
 pub answers: std::collections::BTreeMap<u32,String>,
}
impl Verb {
 pub fn new_mode_kind(self) -> Option<crate::ContestKind> { match self {
Self::Forge => Some(crate::ContestKind::Forge),
Self::Forecast => Some(crate::ContestKind::Forecast),
Self::Decide => Some(crate::ContestKind::Decide),
Self::Negotiate => Some(crate::ContestKind::Negotiate),
Self::BlowItUp => Some(crate::ContestKind::BlowItUp),
Self::Readback => Some(crate::ContestKind::Readback),
Self::Debugging => Some(crate::ContestKind::Debugging),
Self::Trace => Some(crate::ContestKind::Trace),
Self::Rank => Some(crate::ContestKind::Rank),
_ => None } }
}
