//! Source identity and delivery position are separate, including delayed delivery.
use crate::{
    Result,
    error::{decode, encode, invalid},
};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
pub fn project(db: &Connection, projection: &HistoryProjection) -> Result<Option<HistoryPosition>> {
    let seq: i64 = db.query_row(
        "SELECT seq FROM events WHERE id=?",
        [projection.origin_event_id.to_string()],
        |r| r.get(0),
    )?;
    if let HistoryMetadata::Attributed {
        origin_event_id,
        origin_seq,
        ..
    } = &projection.meta
        && (*origin_event_id != projection.origin_event_id || origin_seq.get() != seq)
    {
        return Err(invalid("Projection source identity mismatch"));
    }
    let existing: Option<i64> = db
        .query_row(
            "SELECT pos FROM history WHERE context_id=? AND origin_event_id=? AND projection_key=?",
            params![
                projection.context_id.0,
                projection.origin_event_id.to_string(),
                projection.projection_key.as_str()
            ],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(None);
    }
    let pos: i64 = db.query_row("SELECT next_history_pos FROM room_meta", [], |r| r.get(0))?;
    let role = serde_json::to_value(&projection.role)?;
    db.execute("INSERT INTO history(pos,context_id,event_seq,origin_event_id,projection_key,role,name,content,meta) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![pos,projection.context_id.0,seq,projection.origin_event_id.to_string(),projection.projection_key.as_str(),role.as_str(),projection.name,projection.content,encode(&projection.meta)?])?;
    db.execute(
        "UPDATE room_meta SET next_history_pos=next_history_pos+1",
        [],
    )?;
    Ok(Some(HistoryPosition::new(pos).expect("positive position")))
}
pub fn read(
    db: &Connection,
    context: &ContextId,
    through: HistoryPosition,
) -> Result<Vec<HistoryEntry>> {
    let mut query=db.prepare("SELECT pos,role,name,content,meta FROM history WHERE context_id=? AND pos<=? AND excluded=0 ORDER BY pos")?;
    let rows = query.query_map(params![context.0, through.get()], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    rows.map(|row| {
        let (pos, role, name, content, meta) = row?;
        Ok(HistoryEntry {
            position: HistoryPosition::new(pos).expect("positive position"),
            role: decode(format!("\"{role}\""))?,
            name,
            content,
            meta: decode(meta)?,
        })
    })
    .collect()
}
pub fn attributed(
    event: &CommittedEnvelope,
    context: ContextId,
    key: &str,
    role: HistoryRole,
    name: Option<String>,
    content: String,
) -> HistoryProjection {
    HistoryProjection {
        context_id: context,
        origin_event_id: event.base.id,
        projection_key: Text::new(key).expect("projection key"),
        role,
        name,
        content,
        meta: HistoryMetadata::Attributed {
            origin_event_id: event.base.id,
            origin_seq: event.commit.seq,
            author: event.base.sender.clone(),
            projection_key: Text::new(key).expect("projection key"),
        },
    }
}
pub fn defer(db: &Connection, contest: ContestId, projection: &HistoryProjection) -> Result<()> {
    db.execute("INSERT OR IGNORE INTO deferred_history(contest_id,context_id,origin_event_id,projection_key,projection_json) VALUES (?1,?2,?3,?4,?5)",params![contest.to_string(),projection.context_id.0,projection.origin_event_id.to_string(),projection.projection_key.as_str(),encode(projection)?])?;
    Ok(())
}
pub fn release(db: &Connection, contest: ContestId) -> Result<()> {
    let mut query=db.prepare("SELECT d.projection_json FROM deferred_history d JOIN events e ON e.id=d.origin_event_id WHERE d.contest_id=? ORDER BY e.seq,d.rowid")?;
    let projections = query
        .query_map([contest.to_string()], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for projection in projections {
        project(db, &decode::<HistoryProjection>(projection)?)?;
    }
    db.execute(
        "DELETE FROM deferred_history WHERE contest_id=?",
        [contest.to_string()],
    )?;
    db.execute("UPDATE work SET held_by_contest_id=NULL,runnable=CASE WHEN json_type(scheduling_meta,'$.openModeDormant') IS NULL THEN 1 ELSE 0 END,scheduling_meta=CASE WHEN json_type(scheduling_meta,'$.openModeDormant') IS NULL THEN scheduling_meta ELSE json_set(scheduling_meta,'$.openModeDormant',1) END WHERE held_by_contest_id=? AND status='queued'",[contest.to_string()])?;
    Ok(())
}
