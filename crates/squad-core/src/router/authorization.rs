use squad_protocol::*;
use squad_store::{Result, StoreError};
pub fn forbidden() -> StoreError {
    StoreError::Domain(ErrorCode::PRODUCER_FORBIDDEN, "Producer is not authorized")
}
pub fn recipients(roster: &[MemberNick], requested: &[MemberNick]) -> Result<Vec<Recipient>> {
    if requested.is_empty() {
        return Err(StoreError::Domain(
            ErrorCode::RECIPIENT_REQUIRED,
            "A recipient is required",
        ));
    }
    if requested.iter().any(|nick| !roster.contains(nick)) {
        return Err(StoreError::Domain(
            ErrorCode::UNKNOWN_RECIPIENT,
            "Unknown recipient",
        ));
    }
    if requested
        .iter()
        .enumerate()
        .any(|(i, nick)| requested[..i].contains(nick))
    {
        return Err(forbidden());
    }
    Ok(roster
        .iter()
        .filter(|nick| requested.contains(nick))
        .cloned()
        .map(Recipient::Member)
        .collect())
}
pub fn output(work: &Work, owner: &ExecutorKey, payload: &EnvelopePayload) -> Result<()> {
    if !owner.owns(work) {
        return Err(forbidden());
    }
    let generation = match payload {
        EnvelopePayload::AgentReply(reply) | EnvelopePayload::MessageFinal(reply)
            if work.kind == WorkKind::Reply =>
        {
            if Some(reply.in_reply_to_message_id) != work.in_reply_to_message_id {
                return Err(forbidden());
            }
            &reply.completed.generation
        }
        EnvelopePayload::DebateTurn(turn) if work.kind == WorkKind::DebateTurn => {
            if Some(turn.debate_id) != work.debate_id
                || Some(turn.round) != work.round
                || work.target != Target::Member(turn.speaker.clone())
            {
                return Err(forbidden());
            }
            &turn.generation
        }
        EnvelopePayload::CaseRuling(ruling)
            if work.kind == WorkKind::Judge && work.debate_id.is_none() =>
        {
            if Some(ruling.case_id) != work.case_id
                || ruling.in_reply_to_event_id != work.trigger_event_id
            {
                return Err(forbidden());
            }
            &ruling.completed.generation
        }
        EnvelopePayload::Verdict(verdict) if work.kind == WorkKind::Judge => {
            if Some(verdict.case_id) != work.case_id || Some(verdict.debate_id) != work.debate_id {
                return Err(forbidden());
            }
            &verdict.completed.generation
        }
        EnvelopePayload::DebateRoundScore(score) if work.kind == WorkKind::JudgeRound => {
            if Some(score.case_id) != work.case_id
                || Some(score.debate_id) != work.debate_id
                || Some(score.round) != work.round
            {
                return Err(forbidden());
            }
            return Ok(());
        }
        _ => return Err(forbidden()),
    };
    if generation.work_id != work.work_id
        || generation.message_id != work.message_id
        || generation.trigger_event_id != work.trigger_event_id
        || generation.attempt != work.attempt
    {
        return Err(forbidden());
    }
    Ok(())
}
