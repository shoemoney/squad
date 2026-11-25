//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum OpenMode {
    Interactive,
    OneShot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
// Keep the shared by-value API specified by section 02.
#[allow(clippy::large_enum_variant)]
pub enum Request {
    UserMessage {
        input: UserMessageInput,
        preconfirmed: bool,
        parsed: ParsedInput,
    },
    Stop {
        request_id: EventId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<MemberNick>,
    },
    Regen {
        request_id: EventId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message_id: Option<MessageId>,
    },
    DebateConfirm {
        request_id: EventId,
        debate_id: DebateId,
        plan_id: PlanId,
    },
    DebateCancel {
        request_id: EventId,
        debate_id: DebateId,
        plan_id: PlanId,
    },
    Judge {
        request_id: EventId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debate_id: Option<DebateId>,
    },
    SetVars {
        request_id: EventId,
        updates: Vec<VariableUpdate>,
    },
    Clear {
        request_id: EventId,
    },
    Retry {
        request_id: EventId,
        work_id: WorkId,
    },
    ContestPick {
        request_id: EventId,
        contest_id: ContestId,
        nicks: Vec<MemberNick>,
    },
    ContestConfirm {
        request_id: EventId,
        contest_id: ContestId,
        plan_id: PlanId,
    },
    ContestCancel {
        request_id: EventId,
        contest_id: ContestId,
        plan_id: PlanId,
    },
    CensusContinue {
        request_id: EventId,
        contest_id: ContestId,
        expected_state_version: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_event_id: Option<EventId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replacement_text: Option<Text>,
    },
    CrosscheckAccept {
        request_id: EventId,
        contest_id: ContestId,
        expected_state_version: u64,
        plan_event_id: EventId,
        grid_event_id: EventId,
        cell_ids: Vec<CellId>,
        reason: Text,
    },
    Export {
        scope: ExportScope,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<ContestId>,
    },
    Who,
    Help,
    Internal(InternalRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
pub enum RequestResult {
    Accepted {
        request_id: EventId,
        event_ids: Vec<EventId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<Cursor>,
    },
    PendingConfirmation {
        request_id: EventId,
        preflight: PendingPreflight,
        cursor: Cursor,
    },
    Rejected {
        request_id: EventId,
        error: ErrorPayload,
    },
    Internal(InternalResult),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
pub enum FeedItem {
    Committed(CommittedEnvelope),
    Delta(ProducerEnvelope),
    Presence(ProducerEnvelope),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
// Keep the shared by-value API specified by section 02.
#[allow(clippy::large_enum_variant)]
pub enum ExecutorRequest {
    Claim {
        work_id: WorkId,
    },
    StartAttempt {
        work_id: WorkId,
        attempt: u32,
        fence: u64,
    },
    RenewLease {
        work_id: WorkId,
        fence: u64,
    },
    Delta {
        fence: u64,
        epoch: u64,
        delta: Delta,
    },
    Presence(Presence),
    Result {
        work_id: WorkId,
        attempt: u32,
        fence: u64,
        epoch: u64,
        output: ProducerEnvelope,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
pub enum InternalRequest {
    Executor(ExecutorRequest),
    Produce {
        envelope: ProducerEnvelope,
    },
    Transition {
        envelope_id: EventId,
        submission: ContestTransitionSubmission,
    },
    Barrier {
        request_id: EventId,
        commit: BarrierCommit,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
pub enum InternalResult {
    Claimed(ExecutionGrant),
    Started(ExecutionGrant),
    LeaseRenewed {
        work_id: WorkId,
        fence: u64,
        lease_until: DateTime,
    },
    NoWork,
    Text {
        text: String,
    }, // non-durable export, roster or help response
}

impl Request {
    /// Frontend mapping preserves the successful AST and complete atomic update vectors.
    pub fn from_parsed(
        request_id: EventId,
        parsed: ParsedInput,
        meta: &crate::parser::SubmissionMetadata,
        preconfirmed: bool,
    ) -> Result<Self, crate::validation::ValidationError> {
        use crate::parser::SlashAction;
        let request = match &parsed {
            ParsedInput::Assignment { updates, .. } => Self::SetVars {
                request_id,
                updates: updates.clone(),
            },
            ParsedInput::ContestPick {
                contest_id, nicks, ..
            } => Self::ContestPick {
                request_id,
                contest_id: *contest_id,
                nicks: nicks.clone(),
            },
            ParsedInput::Slash { action, .. } => match action {
                SlashAction::Set { variable, value } => Self::SetVars {
                    request_id,
                    updates: vec![VariableUpdate {
                        name: variable.clone(),
                        value: *value,
                    }],
                },
                SlashAction::Stop { nick } => Self::Stop {
                    request_id,
                    target: nick.clone(),
                },
                SlashAction::Judge { debate_id } => Self::Judge {
                    request_id,
                    debate_id: *debate_id,
                },
                SlashAction::Regen { message_id } => Self::Regen {
                    request_id,
                    message_id: *message_id,
                },
                SlashAction::Clear => Self::Clear { request_id },
                SlashAction::Who => Self::Who,
                SlashAction::Help => Self::Help,
                SlashAction::Export { scope, id } => Self::Export {
                    scope: scope.clone(),
                    id: id.as_deref().map(str::parse).transpose().map_err(|e| {
                        crate::validation::ValidationError {
                            message: format!("invalid export ID: {e}"),
                        }
                    })?,
                },
            },
            _ => Self::UserMessage {
                input: UserMessageInput {
                    message_id: request_id
                        .to_string()
                        .parse()
                        .expect("same UUIDv7 identity"),
                    text: Text::new(parsed.raw())
                        .map_err(|e| crate::validation::ValidationError { message: e.into() })?,
                    quote: (!meta.quote.is_empty()).then(|| meta.quote.clone()),
                    reply_to: (!meta.reply_to.is_empty()).then(|| meta.reply_to.clone()),
                },
                preconfirmed,
                parsed,
            },
        };
        Ok(request)
    }
}
impl FeedItem {
    pub fn validate(&self) -> crate::validation::ValidationResult {
        match self {
            Self::Committed(envelope) => envelope.validate(),
            Self::Delta(envelope) => {
                crate::validation::ensure(
                    matches!(envelope.content, EnvelopePayload::Delta(_)),
                    "delta feed requires delta envelope",
                )?;
                envelope.validate()
            }
            Self::Presence(envelope) => {
                crate::validation::ensure(
                    matches!(envelope.content, EnvelopePayload::Presence(_)),
                    "presence feed requires presence envelope",
                )?;
                envelope.validate()
            }
        }
    }
}
