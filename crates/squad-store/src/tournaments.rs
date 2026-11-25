//! Router-only tournament persistence. Capabilities never expose label maps.
use crate::{Result, StoreError};
use rusqlite::{Connection, params};
use squad_protocol::*;

pub fn state(db: &Connection, id: ContestId) -> Result<TournamentState> {
    let raw: String = db.query_row(
        "SELECT state_json FROM contests WHERE contest_id=? AND kind='tournament'",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn setup(db: &Connection, id: ContestId) -> Result<TournamentSetup> {
    let raw: String = db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='tournament.setup'", [id.to_string()], |r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn validate_execution(db: &Connection, work: &Work) -> Result<TournamentState> {
    let state = state(db, work.contest_id.ok_or(StoreError::Unavailable)?)?;
    let allowed = if work.kind == WorkKind::JudgeTournament {
        state.state == TournamentStatus::Judging
    } else {
        state.state == TournamentStatus::Answering
    };
    if !allowed
        || work.round != Some(state.round)
        || work.replay != Some(state.replay)
        || work.debate_id.is_some()
        || (work.kind == WorkKind::Reply
            && !state
                .survivors
                .iter()
                .any(|n| work.target == Target::Member(n.clone())))
    {
        return Err(StoreError::Domain(
            ErrorCode::STALE_WORK_ATTEMPT,
            "Stale tournament execution",
        ));
    }
    Ok(state)
}
pub fn labels(
    db: &Connection,
    id: ContestId,
    round: u32,
    replay: u8,
) -> Result<Vec<(String, MemberNick)>> {
    let mut q=db.prepare("SELECT label,nick FROM tournament_labels WHERE contest_id=? AND round=? AND replay=? ORDER BY label")?;
    q.query_map(params![id.to_string(), round, replay], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?
    .map(|r| {
        let (l, n) = r?;
        Ok((l, n.parse().map_err(|_| StoreError::Unavailable)?))
    })
    .collect()
}
pub fn entries(
    db: &Connection,
    id: ContestId,
    round: u32,
    replay: u8,
) -> Result<Vec<(Cursor, TournamentEntry)>> {
    let mut q=db.prepare("SELECT e.seq,e.payload FROM events e JOIN contest_events c ON c.event_seq=e.seq WHERE c.contest_id=? AND e.kind='tournament.entry' AND json_extract(e.payload,'$.round')=? AND json_extract(e.payload,'$.replay')=? ORDER BY e.seq")?;
    q.query_map(params![id.to_string(), round, replay], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?
    .map(|r| {
        let (seq, raw) = r?;
        Ok((
            Cursor::new(seq).map_err(|_| StoreError::Unavailable)?,
            serde_json::from_str(&raw)?,
        ))
    })
    .collect()
}
/// Validate and decode against router-only labels, with forfeits stably below
/// completed entries. Strict array position determines rank, including ties.
pub fn decode(
    db: &Connection,
    work: &Work,
    response: &TournamentJudgeResponse,
) -> Result<TournamentRanking> {
    let state = validate_execution(db, work)?;
    let labels = labels(db, state.contest_id, state.round, state.replay)?;
    response
        .validate(&labels.iter().map(|(l, _)| l.clone()).collect::<Vec<_>>())
        .map_err(|_| {
            StoreError::Domain(
                ErrorCode::JUDGE_INVALID_VERDICT,
                "Invalid tournament ranking",
            )
        })?;
    let entries = entries(db, state.contest_id, state.round, state.replay)?;
    let mut ranks = Vec::new();
    for row in &response.ranking {
        let nick = labels
            .iter()
            .find(|(l, _)| l == &row.label)
            .ok_or(StoreError::Unavailable)?
            .1
            .clone();
        let entry = entries
            .iter()
            .find(|(_, e)| e.entrant == nick)
            .ok_or(StoreError::Unavailable)?;
        let forfeited = matches!(entry.1.outcome, DebateTurnOutcome::Forfeit { .. });
        ranks.push(TournamentRank {
            nick,
            rank: 0,
            score: (!forfeited).then_some(row.score),
            critique: (!forfeited).then_some(row.critique.clone()),
            forfeited,
        });
    }
    ranks.sort_by_key(|r| r.forfeited);
    for (i, r) in ranks.iter_mut().enumerate() {
        r.rank = i as u32 + 1;
    }
    Ok(TournamentRanking {
        contest_id: state.contest_id,
        case_id: work.case_id.ok_or(StoreError::Unavailable)?,
        round: state.round,
        replay: state.replay,
        ranking: ranks,
    })
}
