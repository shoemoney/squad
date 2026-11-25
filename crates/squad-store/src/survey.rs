//! Survey records are private until explicitly projected into an assigned step.
use crate::{Result, StoreError};
use rusqlite::{Connection, params};
use squad_protocol::*;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid survey state or result",
    )
}
pub fn setup(db: &Connection, id: ContestId) -> Result<SurveySetup> {
    let raw: String = db.query_row(
        "SELECT setup_json FROM survey_sources WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn state(db: &Connection, id: ContestId) -> Result<SurveyState> {
    let raw:String=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='survey' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn claims(db: &Connection, id: ContestId) -> Result<SurveyExtraction> {
    let mut q=db.prepare("SELECT claim_id,claim_text,origins_json FROM survey_claims WHERE contest_id=? ORDER BY claim_id")?;
    let rows = q
        .query_map([id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(SurveyExtraction {
        claims: rows
            .into_iter()
            .map(|(id, claim, raw)| {
                Ok(SurveyClaim {
                    id,
                    claim,
                    source_answer_ids: serde_json::from_str(&raw)?,
                })
            })
            .collect::<Result<_>>()?,
    })
}
pub fn ballots(db: &Connection, id: ContestId, second: bool) -> Result<Vec<SurveyVote>> {
    let mut q=db.prepare("SELECT e.seq FROM contest_events c JOIN events e ON e.seq=c.event_seq WHERE c.contest_id=? AND e.kind=? ORDER BY e.seq")?;
    let rows = q
        .query_map(
            params![
                id.to_string(),
                if second {
                    "survey.revote"
                } else {
                    "survey.vote"
                }
            ],
            |r| r.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|seq| match crate::events::at(db, seq)?.content {
            EnvelopePayload::SurveyVote(v) | EnvelopePayload::SurveyRevote(v) => Ok(v),
            _ => Err(invalid()),
        })
        .collect()
}
pub fn selected(db: &Connection, id: ContestId) -> Result<SurveySelection> {
    let raw:String=db.query_row("SELECT e.payload FROM events e JOIN contest_events c ON c.event_seq=e.seq WHERE c.contest_id=? AND e.kind='survey.selection'",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
/// Called after the common lease fence and before accepting any typed artifact.
pub fn validate_result(db: &Connection, work: &Work, output: &EnvelopePayload) -> Result<()> {
    let id = work.contest_id.ok_or_else(invalid)?;
    let state = state(db, id)?;
    if state.state.is_terminal() || state.step_id != work.step_id {
        return Err(invalid());
    }
    let setup = setup(db, id)?;
    match output {
        EnvelopePayload::SurveyClaims(c)
            if state.state == SurveyStatus::Extracting && work.kind == WorkKind::SurveyExtract =>
        {
            c.extraction.validate(&setup.answers).map_err(|_| invalid())
        }
        EnvelopePayload::SurveyVote(v)
            if state.state == SurveyStatus::Voting && work.kind == WorkKind::SurveyVote =>
        {
            v.ballot
                .validate(
                    &claims(db, id)?
                        .claims
                        .into_iter()
                        .map(|c| c.id)
                        .collect::<Vec<_>>(),
                    false,
                )
                .map_err(|_| invalid())
        }
        EnvelopePayload::SurveyRevote(v)
            if state.state == SurveyStatus::Revoting && work.kind == WorkKind::SurveyRevote =>
        {
            v.ballot
                .validate(
                    &selected(db, id)?
                        .claims
                        .into_iter()
                        .map(|c| c.claim_id)
                        .collect::<Vec<_>>(),
                    true,
                )
                .map_err(|_| invalid())
        }
        EnvelopePayload::SurveyReport(_)
            if state.state == SurveyStatus::Reporting && work.kind == WorkKind::SurveyReport =>
        {
            Ok(())
        }
        _ => Err(invalid()),
    }
}
pub fn accept(db: &Connection, work: &Work, output: &EnvelopePayload, seq: Cursor) -> Result<()> {
    let id = work.contest_id.ok_or_else(invalid)?;
    match output {
        EnvelopePayload::SurveyClaims(c) => {
            for c in &c.extraction.claims {
                db.execute(
                    "INSERT INTO survey_claims VALUES (?,?,?,?,?)",
                    params![
                        id.to_string(),
                        c.id,
                        seq.get(),
                        c.claim,
                        serde_json::to_string(&c.source_answer_ids)?
                    ],
                )?;
            }
        }
        EnvelopePayload::SurveyVote(v) | EnvelopePayload::SurveyRevote(v) => {
            for b in &v.ballot.votes {
                db.execute(
                    "INSERT INTO survey_votes VALUES (?,?,?,?,?,?,?,?,?)",
                    params![
                        id.to_string(),
                        b.claim_id,
                        if work.kind == WorkKind::SurveyRevote {
                            2
                        } else {
                            1
                        },
                        v.voter.as_str(),
                        serde_json::to_value(b.vote)?.as_str(),
                        b.confidence,
                        b.reason,
                        b.change_evidence,
                        seq.get()
                    ],
                )?;
            }
        }
        EnvelopePayload::SurveyReport(_) => {}
        _ => return Err(invalid()),
    }
    crate::contests::associate(db, id, ContestKind::Survey, seq)?;
    if let Some(case) = work.case_id {
        crate::cases::seal(db, case, seq)?;
    }
    crate::work::finish(db, work.work_id, WorkStatus::Completed, seq)?;
    crate::contest_steps::settle(db, work.step_id.ok_or_else(invalid)?, &work.target, None)
}
