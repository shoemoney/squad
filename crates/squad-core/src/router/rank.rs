//! rank router entry points. Filled by the mode worker.
use rusqlite::{Connection, Transaction};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
fn disabled<T>() -> Result<T> {
    Err(StoreError::Domain(
        ErrorCode::COMMAND_NOT_IMPLEMENTED,
        "rank is disabled",
    ))
}
pub fn prepare(
    _tx: &Transaction<'_>,
    _group: &mut CommitGroup,
    _id: EventId,
    _input: &UserMessageInput,
    _parsed: &ParsedInput,
    _admission: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    disabled()
}
pub fn accepted_work(
    _tx: &Connection,
    _work: &Work,
    _artifact: &ContestArtifact,
    _seq: Cursor,
) -> Result<()> {
    disabled()
}
pub fn human_answer(
    _tx: &Connection,
    _pending: &parser::PendingHuman,
    _complete: bool,
) -> Result<()> {
    Ok(())
}
pub fn stop(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
) -> Result<()> {
    let mut state = load_state(tx, contest)?;
    state.state = RankStatus::Stopped;
    state.state_version += 1;
    state.terminal_reason = Some("stopped".parse().expect("literal"));
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::RankState(state.clone()),
        ),
    )?;
    tx.execute("UPDATE contests SET active=0,state_version=?,state_json=?,updated_seq=? WHERE contest_id=?",rusqlite::params![state.state_version as i64,serde_json::to_string(&state)?,seq.get(),contest.to_string()])?;
    tx.execute(
        "UPDATE contest_human_steps SET closed=1 WHERE contest_id=?",
        [contest.to_string()],
    )?;
    squad_store::history::release(tx, contest)
}
pub fn recover(_tx: &Connection, _contest: ContestId) -> Result<()> {
    Ok(())
}
pub fn load_state(tx: &Connection, contest: ContestId) -> Result<RankState> {
    let raw: String = tx.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind=?",
        rusqlite::params![contest.to_string(), "rank"],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn resolve(
    _tx: &Transaction<'_>,
    _group: &mut CommitGroup,
    _id: EventId,
    _contest: ContestId,
    _plan: PlanId,
    _cancel: bool,
) -> Result<RequestResult> {
    disabled()
}
pub fn transition(
    _tx: &Transaction<'_>,
    _group: &mut CommitGroup,
    _id: EventId,
    _submission: ContestTransitionSubmission,
) -> Result<()> {
    disabled()
}
pub fn produce(
    _tx: &Transaction<'_>,
    _group: &mut CommitGroup,
    _envelope: ProducerEnvelope,
) -> Result<()> {
    disabled()
}
pub fn barrier(
    _tx: &Transaction<'_>,
    _group: &mut CommitGroup,
    _commit: BarrierCommit,
) -> Result<()> {
    disabled()
}
