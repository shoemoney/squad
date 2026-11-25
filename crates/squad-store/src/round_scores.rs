//! The running tally is derived from retained outcomes, never supplied by a model.
use crate::{
    Result,
    error::{decode, encode, invalid},
};
use rusqlite::{Connection, params};
use squad_protocol::*;
use std::collections::BTreeMap;
pub fn tally(db: &Connection, state: &DebateState) -> Result<BTreeMap<MemberNick, u32>> {
    let mut tally = state
        .debaters
        .iter()
        .cloned()
        .map(|nick| (nick, 0))
        .collect::<BTreeMap<_, _>>();
    let mut query=db.prepare("SELECT scores_json FROM round_scores WHERE debate_id=? AND scores_json IS NOT NULL ORDER BY round")?;
    for scores in query.query_map([state.debate_id.to_string()], |r| r.get::<_, String>(0))? {
        let scores: BTreeMap<MemberNick, RoundScore> = decode(scores?)?;
        add(&mut tally, &scores)?;
    }
    Ok(tally)
}
fn add(
    tally: &mut BTreeMap<MemberNick, u32>,
    scores: &BTreeMap<MemberNick, RoundScore>,
) -> Result<()> {
    if tally.keys().ne(scores.keys()) {
        return Err(invalid("Round debater mismatch"));
    }
    for (nick, score) in scores {
        *tally.get_mut(nick).expect("checked debater") += u32::from(score.score);
    }
    Ok(())
}
pub fn canonicalize(db: &Connection, score: &mut DebateRoundScore) -> Result<()> {
    let state = crate::contests::debate(db, score.debate_id)?;
    let mut authoritative = tally(db, &state)?;
    if let Some(scores) = &score.scores {
        add(&mut authoritative, scores)?;
    }
    score.tally = authoritative;
    score
        .validate_against(&state)
        .map_err(|_| invalid("Invalid debate round score"))
}
pub fn insert(db: &Connection, work: &Work, score: &DebateRoundScore, seq: Cursor) -> Result<()> {
    let state = crate::contests::debate(db, score.debate_id)?;
    let valid_case:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM cases WHERE case_id=? AND contest_id=? AND epoch=(SELECT epoch FROM room_meta))",params![score.case_id.to_string(),score.debate_id.to_string()],|r|r.get(0))?;
    if !valid_case
        || (score.scores.is_none() && work.attempt != u32::from(constants::ROUND_JUDGE_ATTEMPTS))
    {
        return Err(invalid("Round case mismatch or attempts remain"));
    }
    score
        .validate_against(&state)
        .map_err(|_| invalid("Invalid debate round score"))?;
    if work.kind != WorkKind::JudgeRound
        || work.debate_id != Some(score.debate_id)
        || work.case_id != Some(score.case_id)
        || work.round != Some(score.round)
    {
        return Err(invalid("Round score does not match work"));
    }
    let mut expected = score.clone();
    canonicalize(db, &mut expected)?;
    if expected.tally != score.tally {
        return Err(invalid("Incorrect running tally"));
    }
    db.execute(
        "INSERT INTO round_scores VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            score.debate_id.to_string(),
            score.case_id.to_string(),
            score.round,
            work.work_id.to_string(),
            seq.get(),
            score.scores.as_ref().map(encode).transpose()?,
            encode(&score.tally)?,
            score.reason.as_ref().map(|s| s.as_str())
        ],
    )?;
    crate::contests::associate(db, score.debate_id, ContestKind::Debate, seq)?;
    crate::cases::associate(db, score.case_id, seq)?;
    crate::work::finish(
        db,
        work.work_id,
        if score.scores.is_some() {
            WorkStatus::Completed
        } else {
            WorkStatus::Failed
        },
        seq,
    )
}
