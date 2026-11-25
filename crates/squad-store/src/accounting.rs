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
/// Assign a frozen per-call share of a canonical plan reservation. The started
/// share survives release of the plan's unstarted remainder, including unknown costs.
fn split_plan_reservation(db: &Connection, grant: &ExecutionGrant, day: &str) -> Result<()> {
    let meta: Option<String> = db
        .query_row(
            "SELECT scheduling_meta FROM work WHERE work_id=?",
            [grant.work.work_id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    let Some(meta) = meta else {
        return Ok(());
    };
    let meta: serde_json::Value = serde_json::from_str(&meta)?;
    let (plan, amount) = match (meta.get("planId"), meta.get("attemptReservationMicros")) {
        (None, None) => return Ok(()),
        (Some(plan), Some(amount)) => (
            plan.as_str()
                .and_then(|s| s.parse::<PlanId>().ok())
                .ok_or_else(|| invalid("Invalid frozen reservation plan"))?,
            amount
                .as_i64()
                .filter(|n| *n >= 0)
                .ok_or_else(|| invalid("Invalid frozen attempt reservation"))?,
        ),
        _ => return Err(invalid("Incomplete frozen attempt reservation")),
    };
    let contest = grant
        .work
        .contest_id
        .ok_or_else(|| invalid("Plan reservation requires contest work"))?;
    let reservation = format!("plan:{plan}");
    let row: Option<(String,i64)> = db.query_row("SELECT day,amount_micros FROM reservations WHERE reservation_id=? AND plan_id=? AND contest_id=? AND attempt IS NULL AND unresolved=0", params![reservation,plan.to_string(),contest.to_string()], |r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (old_day, remaining) = row.ok_or_else(|| invalid("Missing canonical plan reservation"))?;
    if amount > remaining {
        return Err(invalid("Attempt exceeds remaining plan reservation"));
    }
    ensure_day(db, day)?;
    db.execute(
        "UPDATE reservations SET amount_micros=?1,amount_usd=?2 WHERE reservation_id=?3",
        params![
            remaining - amount,
            (remaining - amount) as f64 / 1e6,
            reservation
        ],
    )?;
    db.execute("INSERT INTO reservations(reservation_id,epoch,work_id,plan_id,contest_id,amount_usd,day,amount_micros) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![format!("attempt:{}:{}",grant.work.work_id,grant.work.attempt),grant.epoch as i64,grant.work.work_id.to_string(),plan.to_string(),contest.to_string(),amount as f64/1e6,day,amount])?;
    if old_day != day {
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![amount, old_day],
        )?;
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros+? WHERE day=?",
            params![amount, day],
        )?;
    }
    Ok(())
}

/// Ordinary work has no contest preflight. Reserve its remaining permitted calls
/// atomically with provider start, using the resolved catalog and full context
/// ceiling (including the adapter's safety margin). Unknown prices fail closed.
pub fn start_priced(
    db: &Connection,
    grant: &ExecutionGrant,
    now: DateTime,
    day: &str,
    max: i64,
) -> Result<()> {
    db.execute_batch("SAVEPOINT priced_start")?;
    let result = (|| {
        let started: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM attempts WHERE work_id=? AND attempt=?)",
            params![grant.work.work_id.to_string(), grant.work.attempt],
            |r| r.get(0),
        )?;
        let meta: String = db.query_row(
            "SELECT scheduling_meta FROM work WHERE work_id=?",
            [grant.work.work_id.to_string()],
            |r| r.get(0),
        )?;
        let meta: serde_json::Value = serde_json::from_str(&meta)?;
        if !started
            && meta.get("planId").is_none()
            && meta.get("attemptReservationMicros").is_none()
        {
            reserve_dispatch(db, grant, day, max)?;
        }
        start_inner(db, grant, now, day, max)
    })();
    if result.is_err() {
        db.execute_batch("ROLLBACK TO priced_start")?;
    }
    db.execute_batch("RELEASE priced_start")?;
    result
}

fn reserve_dispatch(db: &Connection, grant: &ExecutionGrant, day: &str, max: i64) -> Result<()> {
    let unknown = || {
        StoreError::Domain(
            ErrorCode::COST_UNKNOWN,
            "Resolved model pricing is unavailable",
        )
    };
    let row: Option<(i64, Option<u32>, String, String)> = db.query_row(
        "SELECT context_length,max_output,pricing_json,supported_params_json FROM catalog WHERE slug=?",
        [grant.binding.model.as_str()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).optional()?;
    let (context, published, pricing, supported) = row.ok_or_else(unknown)?;
    let pricing: serde_json::Value = serde_json::from_str(&pricing)?;
    let price = |key: &str| {
        pricing[key]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| pricing[key].as_f64())
            .filter(|p| p.is_finite() && *p >= 0.0)
            .ok_or_else(unknown)
    };
    let supported: Vec<String> = serde_json::from_str(&supported)?;
    let reasoning = if supported.iter().any(|p| p == "reasoning") {
        match grant.binding.effort {
            ReasoningEffort::Low => constants::REASONING_LOW_TOKENS,
            ReasoningEffort::Medium => constants::REASONING_MEDIUM_TOKENS,
            ReasoningEffort::High => constants::REASONING_HIGH_TOKENS,
        }
    } else {
        0
    };
    let visible = grant
        .work
        .max_output_tokens
        .unwrap_or(match grant.work.kind {
            WorkKind::Reply => constants::REPLY_VISIBLE_TOKENS,
            WorkKind::DebateTurn | WorkKind::GrillQuestion | WorkKind::GrillAnswer => {
                constants::DEBATE_TURN_VISIBLE_TOKENS
            }
            WorkKind::JudgeRound => constants::ROUND_JUDGE_VISIBLE_TOKENS,
            _ => constants::FINAL_JUDGE_VISIBLE_TOKENS,
        });
    // Match the adapter's published output clamping and reasoning reservation.
    let output = u64::from(
        visible.min(
            published
                .unwrap_or(u32::try_from(context).map_err(|_| unknown())?)
                .saturating_sub(reasoning),
        ) + reasoning,
    );
    let context = context as u64;
    let input = context.saturating_sub(output + 1024.max(context.div_ceil(20)));
    let unit =
        reservation_micros(input as f64 * price("prompt")? + output as f64 * price("completion")?)
            .map_err(|_| unknown())?;
    let limit: u32 = match grant.work.kind {
        WorkKind::Judge
        | WorkKind::JudgeTournament
        | WorkKind::CensusMerge
        | WorkKind::CensusFinal
        | WorkKind::CrosscheckGoal
        | WorkKind::CrosscheckNormalize
        | WorkKind::CrosscheckRule
        | WorkKind::SurveyExtract
        | WorkKind::SurveyReport
        | WorkKind::ProbeReport => 3,
        _ => 2,
    };
    let remaining = limit.saturating_sub(grant.work.attempt - 1);
    let pool = format!("dispatch:{}", grant.work.work_id);
    transfer_unstarted(db, grant.work.work_id, day)?;
    let prior: Option<i64> = db
        .query_row(
            "SELECT amount_micros FROM reservations WHERE reservation_id=?",
            [&pool],
            |r| r.get(0),
        )
        .optional()?;
    let needed = unit.checked_mul(i64::from(remaining)).ok_or_else(unknown)?;
    if let Some(prior) = prior {
        // Reprice remaining calls after restart or catalog refresh. Started shares
        // retain their original amounts and billing day.
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![prior, day],
        )?;
        db.execute("DELETE FROM reservations WHERE reservation_id=?", [&pool])?;
    }
    reserve(db, &pool, Some(grant.work.work_id), day, needed, max)?;
    db.execute(
        "UPDATE reservations SET amount_micros=?,amount_usd=? WHERE reservation_id=?",
        params![needed - unit, (needed - unit) as f64 / 1e6, pool],
    )?;
    db.execute("INSERT INTO reservations(reservation_id,epoch,work_id,attempt,unresolved,amount_usd,day,amount_micros) VALUES (?,?,?,?,1,?,?,?)",
        params![format!("attempt:{}:{}",grant.work.work_id,grant.work.attempt),grant.epoch as i64,
            grant.work.work_id.to_string(),grant.work.attempt,unit as f64/1e6,day,unit])?;
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
    // The router may commit a refusal event after a daily gate failure. Keep
    // reservation splitting/day transfer atomic even in that transaction.
    db.execute_batch("SAVEPOINT provider_start")?;
    let result = start_inner(db, grant, now, day, max);
    if result.is_err() {
        db.execute_batch("ROLLBACK TO provider_start")?;
    }
    db.execute_batch("RELEASE provider_start")?;
    result
}
fn start_inner(
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
    split_plan_reservation(db, grant, day)?;
    transfer_unstarted(db, grant.work.work_id, day)?;
    admit(db, day, 0, max)?;
    ensure_day(db, day)?;
    db.execute("INSERT INTO attempts(work_id,attempt,epoch,target,context_id,model,day,started_at,outcome,unresolved,debate_id,case_id,contest_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'unknown',1,?9,?10,?11)",params![grant.work.work_id.to_string(),grant.work.attempt,grant.epoch as i64,grant.work.target.to_string(),grant.work.context_id.0,grant.binding.model.as_str(),day,now.to_rfc3339(),grant.work.debate_id.map(|id|id.to_string()),grant.work.case_id.map(|id|id.to_string()),grant.work.contest_id.map(|id|id.to_string())])?;
    db.execute(
        "UPDATE reservations SET attempt=?1,unresolved=1 WHERE work_id=?2 AND attempt IS NULL AND reservation_id NOT LIKE 'dispatch:%'",
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
    if record.validate().is_err() {
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
    let attribution: (Option<String>, Option<String>, Option<String>) = db.query_row(
        "SELECT debate_id,case_id,contest_id FROM attempts WHERE work_id=? AND attempt=?",
        params![record.work_id.to_string(), record.attempt],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if attribution
        != (
            record.debate_id.map(|id| id.to_string()),
            record.case_id.map(|id| id.to_string()),
            record.contest_id.map(|id| id.to_string()),
        )
    {
        return Err(invalid("Accounting attribution mismatch"));
    }
    let (old_usage, old_ended): (Option<String>, Option<String>) = db.query_row(
        "SELECT usage_json,ended_at FROM attempts WHERE work_id=? AND attempt=?",
        params![record.work_id.to_string(), record.attempt],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut merged_usage = record.usage.clone();
    if let (Some(usage), Some(old_usage)) = (&mut merged_usage, old_usage) {
        let prior: Usage = serde_json::from_str(&old_usage)?;
        if usage.cost_usd.is_none() {
            usage.cost_usd = prior.cost_usd;
        }
    }
    let cost = record
        .usage
        .as_ref()
        .and_then(|usage| usage.cost_usd)
        .map(reported_micros)
        .transpose()
        .map_err(|_| invalid("Invalid reported cost"))?;
    let outcome = serde_json::to_value(&record.outcome)?;
    let stale_pending = old_ended.is_some() && record.ended_at.is_none();
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
    if stale_pending {
        return Ok(());
    }
    db.execute("UPDATE attempts SET outcome=?1,ended_at=?2,usage_json=COALESCE(?3,usage_json),provider_request_id=COALESCE(?4,provider_request_id),response_model=COALESCE(?5,response_model) WHERE work_id=?6 AND attempt=?7",params![outcome.as_str(),record.ended_at.map(|v|v.to_rfc3339()),merged_usage.as_ref().map(encode).transpose()?,record.provider_request_id,record.response_model.as_ref().map(|model|model.as_str()),record.work_id.to_string(),record.attempt])?;
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

/// Terminal work releases only calls that never received a durable provider start.
/// Started unknown amounts and the owning plan's remaining pool are independent.
pub fn release_unstarted_work(db: &Connection, work: WorkId) -> Result<()> {
    let mut query=db.prepare("SELECT day,SUM(amount_micros) FROM reservations WHERE work_id=? AND attempt IS NULL AND unresolved=0 GROUP BY day")?;
    let rows = query
        .query_map([work.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (day, amount) in rows {
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![amount, day],
        )?;
    }
    db.execute(
        "DELETE FROM reservations WHERE work_id=? AND attempt IS NULL AND unresolved=0",
        [work.to_string()],
    )?;
    Ok(())
}

pub fn release_contest(db: &Connection, id: ContestId) -> Result<()> {
    let mut q=db.prepare("SELECT day,SUM(amount_micros) FROM reservations WHERE contest_id=? AND attempt IS NULL AND unresolved=0 GROUP BY day")?;
    let rows = q
        .query_map([id.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (day, amount) in rows {
        db.execute(
            "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
            params![amount, day],
        )?;
    }
    db.execute(
        "DELETE FROM reservations WHERE contest_id=? AND attempt IS NULL AND unresolved=0",
        [id.to_string()],
    )?;
    Ok(())
}
