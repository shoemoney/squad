//! Human-facing projections of committed room state. Reads never award scores.
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use squad_protocol::*;
use squad_store::{Result, StoreError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomState {
    pub room: String,
    pub vars: RoomVars,
    pub participants: Vec<Presence>,
    pub debates: Vec<DebateState>,
    pub pending_preflights: Vec<PendingPreflight>,
    pub models: Vec<Model>,
    pub scoreboard: Vec<Score>,
    pub daily_spend: DailySpend,
    pub trophies: BTreeMap<MemberNick, u64>,
    pub active_contest: Option<ContestState>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoomVars {
    #[serde(rename = "ROUNDS")]
    pub rounds: u8,
    #[serde(rename = "GRILL_ROUNDS")]
    pub grill_rounds: u8,
    #[serde(rename = "CENSUS_PASSES")]
    pub census_passes: u8,
    #[serde(rename = "NEGOTIATE_ROUNDS")]
    pub negotiate_rounds: u8,
    #[serde(rename = "READBACK_ROUNDS")]
    pub readback_rounds: u8,
    #[serde(rename = "DEBUG_ROUNDS")]
    pub debug_rounds: u8,
    pub tune_generations: u8,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySpend {
    pub day: String,
    pub spent_usd: f64,
    pub reserved_usd: f64,
    pub limit_usd: f64,
    pub unknown_calls: u64,
    pub has_unreconciled_charges: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestActivity {
    pub work: ContestWorkCounts,
    pub usage: ContestUsage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestWorkCounts {
    pub queued: u64,
    pub running: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestUsage {
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub unknown_calls: u64,
}

/// Read work and attempt totals in the caller's transaction.
pub(crate) fn contest_activity(db: &Connection, contest_id: ContestId) -> Result<ContestActivity> {
    let id = contest_id.to_string();
    let exists = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE contest_id=? AND epoch=(SELECT epoch FROM room_meta))",
        [&id],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_NOT_FOUND,
            "Contest not found",
        ));
    }
    let work = db.query_row(
        "SELECT COUNT(*) FILTER (WHERE status='queued'),
                COUNT(*) FILTER (WHERE status='running'),
                COUNT(*) FILTER (WHERE status='completed'),
                COUNT(*) FILTER (WHERE status='failed'),
                COUNT(*) FILTER (WHERE status='cancelled')
           FROM work WHERE contest_id=?",
        [&id],
        |row| {
            Ok(ContestWorkCounts {
                queued: row.get::<_, i64>(0)? as u64,
                running: row.get::<_, i64>(1)? as u64,
                completed: row.get::<_, i64>(2)? as u64,
                failed: row.get::<_, i64>(3)? as u64,
                cancelled: row.get::<_, i64>(4)? as u64,
            })
        },
    )?;
    let mut usage = ContestUsage::default();
    let mut cost_micros = 0_u128;
    let mut attempts = db.prepare(
        "SELECT a.usage_json, a.reported_cost_micros, a.unresolved
           FROM work w JOIN attempts a ON a.work_id=w.work_id
          WHERE w.contest_id=? AND a.contest_id=w.contest_id",
    )?;
    let mut rows = attempts.query([&id])?;
    while let Some(row) = rows.next()? {
        usage.calls += 1;
        if let Some(json) = row.get::<_, Option<String>>(0)? {
            let recorded: Usage = serde_json::from_str(&json)?;
            usage.input_tokens = usage.input_tokens.saturating_add(recorded.input_tokens);
            usage.output_tokens = usage.output_tokens.saturating_add(recorded.output_tokens);
        }
        cost_micros += row.get::<_, Option<i64>>(1)?.unwrap_or_default() as u128;
        usage.unknown_calls += u64::from(row.get::<_, bool>(2)?);
    }
    usage.cost_usd = cost_micros as f64 / 1e6;
    Ok(ContestActivity { work, usage })
}

/// Read within the caller's transaction so all durable projections share a snapshot.
/// Presence observations are ephemeral; queue depths always come from durable work.
pub fn snapshot(
    db: &Connection,
    roster: &[MemberNick],
    presence: &BTreeMap<MemberNick, Presence>,
    day: &str,
    daily_max_micros: i64,
) -> Result<RoomState> {
    let variable = |name| -> Result<u8> {
        Ok(db.query_row("SELECT value FROM vars WHERE name=?", [name], |r| r.get(0))?)
    };
    let mut participants = Vec::new();
    for nick in roster {
        let queue_depth = db.query_row(
            "SELECT COUNT(*) FROM work WHERE target=? AND status='queued'",
            [nick.as_str()],
            |r| r.get(0),
        )?;
        let running: Option<String> = db.query_row("SELECT work_id FROM work WHERE target=? AND status='running' ORDER BY rowid LIMIT 1", [nick.as_str()], |r| r.get(0)).optional()?;
        let mut participant = presence.get(nick).cloned().unwrap_or(Presence {
            nick: nick.clone(),
            status: PresenceStatus::Idle,
            work_id: None,
            queue_depth,
            error_code: None,
            observed_at: chrono::Utc::now(),
        });
        participant.queue_depth = queue_depth;
        participant.work_id = running
            .map(|id| id.parse().map_err(|_| StoreError::Unavailable))
            .transpose()?;
        if participant.work_id.is_some() && participant.status == PresenceStatus::Idle {
            participant.status = PresenceStatus::Thinking;
        }
        participants.push(participant);
    }
    let models = {
        let mut q = db.prepare("SELECT nick,role,roster_order,requested_slug,resolved_slug,context_length,pricing_json,resolved_at FROM models ORDER BY role='judge',roster_order")?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<u32>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?
        .map(|row| {
            let (nick, role, order, requested, resolved, length, pricing, at) = row?;
            Ok(Model {
                nick: if role == "judge" {
                    Actor::Judge
                } else {
                    Actor::Member(nick.parse().map_err(|_| StoreError::Unavailable)?)
                },
                requested_slug: Text::new(requested).map_err(|_| StoreError::Unavailable)?,
                resolved_slug: Text::new(resolved).map_err(|_| StoreError::Unavailable)?,
                roster_order: order,
                context_length: length as u64,
                pricing: serde_json::from_str(&pricing)?,
                resolved_at: at.parse().map_err(|_| StoreError::Unavailable)?,
            })
        })
        .collect::<Result<Vec<_>>>()?
    };
    let mut scoreboard = Vec::new();
    let mut trophies = BTreeMap::new();
    let mut q = db.prepare("SELECT nick,wins,losses,trophies FROM scores ORDER BY nick")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })? {
        let (nick, wins, losses, count) = row?;
        let nick: MemberNick = nick.parse().map_err(|_| StoreError::Unavailable)?;
        trophies.insert(nick.clone(), count as u64);
        scoreboard.push(Score {
            nick,
            wins: wins as u64,
            losses: losses as u64,
            trophies: count as u64,
        });
    }
    let mut debates = Vec::new();
    let mut active_contest = None;
    let mut pending_preflights = Vec::new();
    let mut q = db.prepare("SELECT kind,state_json,active,plan_json,expires_at FROM contests WHERE epoch=(SELECT epoch FROM room_meta) ORDER BY rowid")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })? {
        let (kind, json, active, plan, expires) = row?;
        // Owning stores may persist either the shared tagged carrier or its inner state.
        let state = serde_json::from_str::<ContestState>(&json).or_else(|_| {
            if kind == "tournament" {
                serde_json::from_str(&json).map(ContestState::Tournament)
            } else if kind == "crosscheck" {
                serde_json::from_str::<CrosscheckState>(&json)
                    .map(|s| ContestState::Generic(s.generic()))
            } else if kind == "debate" {
                serde_json::from_str(&json)
                    .map(ContestState::Debate)
                    .or_else(|_| serde_json::from_str(&json).map(ContestState::Selection))
            } else if kind == "census" {
                serde_json::from_str(&json)
                    .map(ContestState::Census)
                    .or_else(|_| serde_json::from_str(&json).map(ContestState::Generic))
            } else if kind == "survey" {
                serde_json::from_str(&json).map(ContestState::Survey)
            } else if kind == "grill" {
                serde_json::from_str(&json)
                    .map(ContestState::Grill)
                    .or_else(|_| serde_json::from_str(&json).map(ContestState::Selection))
            } else if kind == "tune" {
                serde_json::from_str(&json).map(ContestState::Tune)
            } else if kind == "bass_ackwards_prd" {
                serde_json::from_str(&json).map(ContestState::BassAckwardsPrd)
            } else if kind == "forge" {
                serde_json::from_str(&json).map(ContestState::Forge)
            } else if kind == "forecast" {
                serde_json::from_str(&json).map(ContestState::Forecast)
            } else if kind == "decide" {
                serde_json::from_str(&json).map(ContestState::Decide)
            } else if kind == "negotiate" {
                serde_json::from_str(&json).map(ContestState::Negotiate)
            } else if kind == "blow_it_up" {
                serde_json::from_str(&json).map(ContestState::BlowItUp)
            } else if kind == "readback" {
                serde_json::from_str(&json).map(ContestState::Readback)
            } else if kind == "debug" {
                serde_json::from_str(&json).map(ContestState::Debugging)
            } else if kind == "trace" {
                serde_json::from_str(&json).map(ContestState::Trace)
            } else if kind == "rank" {
                serde_json::from_str(&json).map(ContestState::Rank)
            } else if kind == "probe" {
                serde_json::from_str(&json).map(ContestState::Probe)
            } else {
                serde_json::from_str(&json)
                    .map(ContestState::Selection)
                    .or_else(|_| serde_json::from_str(&json).map(ContestState::Generic))
            }
        })?;
        if let ContestState::Debate(debate) = &state {
            debates.push(debate.clone());
        }
        let probe_pending =
            matches!(&state,ContestState::Probe(p) if p.state==ProbeStatus::Preflight);
        if active {
            active_contest = Some(state.clone());
        }
        if let Some(plan) = plan.filter(|_| {
            (kind != "probe" || probe_pending) && (kind != "grill" || !matches!(&state,ContestState::Grill(s) if s.state!=GrillStatus::Preflight)) && active && (kind != "census"
                || matches!(&state,ContestState::Census(s) if s.status==CensusStatus::Preflight)) && (kind != "survey" || matches!(&state,ContestState::Survey(s) if s.state==SurveyStatus::Preflight))
        }) {
            let unexpired = expires.as_ref().is_none_or(|at| {
                at.parse::<chrono::DateTime<chrono::Utc>>()
                    .is_ok_and(|at| at > chrono::Utc::now())
            });
            if unexpired && (kind != "crosscheck" || expires.is_some()) {
                let plan = serde_json::from_str::<PendingPreflight>(&plan).or_else(|_| {
                    if kind == "tournament" {
                        serde_json::from_str::<TournamentPreflight>(&plan)
                            .map(|p| PendingPreflight::Contest(p.base))
                    } else if kind == "crosscheck" {
                        serde_json::from_str::<CrosscheckPreflight>(&plan)
                            .map(|p| PendingPreflight::Contest(p.base))
                    } else if kind == "debate" {
                        serde_json::from_str(&plan).map(PendingPreflight::Debate)
            } else if kind == "probe" {
                        serde_json::from_str::<ProbePreflight>(&plan)
                            .map(|p| PendingPreflight::Contest(p.base))
                    } else {
                        serde_json::from_str(&plan).map(PendingPreflight::Contest)
                    }
                })?;
                pending_preflights.push(plan);
            }
        }
    }
    let spend = squad_store::accounting::daily(db, day)?;
    Ok(RoomState {
        room: "main".into(),
        vars: RoomVars {
            rounds: variable("ROUNDS")?,
            grill_rounds: variable("GRILL_ROUNDS")?,
            census_passes: variable("CENSUS_PASSES")?,
            negotiate_rounds: variable("NEGOTIATE_ROUNDS")?,
            readback_rounds: variable("READBACK_ROUNDS")?,
            debug_rounds: variable("DEBUG_ROUNDS")?,
            tune_generations: variable("TUNE_GENERATIONS")?,
        },
        participants,
        debates,
        pending_preflights,
        models,
        scoreboard,
        trophies,
        active_contest,
        daily_spend: DailySpend {
            day: spend.day,
            spent_usd: spend.spent_micros as f64 / 1e6,
            reserved_usd: spend.reserved_micros as f64 / 1e6,
            limit_usd: daily_max_micros as f64 / 1e6,
            unknown_calls: u64::from(spend.unknown_calls),
            has_unreconciled_charges: spend.unknown_calls != 0,
        },
    })
}
