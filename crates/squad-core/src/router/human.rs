//! Version-bound human-step commits; no timeout and no provider calls.
use rusqlite::{Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn request(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    pending: &parser::PendingHuman,
) -> Result<()> {
    if !squad_store::schema::validate_contest_kind(tx, pending.contest_id, pending.kind)? {
        return Err(StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Human step kind mismatch",
        ));
    }
    let current: i64 = tx.query_row(
        "SELECT state_version FROM contests WHERE contest_id=? AND active=1",
        [pending.contest_id.to_string()],
        |r| r.get(0),
    )?;
    if pending.state_version != (current + 1) as u64 {
        return Err(StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Human step version changed",
        ));
    }
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestHumanRequested(ContestHumanRequested {
                pending: pending.clone(),
            }),
        ),
    )?;
    squad_store::contest_steps::human_requested(tx, pending, seq)?;
    tx.execute("UPDATE contests SET state_version=?,state_json=json_set(state_json,'$.state','AWAITING_HUMAN','$.stateVersion',?),expires_at=NULL,updated_seq=? WHERE contest_id=?",params![pending.state_version as i64,pending.state_version as i64,seq.get(),pending.contest_id.to_string()])?;
    squad_store::history::release(tx, pending.contest_id)
}
pub fn answer(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    expected: u64,
    text: Option<String>,
) -> Result<()> {
    let mut pending = squad_store::contest_steps::pending_human(tx)?
        .filter(|p| p.contest_id == contest && p.state_version == expected)
        .ok_or(StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Human step version changed",
        ))?;
    let ctx = parser::ParseContext {
        pending_human: Some(pending.clone()),
        ..Default::default()
    };
    if let Some(text) = &text {
        let raw = format!("/answer {text}");
        parse(&raw, &parser::SubmissionMeta::default(), &ctx)
            .map_err(|_| StoreError::Domain(ErrorCode::INVALID_COMMAND, "Invalid human answer"))?;
        crate::orchestrator::human::apply(&mut pending, text);
    }
    let complete = text.is_none() || crate::orchestrator::human::complete(&pending);
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::Me,
            vec![Recipient::Me],
            EnvelopePayload::ContestHumanAnswered(ContestHumanAnswered {
                contest_id: contest,
                step_id: pending.step_id,
                expected_state_version: expected,
                text,
                complete,
            }),
        ),
    )?;
    pending.state_version += 1;
    squad_store::contest_steps::human_answered(tx, &pending, complete, seq)?;
    tx.execute("UPDATE contests SET state_version=?,state_json=json_set(state_json,'$.stateVersion',?),updated_seq=? WHERE contest_id=? AND state_version=?",params![pending.state_version as i64,pending.state_version as i64,seq.get(),contest.to_string(),expected as i64])?;
    if complete {
        tx.execute("UPDATE contests SET state_json=json_set(state_json,'$.state','WAITING_FOR_PARTICIPANTS') WHERE contest_id=?",[contest.to_string()])?;
        // Hold new ordinary inputs while already queued work drains; the mode resumes at the barrier.
        tx.execute("UPDATE work SET held_by_contest_id=?,runnable=0 WHERE kind='reply' AND status='queued' AND trigger_event_seq>? AND target IN (SELECT target FROM contest_assignments WHERE step_id=?)",params![contest.to_string(),seq.get(),pending.step_id.to_string()])?;
        super::new_mode_human_answer(pending.kind, tx, &pending, true)?;
    }
    Ok(())
}

/// A pending human step owns the claim but deliberately holds no participant.
pub fn held_contest(tx: &rusqlite::Connection, nick: &MemberNick) -> Result<Option<ContestId>> {
    use rusqlite::OptionalExtension;
    let id:Option<String>=tx.query_row("SELECT c.contest_id FROM contests c WHERE c.active=1 AND c.kind IN ('tune','bass_ackwards_prd','forge','forecast','decide','negotiate','blow_it_up','readback','debug','trace','rank') AND json_extract(c.state_json,'$.state') NOT IN ('PREFLIGHT','AWAITING_HUMAN') AND EXISTS(SELECT 1 FROM events e,json_each(e.payload,'$.participants') p WHERE json_extract(e.payload,'$.contestId')=c.contest_id AND e.kind=c.kind||'.setup' AND p.value=?)",[nick.as_str()],|r|r.get(0)).optional()?;
    id.map(|id| id.parse().map_err(|_| StoreError::Unavailable))
        .transpose()
}
