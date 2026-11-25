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
        EnvelopePayload::TournamentEntry(e)
            if work.kind == WorkKind::Reply && work.round.is_some() =>
        {
            if work.contest_id != Some(e.contest_id)
                || work.round != Some(e.round)
                || work.replay != Some(e.replay)
                || work.target != Target::Member(e.entrant.clone())
                || !matches!(e.outcome, DebateTurnOutcome::Completed { .. })
            {
                return Err(forbidden());
            }
            &e.generation
        }
        EnvelopePayload::TournamentJudgment(j) if work.kind == WorkKind::JudgeTournament => {
            if work.contest_id != Some(j.contest_id)
                || work.round != Some(j.round)
                || work.replay != Some(j.replay)
                || work.case_id != Some(j.case_id)
            {
                return Err(forbidden());
            }
            &j.generation
        }

        EnvelopePayload::AgentReply(reply) | EnvelopePayload::MessageFinal(reply)
            if work.kind == WorkKind::Reply && work.round.is_none() =>
        {
            if Some(reply.in_reply_to_message_id) != work.in_reply_to_message_id {
                return Err(forbidden());
            }
            &reply.completed.generation
        }
        EnvelopePayload::DebateTurn(turn) if work.kind == WorkKind::DebateTurn => {
            if !matches!(turn.outcome, DebateTurnOutcome::Completed { .. })
                || Some(turn.debate_id) != work.debate_id
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
            if (score.scores.is_none()
                && work.attempt != u32::from(constants::ROUND_JUDGE_ATTEMPTS))
                || Some(score.case_id) != work.case_id
                || Some(score.debate_id) != work.debate_id
                || Some(score.round) != work.round
            {
                return Err(forbidden());
            }
            return Ok(());
        }
        EnvelopePayload::ContestArtifact(artifact)
            if artifact.work_id == Some(work.work_id)
                && work.contest_id == Some(artifact.contest_id)
                && work.step_id == Some(artifact.step_id) =>
        {
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

/// Protocol's sole visibility table is enforced at the commit boundary, even
/// for trusted engine delivery proposals. Attributed metadata must match source.
pub fn projection(
    db: &rusqlite::Connection,
    state: &DebateState,
    p: &HistoryProjection,
) -> Result<()> {
    let source = squad_store::events::by_id(db, p.origin_event_id)?.ok_or_else(forbidden)?;
    if p.context_id.0.starts_with("contest:") {
        use rusqlite::OptionalExtension;
        let assignment:Option<String>=db.query_row("SELECT a.assignment_json FROM contest_assignments a JOIN contest_steps s ON s.step_id=a.step_id WHERE a.context_id=? AND s.contest_id=?",rusqlite::params![p.context_id.0,state.debate_id.to_string()],|r|r.get(0)).optional()?;
        let assignment: ContestAssignment =
            serde_json::from_str(&assignment.ok_or_else(forbidden)?)?;
        if !assignment.payload_refs.contains(&source.base.id)
            || p.name.is_some()
            || !matches!(p.meta, HistoryMetadata::Anonymous { .. })
            || !matches!(
                source.content,
                EnvelopePayload::UserMessage(_)
                    | EnvelopePayload::MessageFinal(_)
                    | EnvelopePayload::ContestArtifact(_)
            )
        {
            return Err(forbidden());
        }
        return Ok(());
    }
    match &p.meta {
        HistoryMetadata::Attributed {
            origin_event_id,
            origin_seq,
            author,
            projection_key,
        } if *origin_event_id == source.base.id
            && *origin_seq == source.commit.seq
            && *author == source.base.sender
            && *projection_key == p.projection_key => {}
        _ => return Err(forbidden()),
    }
    let member = state.debaters.iter().any(|n| n.as_str() == p.context_id.0);
    let judge = state
        .case_ids
        .iter()
        .any(|c| p.context_id.0 == format!("judge:{c}"));
    let allowed = match &source.content {
        EnvelopePayload::DebateTurn(t) => {
            t.debate_id == state.debate_id
                && (member || judge)
                && (t.round < state.round || state.state == DebateStatus::RoundCommitting)
        }
        EnvelopePayload::DebateRoundScore(s) => {
            s.debate_id == state.debate_id
                && (judge || (member && state.state == DebateStatus::Completed))
        }
        EnvelopePayload::Verdict(v) => v.debate_id == state.debate_id && (member || judge),
        EnvelopePayload::DebateSetup(s) => s.debate_id == state.debate_id && (member || judge),
        EnvelopePayload::DebateOpening(o) => o.debate_id == state.debate_id && member,
        _ => false,
    };
    if !allowed {
        return Err(forbidden());
    }
    Ok(())
}
