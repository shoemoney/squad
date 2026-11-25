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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boundaries {
    pub high_water: Option<Cursor>,
    pub replay_floor: Option<Cursor>,
}
/// Snapshot callers use a read transaction to hold this boundary across pages.
pub fn boundaries(db: &Connection) -> Result<Boundaries> {
    let (high, floor): (Option<i64>, Option<i64>) = db.query_row(
        "SELECT (SELECT MAX(seq) FROM events),replay_floor FROM room_meta",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(Boundaries {
        high_water: high
            .map(Cursor::new)
            .transpose()
            .map_err(crate::error::invalid)?,
        replay_floor: floor
            .map(Cursor::new)
            .transpose()
            .map_err(crate::error::invalid)?,
    })
}

#[derive(Debug)]
pub enum ReplayError {
    Store(crate::StoreError),
    Cursor {
        code: ErrorCode,
        earliest_cursor: Option<Cursor>,
    },
}
impl From<crate::StoreError> for ReplayError {
    fn from(value: crate::StoreError) -> Self {
        Self::Store(value)
    }
}
/// Validate an untrusted API cursor and preserve the explicit-reset boundary.
/// The optional high water must be a complete commit boundary, captured after
/// subscribing to the live feed. Groups crossing that boundary are not emitted.
pub fn replay(
    db: &Connection,
    after: Option<&str>,
    through: Option<Cursor>,
    limit: usize,
) -> std::result::Result<Vec<CommittedEnvelope>, ReplayError> {
    let invalid = || ReplayError::Cursor {
        code: ErrorCode::CURSOR_INVALID,
        earliest_cursor: None,
    };
    let after = after
        .map(|s| {
            serde_json::from_value::<Cursor>(serde_json::Value::String(s.into()))
                .map_err(|_| invalid())
        })
        .transpose()?;
    let bounds = boundaries(db)?;
    if after.is_some_and(|c| bounds.high_water.is_none_or(|high| c > high)) {
        return Err(invalid());
    }
    if after.is_some_and(|c| bounds.replay_floor.is_some_and(|floor| c < floor)) {
        return Err(ReplayError::Cursor {
            code: ErrorCode::CURSOR_EXPIRED,
            earliest_cursor: bounds.replay_floor,
        });
    }
    if through.is_some_and(|c| {
        bounds.high_water.is_none_or(|high| c > high) || after.is_some_and(|a| a > c)
    }) {
        return Err(invalid());
    }
    let events = page(db, after, limit)?;
    Ok(events
        .into_iter()
        .filter(|event| {
            through.is_none_or(|end| {
                event.commit.seq.get() - i64::from(event.commit.commit_index)
                    + i64::from(event.commit.commit_size)
                    - 1
                    <= end.get()
            })
        })
        .collect())
}

/// Backward transcript paging, still returned in ascending order and with complete groups.
pub fn before(
    db: &Connection,
    before: Option<Cursor>,
    limit: usize,
) -> Result<Vec<CommittedEnvelope>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let bounds = boundaries(db)?;
    if before.is_some_and(|c| {
        bounds
            .high_water
            .is_none_or(|high| c.get() > high.get() + 1)
    }) {
        return Err(crate::StoreError::Domain(
            ErrorCode::CURSOR_INVALID,
            "Cursor is ahead of the room",
        ));
    }
    if before.is_some_and(|c| bounds.replay_floor.is_some_and(|floor| c < floor)) {
        return Err(crate::StoreError::Domain(
            ErrorCode::CURSOR_EXPIRED,
            "Cursor belongs to cleared history",
        ));
    }
    let end = before.map_or(i64::MAX, Cursor::get);
    let start: Option<i64> = db.query_row("SELECT MIN(commit_start_seq) FROM (SELECT commit_start_seq FROM events WHERE seq<?1 ORDER BY seq DESC LIMIT ?2)",params![end,i64::try_from(limit).unwrap_or(i64::MAX)],|r|r.get(0))?;
    let Some(start) = start else {
        return Ok(Vec::new());
    };
    // A cursor inside a group requests that entire group, including its tail.
    let last: i64 = db.query_row(
        "SELECT commit_start_seq+commit_size-1 FROM events WHERE seq<? ORDER BY seq DESC LIMIT 1",
        [end],
        |r| r.get(0),
    )?;
    let mut query = db.prepare("SELECT seq FROM events WHERE seq BETWEEN ? AND ? ORDER BY seq")?;
    query
        .query_map(params![start, last], |r| r.get::<_, i64>(0))?
        .map(|r| at(db, r?))
        .collect()
}

/// Tracks only the last complete durable group. Ephemeral items never advance it.
/// Subscribe before creating this reader; on lag call catch_up and discard chunks.
#[derive(Default)]
pub struct FeedReplay {
    pub after: Option<Cursor>,
}
impl FeedReplay {
    pub fn catch_up(
        &mut self,
        db: &Connection,
        limit: usize,
    ) -> std::result::Result<Vec<CommittedEnvelope>, ReplayError> {
        let after = self.after.map(|c| c.get().to_string());
        let events = replay(db, after.as_deref(), boundaries(db)?.high_water, limit)?;
        if let Some(last) = events.last() {
            self.after = Some(last.commit.seq);
        }
        Ok(events)
    }
}
