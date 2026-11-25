use rusqlite::Connection;
use squad_protocol::*;
use squad_store::{Result, StoreError, events};
pub fn quotes(db: &Connection, ids: &[EventId]) -> Result<Vec<ResolvedQuote>> {
    ids.iter()
        .map(|id| {
            let event = events::by_id(db, *id)?.ok_or(StoreError::Domain(
                ErrorCode::INVALID_REFERENCE,
                "Invalid reference",
            ))?;
            let text = match &event.content {
                EnvelopePayload::UserMessage(message) => message.body.clone(),
                EnvelopePayload::MessageFinal(message) => message.content.clone(),
                EnvelopePayload::CensusPlan(plan) => plan.text.to_string(),
                EnvelopePayload::CensusReport(report) => crate::export::census::report_text(report),
                EnvelopePayload::Verdict(verdict) => serde_json::to_string(verdict)?,
                EnvelopePayload::CaseRuling(ruling) => ruling.content.to_string(),
                EnvelopePayload::DebateRoundScore(score) => {
                    let state = squad_store::contests::debate(db, score.debate_id)?;
                    if state.state != DebateStatus::Completed {
                        return Err(StoreError::Domain(
                            ErrorCode::INVALID_REFERENCE,
                            "Round scores are private until the verdict",
                        ));
                    }
                    serde_json::to_string(score)?
                }
                // Round scores and unrestricted operational payloads never enter member prompts through quotes.
                EnvelopePayload::DebateTurn(turn) => match &turn.outcome {
                    DebateTurnOutcome::Completed { content, .. } => content.clone(),
                    DebateTurnOutcome::Forfeit { text, .. } => text.to_string(),
                },
                _ if crate::orchestrator::census::grill_plan_text(&serde_json::to_value(
                    &event.content,
                )?)
                .is_some() =>
                {
                    crate::orchestrator::census::grill_plan_text(&serde_json::to_value(
                        &event.content,
                    )?)
                    .expect("checked canonical plan")
                }
                _ => {
                    return Err(StoreError::Domain(
                        ErrorCode::INVALID_REFERENCE,
                        "Reference is not shareable message content",
                    ));
                }
            };
            if let EnvelopePayload::DebateTurn(turn) = &event.content {
                let state = squad_store::contests::debate(db, turn.debate_id)?;
                if state.round <= turn.round
                    && !matches!(
                        state.state,
                        DebateStatus::Completed
                            | DebateStatus::CompleteUnjudged
                            | DebateStatus::Stopped
                            | DebateStatus::Failed
                    )
                {
                    return Err(StoreError::Domain(
                        ErrorCode::INVALID_REFERENCE,
                        "Current round content is private",
                    ));
                }
            }
            Ok(ResolvedQuote {
                event_id: *id,
                author: event.base.sender,
                text,
            })
        })
        .collect()
}
pub fn content(body: &str, quotes: &[ResolvedQuote]) -> String {
    let mut content = body.to_owned();
    for quote in quotes {
        content.push_str(&format!("\n\n> {}: {}", quote.author, quote.text));
    }
    content
}
