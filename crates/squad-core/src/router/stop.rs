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
        let kind:String=tx.query_row("SELECT kind FROM contests WHERE contest_id=?",[&contest],|r|r.get(0))?;
        let kind:ContestKind=serde_json::from_value(serde_json::Value::String(kind))?;
        if matches!(kind,ContestKind::Forge|ContestKind::Forecast|ContestKind::Decide|ContestKind::Negotiate|ContestKind::BlowItUp|ContestKind::Readback|ContestKind::Debugging|ContestKind::Trace|ContestKind::Rank) {
            let id_contest:ContestId=contest.parse().map_err(|_|StoreError::Unavailable)?;
            let participant=target.as_ref().is_none_or(|nick| {
                tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE contest_id=? AND target=?)",params![&contest,nick.as_str()],|r|r.get::<_,bool>(0)).unwrap_or(false)
            });
            if participant {super::new_mode_stop(kind,tx,group,id,id_contest)?;stopped_contest=Some(id_contest);}
        }

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
        } else if let Ok(mut state) = serde_json::from_str::<GrillState>(&raw) {
            let setup = squad_store::grill::setup(tx, state.contest_id)?.setup;
            if target
                .as_ref()
                .is_none_or(|n| setup.participants().contains(n))
            {
                if state.state == GrillStatus::Preflight {
                    let raw: String = tx.query_row(
                        "SELECT plan_json FROM contests WHERE contest_id=?",
                        [state.contest_id.to_string()],
                        |r| r.get(0),
                    )?;
                    let plan: ContestPreflight = serde_json::from_str(&raw)?;
                    super::grill::resolve(tx, group, id, state.contest_id, plan.plan_id, true, 0)?;
                } else {
                    state.state = GrillStatus::Stopped;
                    state.state_version += 1;
                    state.terminal_reason = Some("stopped".into());
                    super::grill::save(tx, group, id, &state)?;
                }
                stopped_contest = Some(state.contest_id);
            }
        } else if let Ok(mut state) = serde_json::from_str::<TournamentState>(&raw) {
            if target.as_ref().is_none_or(|n| state.survivors.contains(n)) {
                stop_tournament(tx, group, id, &mut state)?;
                stopped_contest = Some(state.contest_id);
            }
        } else if let Ok(mut state) = serde_json::from_str::<CrosscheckState>(&raw) {
            let setup: CrosscheckSetup =
                super::crosscheck::map(tx, state.contest_id, "crosscheck.setup")?;
            if target
                .as_ref()
                .is_none_or(|n| setup.participants.contains(n))
            {
                let version = state.state_version;
                state.state_version += 1;
                state.status = CrosscheckStatus::Stopped;
                state.terminal_reason = Some(Text::new("stopped").expect("literal"));
                super::crosscheck::save(tx, group, &state, Some(version))?;
                stopped_contest = Some(state.contest_id);
            }
        } else if let Ok(mut state) = serde_json::from_str::<CensusState>(&raw) {
            let setup = squad_store::census::setup(tx, state.contest_id)?;
            if target
                .as_ref()
                .is_none_or(|n| setup.participants.contains(n))
            {
                let version = state.state_version;
                state.state_version += 1;
                state.status = CensusStatus::Stopped;
                state.terminal_reason = Some(Text::new("stopped").expect("reason"));
                super::census::save(tx, group, id, &state, Some(version))?;
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
        } else if let Ok(mut state) = serde_json::from_str::<ProbeState>(&raw) {
            let setup = squad_store::probe::setup(tx, state.contest_id)?;
            if target
                .as_ref()
                .is_none_or(|n| setup.participants.contains(n))
            {
                state.state = ProbeStatus::Stopped;
                state.state_version += 1;
                state.terminal_reason = Some(Text::new("Stopped").expect("literal"));
                super::probe::save(tx, group, id, &state)?;
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
    cancel_work(tx, group, id, ids)?;
    super::picks::refresh(tx, group, id)?;
    Ok(())
}

pub fn tournament(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest_id: ContestId,
) -> Result<()> {
    let raw: String = tx
        .query_row(
            "SELECT state_json FROM contests WHERE contest_id=? AND kind='tournament' AND active=1",
            [contest_id.to_string()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(StoreError::Domain(
            ErrorCode::NOTHING_TO_STOP,
            "Tournament is no longer active",
        ))?;
    let mut state: TournamentState = serde_json::from_str(&raw)?;
    if state.state.is_terminal() || state.state == TournamentStatus::Preflight {
        return Err(super::tournaments::invalid());
    }
    stop_tournament(tx, group, id, &mut state)?;
    let mut query = tx.prepare(
        "SELECT work_id FROM work WHERE contest_id=? AND status IN ('queued','running') ORDER BY trigger_event_seq,work_id",
    )?;
    let ids = query
        .query_map([contest_id.to_string()], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    cancel_work(tx, group, id, ids)
}

fn stop_tournament(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &mut TournamentState,
) -> Result<()> {
    let old = state.state_version;
    state.state_version += 1;
    state.state = TournamentStatus::Stopped;
    state.terminal_reason = Some("stopped".into());
    super::tournaments::save(tx, group, id, state, Some(old))
}

fn cancel_work(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    ids: Vec<String>,
) -> Result<()> {
    for work_id in ids {
        let work_id = work_id.parse().map_err(|_| StoreError::Unavailable)?;
        let record = squad_store::work::record(tx, work_id)?.ok_or(StoreError::Unavailable)?;
        let work = squad_store::work::template(tx, work_id)?
            .ok_or(StoreError::Unavailable)?
            .0;
        let event = EnvelopePayload::WorkState(WorkStateEvent {
            work_id,
            trigger_event_id: work.trigger_event_id,
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
        squad_store::accounting::release_unstarted_work(tx, work_id)?;
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1,completed_event_seq=? WHERE work_id=?",params![seq.get(),work_id.to_string()])?;
    }
    Ok(())
}
