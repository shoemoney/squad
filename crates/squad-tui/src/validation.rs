//! One validation path for preview and submission; diagnostics remain verbatim.
use squad_protocol::{
    parser::{ParseContext, ParseError, SubmissionMeta},
    *,
};

#[derive(Debug, Clone)]
pub struct ValidatedInput {
    pub parsed: ParsedInput,
    pub recipients: Vec<String>,
    pub updates: Vec<VariableUpdate>,
}
pub fn validate(
    raw: &str,
    meta: &SubmissionMeta,
    context: &ParseContext,
) -> Result<ValidatedInput, ParseError> {
    let parsed = squad_protocol::parse(raw, meta, context)?;
    let recipients = match &parsed {
        ParsedInput::Message { recipients, .. } => recipients.clone(),
        ParsedInput::Debate { debaters, .. } => debaters.to_vec(),
        ParsedInput::Tournament { entrants, .. } => entrants.clone(),
        ParsedInput::QuestionFirst { candidates, .. } => candidates.clone(),
        ParsedInput::Grill {
            leader, grillers, ..
        } => std::iter::once(leader.clone())
            .chain(grillers.clone())
            .collect(),
        ParsedInput::Census { participants, .. }
        | ParsedInput::Crosscheck { participants, .. }
        | ParsedInput::Survey { participants, .. }
        | ParsedInput::Probe { participants, .. } => participants.clone(),
        ParsedInput::ContestPick { nicks, .. } => nicks.clone(),
        _ => vec![],
    }
    .into_iter()
    .map(|n| format!("@{n}"))
    .collect::<Vec<_>>();
    let recipients = if matches!(parsed, ParsedInput::Judge { .. }) {
        vec!["@judge".into()]
    } else {
        recipients
    };
    let updates = match &parsed {
        ParsedInput::Assignment { updates, .. } => updates.clone(),
        ParsedInput::Slash {
            action: squad_protocol::parser::SlashAction::Set { variable, value },
            ..
        } => vec![VariableUpdate {
            name: variable.clone(),
            value: *value,
        }],
        _ => parsed
            .conversational()
            .map(|i| i.variable_updates.clone())
            .unwrap_or_default(),
    };
    Ok(ValidatedInput {
        parsed,
        recipients,
        updates,
    })
}
pub fn request(
    raw: &str,
    meta: &SubmissionMeta,
    context: &ParseContext,
    id: EventId,
) -> Result<Request, ParseError> {
    let input = validate(raw, meta, context)?;
    Request::from_parsed_with_context(id, input.parsed, meta, context, false)
}

/// Where an accepted input goes. Pick answers and ordinary messages are
/// distinguished by the parser, never by the client.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Pick {
        contest_id: ContestId,
        nicks: Vec<MemberNick>,
    },
    Judge,
    Ordinary,
}
pub fn route(parsed: &ParsedInput) -> Route {
    match parsed {
        ParsedInput::ContestPick {
            contest_id, nicks, ..
        } => Route::Pick {
            contest_id: *contest_id,
            nicks: nicks.clone(),
        },
        ParsedInput::Judge { .. } => Route::Judge,
        _ => Route::Ordinary,
    }
}
/// A mentions-only pick becomes `ContestPick { contestId, nicks }`. Anything the
/// parser reads as an ordinary message returns `Ok(None)` so the caller submits it
/// unchanged and the pending pick is left alone. Wrong-pick and insufficient-answer
/// diagnostics come back verbatim as the parser's error.
pub fn pick_request(
    raw: &str,
    meta: &SubmissionMeta,
    context: &ParseContext,
    id: EventId,
) -> Result<Option<Request>, ParseError> {
    let input = validate(raw, meta, context)?;
    if !matches!(route(&input.parsed), Route::Pick { .. }) {
        return Ok(None);
    }
    Request::from_parsed_with_context(id, input.parsed, meta, context, false).map(Some)
}
/// Judge-input validation text, verbatim from the parser (e.g. the quote rule).
pub fn judge_error(raw: &str, meta: &SubmissionMeta, context: &ParseContext) -> Option<String> {
    validate(raw, meta, context)
        .err()
        .filter(|_| raw.contains("@judge"))
        .map(|e| e.message)
}
