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
