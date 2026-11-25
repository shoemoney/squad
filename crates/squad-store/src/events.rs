//! Durable replay returns whole commit groups, including when the target limit is small.
use crate::{Result, error::decode};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
pub fn by_id(db: &Connection, id: EventId) -> Result<Option<CommittedEnvelope>> {
    let seq = db
        .query_row("SELECT seq FROM events WHERE id=?", [id.to_string()], |r| {
            r.get(0)
        })
        .optional()?;
    seq.map(|seq| at(db, seq)).transpose()
}
pub fn at(db: &Connection, seq: i64) -> Result<CommittedEnvelope> {
    let value = db.query_row("SELECT json_object('version',version,'id',id,'roomId',room_id,'correlationId',correlation_id,'sender',sender,'recipients',json(recipients),'createdAt',created_at,'seq',CAST(seq AS TEXT),'acceptedAt',accepted_at,'commitIndex',commit_index,'commitSize',commit_size,'kind',kind,'payload',json(payload)) FROM events WHERE seq=?", [seq], |row| row.get::<_, String>(0))?;
    let mut value: serde_json::Value = decode(value)?;
    let cause: Option<String> =
        db.query_row("SELECT causation_id FROM events WHERE seq=?", [seq], |r| {
            r.get(0)
        })?;
    if let Some(cause) = cause {
        value["causationId"] = cause.into();
    }
    Ok(serde_json::from_value(value)?)
}
pub fn page(
    db: &Connection,
    after: Option<Cursor>,
    limit: usize,
) -> Result<Vec<CommittedEnvelope>> {
    let (next, floor): (i64, Option<i64>) = db.query_row(
        "SELECT next_event_seq,replay_floor FROM room_meta",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let after = after.map_or(0, Cursor::get);
    if after >= next {
        return Err(crate::StoreError::Domain(
            ErrorCode::CURSOR_INVALID,
            "Cursor is ahead of the room",
        ));
    }
    if floor.is_some_and(|floor| after != 0 && after < floor) {
        return Err(crate::StoreError::Domain(
            ErrorCode::CURSOR_EXPIRED,
            "Cursor belongs to cleared history",
        ));
    }
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut statement =
        db.prepare("SELECT seq,commit_start_seq,commit_size FROM events WHERE seq>? ORDER BY seq")?;
    let rows = statement.query_map([after], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut end = after;
    let mut start = None;
    for row in rows {
        let (seq, group, size) = row?;
        start.get_or_insert(group);
        end = group + size - 1;
        if seq - after >= i64::try_from(limit).unwrap_or(i64::MAX) {
            break;
        }
    }
    let Some(start) = start else {
        return Ok(Vec::new());
    };
    let mut statement =
        db.prepare("SELECT seq FROM events WHERE seq BETWEEN ? AND ? ORDER BY seq")?;
    statement
        .query_map(params![start, end], |r| r.get::<_, i64>(0))?
        .map(|seq| at(db, seq?))
        .collect()
}
