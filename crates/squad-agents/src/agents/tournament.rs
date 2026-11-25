//! Reply-like tournament entry output preserves the durable execution identity.
use super::*;
pub fn entry(
    grant: &ExecutionGrant,
    entrant: MemberNick,
    output: StreamOutput,
) -> Result<EnvelopePayload, ReaderError> {
    Ok(EnvelopePayload::TournamentEntry(TournamentEntry {
        generation: identity(grant),
        contest_id: grant.work.contest_id.ok_or(ReaderError::NotRunnable)?,
        round: grant.work.round.ok_or(ReaderError::NotRunnable)?,
        replay: grant.work.replay.ok_or(ReaderError::NotRunnable)?,
        entrant,
        outcome: DebateTurnOutcome::Completed {
            content: output.content,
            model: grant.binding.model.clone(),
            finish_reason: output.finish_reason,
            usage: output.usage,
        },
    }))
}
