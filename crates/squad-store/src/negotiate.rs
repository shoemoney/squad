//! Durable Negotiate state and immutable, assignment-scoped invocation inputs.
use crate::{Result, StoreError};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::negotiate::*;
use squad_protocol::*;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid negotiate state or artifact",
    )
}
pub fn state(db: &Connection, id: ContestId) -> Result<NegotiateState> {
    let s:String=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='negotiate' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&s)?)
}
pub fn save(db: &Connection, s: &NegotiateState, expected: Option<u64>, seq: Cursor) -> Result<()> {
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
    db.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,updated_seq) VALUES (?,'negotiate',(SELECT epoch FROM room_meta),?,?,?,?) ON CONFLICT(contest_id) DO UPDATE SET state_version=excluded.state_version,state_json=excluded.state_json,active=excluded.active,updated_seq=excluded.updated_seq",params![s.contest_id.to_string(),s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get()])?;
    crate::contests::associate(db, s.contest_id, ContestKind::Negotiate, seq)
}
pub fn put(db: &Connection, id: ContestId, key: &str, v: &serde_json::Value) -> Result<()> {
    crate::contest_steps::private_map(db, id, &format!("negotiate.{key}"), v)
}
pub fn get(db: &Connection, id: ContestId, key: &str) -> Result<serde_json::Value> {
    let s: String = db.query_row(
        "SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",
        params![id.to_string(), format!("negotiate.{key}")],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&s)?)
}
pub fn setup(db: &Connection, id: ContestId) -> Result<NegotiateSetup> {
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
        "SELECT input_json FROM negotiate_inputs WHERE work_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn accept(db: &Connection, w: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    let s = state(db, a.contest_id)?;
    let expected = match s.state {
        NegotiateStatus::Mapping => w.kind == WorkKind::NegotiateMap,
        NegotiateStatus::Briefing => w.kind == WorkKind::NegotiateBrief,
        NegotiateStatus::Packaging => w.kind == WorkKind::NegotiatePackage,
        NegotiateStatus::Rehearsing => {
            matches!(w.kind, WorkKind::NegotiateMove | WorkKind::NegotiateCounter)
        }
        NegotiateStatus::WritingPlaybook => w.kind == WorkKind::NegotiatePlaybook,
        _ => false,
    };
    let actor = match &w.target {
        Target::Member(n) => Actor::Member(n.clone()),
        Target::Judge => Actor::Judge,
    };
    if s.step_id != w.step_id
        || !expected
        || a.actor != Some(actor)
        || a.contest_id != w.contest_id.ok_or_else(invalid)?
        || a.step_id != w.step_id.ok_or_else(invalid)?
        || a.work_id != Some(w.work_id)
        || a.artifact_kind.as_str() != composition(&w.kind).ok_or_else(invalid)?
        || a.audience != Some(ArtifactAudience::Human)
        || a.round != Some(s.round)
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
        WorkKind::NegotiateMap => "negotiate.map",
        WorkKind::NegotiateBrief => "negotiate.brief",
        WorkKind::NegotiatePackage => "negotiate.package",
        WorkKind::NegotiateMove => "negotiate.move",
        WorkKind::NegotiateCounter => "negotiate.counter",
        WorkKind::NegotiatePlaybook => "negotiate.playbook",
        _ => return None,
    })
}
/// Mutable journal is private to the mode; invocation material remains immutable.
pub fn journal(db: &Connection, id: ContestId, key: &str, v: &serde_json::Value) -> Result<()> {
    db.execute("INSERT INTO negotiate_journal(contest_id,key,value_json) VALUES (?,?,?) ON CONFLICT(contest_id,key) DO UPDATE SET value_json=excluded.value_json",params![id.to_string(),key,v.to_string()])?;
    Ok(())
}
pub fn read_journal(db: &Connection, id: ContestId, key: &str) -> Result<serde_json::Value> {
    let raw: String = db.query_row(
        "SELECT value_json FROM negotiate_journal WHERE contest_id=? AND key=?",
        params![id.to_string(), key],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
/// No caller-selected recipient: the capability's frozen assignment chooses the projection.
pub fn invocation_for(db: &Connection, w: &Work) -> Result<serde_json::Value> {
    let assigned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM contest_assignments WHERE work_id=? AND step_id=? AND target=? AND context_id=?)",params![w.work_id.to_string(),w.step_id.map(|s|s.to_string()),w.target.to_string(),w.context_id.0],|r|r.get(0))?;
    if !assigned {
        return Err(invalid());
    }
    input(db, w.work_id)
}
/// Installed after all migrations because later scaffold migrations rebuild work.
pub fn ensure_holds(db: &Connection) -> Result<()> {
    db.execute_batch(r#"CREATE TRIGGER IF NOT EXISTS negotiate_hold_ordinary AFTER INSERT ON work
WHEN NEW.kind='reply' AND NEW.step_id IS NULL AND NEW.held_by_contest_id IS NULL
 AND EXISTS(SELECT 1 FROM contests c JOIN contest_private_maps p USING(contest_id)
 WHERE c.active=1 AND c.kind='negotiate' AND p.scope_key='negotiate.setup'
 AND json_extract(c.state_json,'$.state')<>'PREFLIGHT'
 AND EXISTS(SELECT 1 FROM json_each(p.map_json,'$.participants') WHERE value=NEW.target))
BEGIN
 UPDATE work SET runnable=0,held_by_contest_id=(SELECT c.contest_id FROM contests c JOIN contest_private_maps p USING(contest_id)
 WHERE c.active=1 AND c.kind='negotiate' AND p.scope_key='negotiate.setup'
 AND EXISTS(SELECT 1 FROM json_each(p.map_json,'$.participants') WHERE value=NEW.target)) WHERE work_id=NEW.work_id;
END;"#)?;
    Ok(())
}
