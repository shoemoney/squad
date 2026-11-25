//! Deterministic USD-to-millionth conversions, independent of provider token budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAmount;
fn convert(usd: f64, reservation: bool) -> std::result::Result<i64, InvalidAmount> {
    if !usd.is_finite() || usd < 0.0 {
        return Err(InvalidAmount);
    }
    let scaled = usd * 1_000_000.0;
    let value = if reservation {
        scaled.ceil()
    } else {
        scaled.round()
    };
    // i64::MAX rounds up when represented as f64: reject that boundary as well.
    if value >= i64::MAX as f64 {
        return Err(InvalidAmount);
    }
    Ok(value as i64)
}
pub fn reported_micros(usd: f64) -> std::result::Result<i64, InvalidAmount> {
    convert(usd, false)
}
pub fn reservation_micros(usd: f64) -> std::result::Result<i64, InvalidAmount> {
    convert(usd, true)
}

use crate::{
    Result, StoreError,
    error::{encode, invalid},
};
use rusqlite::{Connection, OptionalExtension, params};
use squad_protocol::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailySpend {
    pub day: String,
    pub spent_micros: i64,
    pub reserved_micros: i64,
    pub unknown_calls: u32,
}
pub fn daily(db: &Connection, day: &str) -> Result<DailySpend> {
    let row = db
        .query_row(
            "SELECT spent_micros,reserved_micros,unknown_calls FROM spend_daily WHERE day=?",
            [day],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .unwrap_or((0, 0, 0));
    Ok(DailySpend {
        day: day.into(),
        spent_micros: row.0,
        reserved_micros: row.1,
        unknown_calls: row.2,
    })
}
fn ensure_day(db: &Connection, day: &str) -> Result<()> {
    if chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .is_none_or(|date| date.to_string() != day)
    {
        return Err(invalid("Invalid local day"));
    }
    db.execute("INSERT OR IGNORE INTO spend_daily VALUES (?,0,0,0)", [day])?;
    Ok(())
}
pub fn admit(db: &Connection, day: &str, additional: i64, max: i64) -> Result<()> {
    if additional < 0 || max < 0 {
        return Err(invalid("Invalid daily amount"));
    }
    let spend = daily(db, day)?;
    if spend.spent_micros >= max
        || spend
            .spent_micros
            .checked_add(spend.reserved_micros)
            .and_then(|v| v.checked_add(additional))
            .is_none_or(|v| v > max)
    {
        return Err(StoreError::Domain(
            ErrorCode::DAILY_MAX_REACHED,
            "Daily spending limit reached",
        ));
    }
    Ok(())
}
pub fn reserve(
    db: &Connection,
    id: &str,
    work: Option<WorkId>,
    day: &str,
    amount: i64,
    max: i64,
) -> Result<()> {
    if let Some((old_day, old_amount, old_work)) = db
        .query_row(
            "SELECT day,amount_micros,work_id FROM reservations WHERE reservation_id=?",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
    {
        if old_day == day && old_amount == amount && old_work == work.map(|id| id.to_string()) {
            return Ok(());
        }
        return Err(invalid("Conflicting reservation"));
    }
    admit(db, day, amount, max)?;
    ensure_day(db, day)?;
    let epoch: i64 = db.query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))?;
    db.execute("INSERT INTO reservations(reservation_id,epoch,work_id,amount_usd,day,amount_micros) VALUES (?1,?2,?3,?4,?5,?6)",params![id,epoch,work.map(|id|id.to_string()),amount as f64/1e6,day,amount])?;
    db.execute(
        "UPDATE spend_daily SET reserved_micros=reserved_micros+? WHERE day=?",
        params![amount, day],
    )?;
    Ok(())
}
/// Move only reservations that have no started provider attempt. In-flight costs keep their day.
pub fn transfer_unstarted(db: &Connection, work: WorkId, day: &str) -> Result<()> {
    ensure_day(db, day)?;
    let mut query=db.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE work_id=? AND attempt IS NULL AND unresolved=0")?;
    let rows = query
        .query_map([work.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (id, old, amount) in rows {
        if old != day {
            db.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![amount, old],
            )?;
            db.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros+? WHERE day=?",
                params![amount, day],
            )?;
            db.execute(
                "UPDATE reservations SET day=? WHERE reservation_id=?",
                params![day, id],
            )?;
        }
    }
    Ok(())
}
/// This must commit before calling the provider. A crash after this point is an unknown call.
pub fn start(
    db: &Connection,
    grant: &ExecutionGrant,
    now: DateTime,
    day: &str,
    max: i64,
) -> Result<()> {
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM attempts WHERE work_id=? AND attempt=?)",
        params![grant.work.work_id.to_string(), grant.work.attempt],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    transfer_unstarted(db, grant.work.work_id, day)?;
    admit(db, day, 0, max)?;
    ensure_day(db, day)?;
    db.execute("INSERT INTO attempts(work_id,attempt,epoch,target,context_id,model,day,started_at,outcome,unresolved) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'unknown',1)",params![grant.work.work_id.to_string(),grant.work.attempt,grant.epoch as i64,grant.work.target.to_string(),grant.work.context_id.0,grant.binding.model.as_str(),day,now.to_rfc3339()])?;
    db.execute(
        "UPDATE reservations SET attempt=?1,unresolved=1 WHERE work_id=?2 AND attempt IS NULL",
        params![grant.work.attempt, grant.work.work_id.to_string()],
    )?;
    db.execute(
        "UPDATE spend_daily SET unknown_calls=unknown_calls+1 WHERE day=?",
        [day],
    )?;
    db.execute(
        "UPDATE work SET provider_may_have_started=1 WHERE work_id=?",
        [grant.work.work_id.to_string()],
    )?;
    Ok(())
}
/// Idempotent corrections apply only their difference. No work foreign key permits late billing after clear.
pub fn reconcile(db: &Connection, record: &UsageRecord) -> Result<()> {
    if record.attempt == 0 {
        return Err(invalid("Invalid accounting attempt"));
    }
    let row:Option<(String,Option<i64>,bool,String,String,String)>=db.query_row("SELECT day,reported_cost_micros,unresolved,target,context_id,model FROM attempts WHERE work_id=? AND attempt=?",params![record.work_id.to_string(),record.attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let Some((day, old, unknown, target, context, model)) = row else {
        return Err(invalid("Accounting has no provider start record"));
    };
    if target != record.target.to_string()
        || context != record.context_id.0
        || model != record.model.as_str()
        || day != record.day
    {
        return Err(invalid("Accounting identity mismatch"));
    }
    let cost = record
        .usage
        .as_ref()
        .and_then(|usage| usage.cost_usd)
        .map(reported_micros)
        .transpose()
        .map_err(|_| invalid("Invalid reported cost"))?;
    let outcome = serde_json::to_value(&record.outcome)?;
    if let Some(cost) = cost {
        let difference = cost - old.unwrap_or(0);
        db.execute("UPDATE spend_daily SET spent_micros=spent_micros+?1,unknown_calls=unknown_calls-?2 WHERE day=?3",params![difference,u32::from(unknown),day])?;
        let reserved: i64 = db.query_row(
            "SELECT COALESCE(SUM(amount_micros),0) FROM reservations WHERE work_id=? AND attempt=?",
            params![record.work_id.to_string(), record.attempt],
            |r| r.get(0),
        )?;
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![reserved, day],
        )?;
        db.execute(
            "DELETE FROM reservations WHERE work_id=? AND attempt=?",
            params![record.work_id.to_string(), record.attempt],
        )?;
        db.execute("UPDATE attempts SET reported_cost_micros=?1,unresolved=0 WHERE work_id=?2 AND attempt=?3",params![cost,record.work_id.to_string(),record.attempt])?;
    }
    db.execute("UPDATE attempts SET outcome=?1,ended_at=?2,usage_json=?3,provider_request_id=COALESCE(?4,provider_request_id),response_model=?5 WHERE work_id=?6 AND attempt=?7",params![outcome.as_str(),record.ended_at.map(|v|v.to_rfc3339()),record.usage.as_ref().map(encode).transpose()?,record.provider_request_id,record.response_model.as_ref().map(|model|model.as_str()),record.work_id.to_string(),record.attempt])?;
    Ok(())
}
pub fn budget(db: &Connection, day: &str, max: i64) -> Result<BudgetStateEvent> {
    let spend = daily(db, day)?;
    Ok(BudgetStateEvent {
        spent_usd: spend.spent_micros as f64 / 1e6,
        reserved_usd: spend.reserved_micros as f64 / 1e6,
        limit_usd: max as f64 / 1e6,
        estimated_usd: 0.,
        unresolved_attempts: spend.unknown_calls,
        day: day.into(),
        unknown_calls: spend.unknown_calls,
    })
}
