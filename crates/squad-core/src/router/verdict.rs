use rusqlite::{Connection, params};
use squad_protocol::*;
use squad_store::{Result, StoreError};
pub fn validate(db: &Connection, verdict: &mut DebateVerdict) -> Result<()> {
    let state = squad_store::contests::debate(db, verdict.debate_id)?;
    let tally = squad_store::round_scores::tally(db, &state)?;
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
