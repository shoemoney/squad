use rusqlite::{OptionalExtension, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn apply(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    target: Option<MemberNick>,
) -> Result<()> {
    let active: Option<(String, String)> = tx
        .query_row(
            "SELECT contest_id,state_json FROM contests WHERE active=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let mut stopped_contest = None;
    if let Some((contest, raw)) = active {
        if let Ok(mut state) = serde_json::from_str::<ContestSelectionState>(&raw) {
            if target.is_none() {
                state.state = ContestSelectionStatus::Stopped;
                state.state_version += 1;
                let seq = group.push(
                    tx,
                    super::envelope(
                        id,
                        Actor::System,
                        vec![Recipient::Me],
                        EnvelopePayload::ContestState(ContestState::Selection(state.clone())),
                    ),
                )?;
                tx.execute("UPDATE contests SET state_json=?,state_version=?,active=0,updated_seq=?,pending_request_json=json_set(COALESCE(pending_request_json,'{}'),'$.terminalReason','pick_cancelled') WHERE contest_id=?",params![serde_json::to_string(&state)?,state.state_version as i64,seq.get(),contest])?;
                stopped_contest = Some(state.contest_id);
            }
        } else if let Ok(mut state) = serde_json::from_str::<SurveyState>(&raw) {
            let setup = squad_store::survey::setup(tx, state.contest_id)?;
            if target
                .as_ref()
                .is_none_or(|n| setup.participants.contains(n))
            {
                let version = state.state_version;
                state.state_version += 1;
                state.state = SurveyStatus::Stopped;
                state.terminal_reason = Some(Text::new("Stopped").expect("literal"));
                super::survey::save(tx, group, id, &state, version)?;
                stopped_contest = Some(state.contest_id);
            }
        } else if let Ok(mut state) = serde_json::from_str::<DebateState>(&raw)
            && target.as_ref().is_none_or(|n| state.debaters.contains(n))
        {
            let version = state.state_version;
            state.state_version += 1;
            state.state = DebateStatus::Stopped;
            state.terminal_reason = Some("stopped".into());
            super::debates::save(tx, group, id, &state, Some(version))?;
            stopped_contest = Some(state.debate_id);
        }
    }
    let mut query=tx.prepare("SELECT work_id FROM work WHERE status IN ('queued','running') AND (?1 IS NULL OR target=?1 OR contest_id=?2) ORDER BY trigger_event_seq,work_id")?;
    let ids = query
        .query_map(
            params![
                target.as_ref().map(|n| n.as_str()),
                stopped_contest.map(|c| c.to_string())
            ],
            |r| r.get::<_, String>(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if ids.is_empty() && stopped_contest.is_none() {
        return Err(StoreError::Domain(
            ErrorCode::NOTHING_TO_STOP,
            "Nothing to stop",
        ));
    }
    for work_id in ids {
        let work_id = work_id.parse().map_err(|_| StoreError::Unavailable)?;
        let record = squad_store::work::record(tx, work_id)?.ok_or(StoreError::Unavailable)?;
        let work = squad_store::work::template(tx, work_id)?
            .ok_or(StoreError::Unavailable)?
            .0;
        let event = EnvelopePayload::WorkState(WorkStateEvent {
            work_id,
            trigger_event_id: squad_store::work::template(tx, work_id)?
                .ok_or(StoreError::Unavailable)?
                .0
                .trigger_event_id,
            target: record.target,
            context_id: record.context_id,
            attempt: record.attempt,
            status: WorkStatus::Cancelled,
            error: None,
            debate_id: work.debate_id,
            case_id: work.case_id,
            contest_id: work.contest_id,
        });
        let seq = group.push(
            tx,
            super::envelope(id, Actor::System, vec![Recipient::Me], event),
        )?;
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1,completed_event_seq=? WHERE work_id=?",params![seq.get(),work_id.to_string()])?;
    }
    super::picks::refresh(tx, group, id)?;
    Ok(())
}
