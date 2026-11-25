use rusqlite::{Connection, params};
use squad_protocol::*;
use squad_store::{Result, StoreError};
pub fn validate(db: &Connection, verdict: &mut DebateVerdict) -> Result<()> {
    let state = squad_store::contests::debate(db, verdict.debate_id)?;
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE debate_id=? AND kind='judge.round' AND status IN ('queued','running'))",[state.debate_id.to_string()],|r|r.get(0))?;
    if state.state != DebateStatus::Judging || pending {
        return Err(super::debates::invalid());
    }
    let tally = squad_store::round_scores::tally(db, &state)?;
    verdict.verdict = squad_protocol::verdict::assemble_verdict(
        verdict.verdict.verdict.clone(),
        &state.debaters,
        &tally,
    )
    .map_err(|_| StoreError::Domain(ErrorCode::JUDGE_INVALID_VERDICT, "Invalid judge verdict"))?;
    verdict
        .validate_against(&state, &tally)
        .map_err(|_| StoreError::Domain(ErrorCode::JUDGE_INVALID_VERDICT, "Invalid judge verdict"))
}
pub fn seal(db: &Connection, verdict: &DebateVerdict, seq: Cursor) -> Result<()> {
    squad_store::cases::seal(db, verdict.case_id, seq)?;
    squad_store::contests::associate(db, verdict.debate_id, ContestKind::Debate, seq)?;
    let state = squad_store::contests::debate(db, verdict.debate_id)?;
    for nick in &state.debaters {
        let win = verdict.verdict.verdict.winner == *nick;
        db.execute("INSERT INTO scores(nick,wins,losses,trophies,updated_seq) VALUES (?1,?2,?3,0,?4) ON CONFLICT(nick) DO UPDATE SET wins=wins+excluded.wins,losses=losses+excluded.losses,updated_seq=excluded.updated_seq",params![nick.as_str(),u8::from(win),u8::from(!win),seq.get()])?;
    }
    Ok(())
}

/// Deliver the sealed verdict and retained round outcomes as one announcement,
/// preserving each source identity; round scores never enter earlier prompts.
pub fn announce(
    tx: &rusqlite::Transaction<'_>,
    group: &mut squad_store::commit::CommitGroup,
    verdict: &DebateVerdict,
    event: &CommittedEnvelope,
) -> Result<()> {
    let mut state = squad_store::contests::debate(tx, verdict.debate_id)?;
    let mut query =
        tx.prepare("SELECT event_seq FROM round_scores WHERE debate_id=? ORDER BY round")?;
    let seqs = query
        .query_map([verdict.debate_id.to_string()], |r| r.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for nick in &state.debaters {
        for seq in &seqs {
            let score = squad_store::events::at(tx, *seq)?;
            squad_store::history::project(
                tx,
                &squad_store::history::attributed(
                    &score,
                    ContextId(nick.to_string()),
                    "verdict.round.outcome",
                    HistoryRole::User,
                    Some("judge".into()),
                    serde_json::to_string(&score.content)?,
                ),
            )?;
        }
        squad_store::history::project(
            tx,
            &squad_store::history::attributed(
                event,
                ContextId(nick.to_string()),
                "verdict.announcement",
                HistoryRole::User,
                Some("judge".into()),
                serde_json::to_string(verdict)?,
            ),
        )?;
    }
    squad_store::history::project(
        tx,
        &squad_store::history::attributed(
            event,
            ContextId(format!("judge:{}", verdict.case_id)),
            "case.verdict",
            HistoryRole::Assistant,
            Some("judge".into()),
            serde_json::to_string(verdict)?,
        ),
    )?;
    let mut query = tx.prepare("SELECT nick,wins,losses,trophies FROM scores ORDER BY nick")?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let scores = rows
        .into_iter()
        .map(|(nick, wins, losses, trophies)| {
            Ok(Score {
                nick: MemberNick::new(&nick).map_err(|_| StoreError::Unavailable)?,
                wins: wins as u64,
                losses: losses as u64,
                trophies: trophies as u64,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    group.push(
        tx,
        super::envelope(
            event.base.id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ScoresChanged(ScoresChanged {
                verdict_event_id: event.base.id,
                scores,
            }),
        ),
    )?;
    let version = state.state_version;
    state.state_version += 1;
    state.state = DebateStatus::Completed;
    super::debates::save(tx, group, event.base.id, &state, Some(version))
}
