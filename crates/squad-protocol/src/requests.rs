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
        /// Estimated tokens of the prompt assembled for this attempt. Ordinary
        /// dispatch sizes its reservation from it; `None` reserves the worst case.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_tokens: Option<u64>,
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
    /// Upsert the one provider-call record, including after result fencing.
    Account {
        fence: u64,
        epoch: u64,
        record: UsageRecord,
    },
    /// Report an attempt failure; only the router decides work terminal state.
    Failed {
        work_id: WorkId,
        attempt: u32,
        fence: u64,
        epoch: u64,
        error: ErrorPayload,
        /// Private rejected output for router-owned repair storage. Never project into events.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repair: Option<JudgeRepairData>,
    },
    Result {
        work_id: WorkId,
        attempt: u32,
        fence: u64,
        epoch: u64,
        output: EnvelopePayload,
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
    /// Frontends use this owner for snapshot-bound controls as well as ordinary input.
    pub fn from_parsed_with_context(
        request_id: EventId,
        parsed: ParsedInput,
        meta: &crate::parser::SubmissionMeta,
        ctx: &crate::parser::ParseContext,
        preconfirmed: bool,
    ) -> Result<Self, crate::parser::ParseError> {
        use crate::parser::{SlashAction, Span, errors::Rule};
        crate::parser::validate_control(&parsed, meta, ctx)?;
        let span = Span {
            start: 0,
            end: parsed.raw().len(),
        };
        match &parsed {
            ParsedInput::Slash {
                action: SlashAction::Continue { plan },
                ..
            } => {
                let (contest_id, expected_state_version) = crate::parser::waiting_census(ctx)
                    .ok_or_else(|| Rule::CensusNotWaiting.error(span.clone(), &[]))?;
                Ok(Self::CensusContinue {
                    request_id,
                    contest_id,
                    expected_state_version,
                    plan_event_id: crate::parser::continuation_plan(meta, ctx),
                    replacement_text: plan.as_ref().map(Text::new).transpose().map_err(|_| {
                        Rule::Usage.error(span, &[("usage", "/continue [\"<plan>\"]")])
                    })?,
                })
            }
            ParsedInput::Slash {
                action: SlashAction::Accept { cell_ids, reason },
                ..
            } => {
                let record = crate::parser::complete_crosscheck(ctx)
                    .ok_or_else(|| Rule::NoCompleteCrosscheck.error(span.clone(), &[]))?;
                Ok(Self::CrosscheckAccept {
                    request_id,
                    contest_id: record.contest_id.expect("validated complete target"),
                    expected_state_version: record.state_version.expect("validated state version"),
                    plan_event_id: record.plan_event_id.expect("validated plan"),
                    grid_event_id: record.grid_event_id.expect("validated grid"),
                    cell_ids: cell_ids.clone(),
                    reason: Text::new(reason).map_err(|_| {
                        Rule::Usage.error(span, &[("usage", "/accept <cellId…> -- <reason>")])
                    })?,
                })
            }
            _ => Self::from_parsed(request_id, parsed, meta, preconfirmed).map_err(|error| {
                crate::parser::ParseError {
                    code: ErrorCode::INVALID_COMMAND,
                    message: error.message,
                    span,
                }
            }),
        }
    }

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
            ParsedInput::Slash { action, .. } => {
                match action {
                    SlashAction::Continue { .. } | SlashAction::Accept { .. } => {
                        return Err(crate::validation::ValidationError {
                        message: "continuation and acceptance require Request::from_parsed_with_context".into(),
                    });
                    }
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
                        id: *id,
                    },
                }
            }
            _ => Self::UserMessage {
                input: UserMessageInput {
                    message_id: MessageId::from_event(request_id),
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

impl ExecutorRequest {
    /// Structural validation for additive attempt lifecycle submissions. Authority,
    /// current fences and accounting upserts are checked by the bound router.
    pub fn validate_attempt_submission(&self) -> crate::validation::ValidationResult {
        use crate::validation::ensure;
        match self {
            Self::Account { record, .. } => record.validate(),
            Self::Failed {
                work_id,
                attempt,
                error,
                ..
            } => {
                ensure(*attempt > 0, "failure attempt must be positive")?;
                ensure(
                    error.work_id == Some(*work_id),
                    "failure work identity mismatch",
                )
            }
            _ => Ok(()),
        }
    }
}
