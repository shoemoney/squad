//! Durable Forge state and immutable, assignment-scoped invocation inputs.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::forge::*;
use squad_protocol::*;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid forge state or artifact",
    )
}
pub fn state(db: &Connection, id: ContestId) -> Result<ForgeState> {
    let s:String=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='forge' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&s)?)
}
pub fn save(db: &Connection, s: &ForgeState, expected: Option<u64>, seq: Cursor) -> Result<()> {
    s.validate().map_err(|_| invalid())?;
    let old: Option<i64> = db
        .query_row(
            "SELECT state_version FROM contests WHERE contest_id=?",
            [s.contest_id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    if old != expected.map(|v| v as i64) {
        return Err(invalid());
    }
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'forge',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get()])?;
    crate::contests::associate(db, s.contest_id, ContestKind::Forge, seq)
}
pub fn put(db: &Connection, id: ContestId, key: &str, v: &serde_json::Value) -> Result<()> {
    crate::contest_steps::private_map(db, id, &format!("forge.{key}"), v)
}
pub fn get(db: &Connection, id: ContestId, key: &str) -> Result<serde_json::Value> {
    let s: String = db.query_row(
        "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",
        params![id.to_string(), format!("forge.{key}")],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn setup(db: &Connection, id: ContestId) -> Result<ForgeSetup> {
    Ok(serde_json::from_value(get(db, id, "setup")?)?)
}
pub fn events(db: &Connection, id: ContestId) -> Result<Vec<CommittedEnvelope>> {
    let mut q =
        db.prepare("SELECT event_seq FROM contest_events WHERE contest_id=? ORDER BY event_seq")?;
    q.query_map([id.to_string()], |r| r.get::<_, i64>(0))?
        .map(|s| crate::events::at(db, s?))
        .collect()
}
pub fn artifacts(
    db: &Connection,
    id: ContestId,
    kind: &str,
) -> Result<Vec<(MemberNick, serde_json::Value)>> {
    let mut out = vec![];
    for e in events(db, id)? {
        if let EnvelopePayload::ContestArtifact(a) = e.content
            && a.artifact_kind.as_str() == kind
            && let Some(Actor::Member(n)) = a.actor
        {
            out.push((n, a.body.ok_or_else(invalid)?));
        }
    }
    Ok(out)
}
pub fn input(db: &Connection, id: WorkId) -> Result<serde_json::Value> {
    let raw: String = db.query_row(
        "SELECT input_json FROM forge_inputs WHERE work_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn accept(db: &Connection, w: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    let s = state(db, a.contest_id)?;
    let expected = match s.state {
        ForgeStatus::Drafting => WorkKind::ForgeDraft,
        ForgeStatus::Ranking => WorkKind::ForgeRank,
        ForgeStatus::Fusing => WorkKind::ForgeFuse,
        ForgeStatus::Judging => WorkKind::ForgeJudge,
        _ => return Err(invalid()),
    };
    if s.step_id != w.step_id
        || w.kind != expected
        || a.artifact_kind.as_str() != composition(&w.kind).ok_or_else(invalid)?
        || a.audience != Some(ArtifactAudience::Human)
        || a.round != Some(0)
    {
        return Err(invalid());
    }
    validate_output(
        &w.kind,
        &input(db, w.work_id)?["data"],
        a.body.as_ref().ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    crate::contest_steps::artifact(db, a, seq)
}
pub fn composition(k: &WorkKind) -> Option<&'static str> {
    Some(match k {
        WorkKind::ForgeDraft => "forge.draft",
        WorkKind::ForgeRank => "forge.rank",
        WorkKind::ForgeFuse => "forge.fuse",
        WorkKind::ForgeJudge => "forge.judge",
        _ => return None,
    })
}
