//! Durable preflight outcomes; pending commands are not conversational events.
use rusqlite::{Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};

pub fn propose(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    mut plan: DebatePreflight,
    daily_max: i64,
    threshold: f64,
) -> Result<RequestResult> {
    plan.validate().map_err(|_| super::debates::invalid())?;
    let mut state = squad_store::contests::debate(tx, plan.debate_id)?;
    if state.state != DebateStatus::Preparing
        || plan.debaters != state.debaters
        || plan.rounds != state.total_rounds
        || (!plan.judge_only && plan.judge_requested != state.judge_requested)
    {
        return Err(super::debates::invalid());
    }
    let judge_only:bool=tx.query_row("SELECT json_type(pending_request_json,'$.priorState') IS NOT NULL FROM contests WHERE contest_id=?",[plan.debate_id.to_string()],|r|r.get(0))?;
    if judge_only != plan.judge_only {
        return Err(super::debates::invalid());
    }
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let used: i64 = tx.query_row(
        "SELECT COALESCE((SELECT spent_micros+reserved_micros FROM spend_daily WHERE day=?),0)",
        [day],
        |r| r.get(0),
    )?;
    let needed = (plan.estimate.reserved_usd * 1_000_000.0).ceil();
    if used >= daily_max || needed > daily_max.saturating_sub(used) as f64 {
        let max = (daily_max as f64 / 1_000_000.0).to_string();
        let diagnostic = parser::errors::Rule::DailyMax
            .error(parser::Span { start: 0, end: 0 }, &[("max", &max)]);
        return reject(
            tx,
            group,
            id,
            &plan,
            super::error(diagnostic.code, diagnostic.message),
            PreflightStatus::Rejected,
        );
    }
    if plan.expires_at <= group.accepted_at {
        return reject(
            tx,
            group,
            id,
            &plan,
            super::error(ErrorCode::DEBATE_PLAN_EXPIRED, "Debate plan expired"),
            PreflightStatus::Expired,
        );
    }
    plan.confirmation_required = plan.estimate.estimated_usd > threshold;
    plan.estimate.remaining_usd = (daily_max.saturating_sub(used)) as f64 / 1_000_000.0;
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflight(plan.clone()),
        ),
    )?;
    state.judge_model = plan.judge_model.clone().unwrap_or_default();
    let mut frozen = serde_json::to_value(&plan)?;
    frozen["admissionMaxMicros"] = serde_json::json!(daily_max);
    frozen["confirmationThreshold"] = serde_json::json!(threshold);
    frozen["stateVersion"] = serde_json::json!(state.state_version + 1);
    tx.execute(
        "UPDATE contests SET plan_id=?,plan_json=?,expires_at=? WHERE contest_id=?",
        params![
            plan.plan_id.to_string(),
            frozen.to_string(),
            plan.expires_at.to_rfc3339(),
            plan.debate_id.to_string()
        ],
    )?;
    squad_store::contests::associate(tx, plan.debate_id, ContestKind::Debate, seq)?;
    let version = state.state_version;
    state.state_version += 1;
    state.state = DebateStatus::AwaitingConfirmation;
    super::debates::save(tx, group, id, &state, Some(version))?;
    let pending: Option<String> = tx.query_row(
        "SELECT pending_request_json FROM contests WHERE contest_id=?",
        [plan.debate_id.to_string()],
        |r| r.get(0),
    )?;
    let preconfirmed = pending
        .and_then(|p| serde_json::from_str::<Request>(&p).ok())
        .is_some_and(|r| {
            matches!(
                r,
                Request::UserMessage {
                    preconfirmed: true,
                    ..
                }
            )
        });
    if plan.estimate.estimated_usd <= threshold || preconfirmed {
        return resolve(tx, group, id, plan.debate_id, plan.plan_id, false);
    }
    Ok(RequestResult::PendingConfirmation {
        request_id: id,
        preflight: PendingPreflight::Debate(plan),
        cursor: seq,
    })
}
pub fn resolve(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    debate: DebateId,
    plan_id: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let kind: String = tx.query_row(
        "SELECT kind FROM contests WHERE contest_id=?",
        [debate.to_string()],
        |r| r.get(0),
    )?;
    if kind == "tournament" {
        return super::tournaments::resolve(tx, group, id, debate, plan_id, cancel);
    }
    if squad_store::schema::validate_contest_kind(tx, debate, ContestKind::Crosscheck)? {
        let max: i64 = tx.query_row("SELECT json_extract(pending_request_json,'$.dailyMax') FROM contests WHERE contest_id=?",[debate.to_string()],|r|r.get(0))?;
        return super::crosscheck::resolve(
            tx,
            group,
            id,
            debate,
            plan_id,
            cancel,
            max,
            Confirmation::Explicit,
        );
    }
    let mut state = squad_store::contests::debate(tx, debate)?;
    let (raw, pending): (Option<String>, Option<String>) = tx.query_row(
        "SELECT plan_json,pending_request_json FROM contests WHERE contest_id=?",
        [debate.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let raw = raw.ok_or(StoreError::Domain(
        ErrorCode::DEBATE_PLAN_STALE,
        "Debate plan stale",
    ))?;
    let frozen: serde_json::Value = serde_json::from_str(&raw)?;
    let plan: DebatePreflight = serde_json::from_str(&raw)?;
    if plan.plan_id != plan_id
        || frozen["stateVersion"].as_u64() != Some(state.state_version)
        || state.state != DebateStatus::AwaitingConfirmation
        || plan.debaters != state.debaters
        || plan.rounds != state.total_rounds
    {
        return Err(StoreError::Domain(
            ErrorCode::DEBATE_PLAN_STALE,
            "Debate plan stale",
        ));
    }
    let preconfirmed = pending
        .as_ref()
        .and_then(|p| serde_json::from_str::<Request>(p).ok())
        .is_some_and(|r| {
            matches!(
                r,
                Request::UserMessage {
                    preconfirmed: true,
                    ..
                }
            )
        });
    let confirmation = if preconfirmed {
        Confirmation::Preconfirmed
    } else if plan.estimate.estimated_usd
        <= frozen["confirmationThreshold"]
            .as_f64()
            .unwrap_or(constants::DEFAULT_DEBATE_CONFIRM_USD)
    {
        Confirmation::Automatic
    } else {
        Confirmation::Explicit
    };
    let expired = group.accepted_at >= plan.expires_at;
    let status = if expired {
        PreflightStatus::Expired
    } else if cancel {
        PreflightStatus::Cancelled
    } else {
        PreflightStatus::Confirmed
    };
    let version = state.state_version;
    if expired || cancel {
        state.state = if expired {
            DebateStatus::Expired
        } else {
            DebateStatus::Cancelled
        };
        if plan.judge_only {
            let value: serde_json::Value =
                serde_json::from_str(pending.as_deref().ok_or_else(super::debates::invalid)?)?;
            state = serde_json::from_value(value["priorState"].clone())?;
        }
    } else {
        let max = frozen["admissionMaxMicros"]
            .as_i64()
            .ok_or_else(super::debates::invalid)?;
        let amount = squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
            .map_err(|_| super::debates::invalid())?;
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        squad_store::accounting::reserve(tx, &format!("plan:{plan_id}"), None, &day, amount, max)?;
        tx.execute(
            "UPDATE reservations SET plan_id=?,contest_id=?,debate_id=? WHERE reservation_id=?",
            params![
                plan_id.to_string(),
                debate.to_string(),
                debate.to_string(),
                format!("plan:{plan_id}")
            ],
        )?;
        if let Some(raw) = pending
            && let Ok(Request::UserMessage { input, parsed, .. }) =
                serde_json::from_str::<Request>(&raw)
        {
            let ParsedInput::Debate { instruction, .. } = parsed else {
                return Err(super::debates::invalid());
            };
            let event = ProducerEnvelope {
                base: EnvelopeBase {
                    version: 1,
                    id: MessageId::from_event(id).to_string().parse().expect("UUID"),
                    room_id: MainRoom::Main,
                    correlation_id: id,
                    causation_id: Some(id),
                    sender: Actor::Me,
                    recipients: state
                        .debaters
                        .iter()
                        .cloned()
                        .map(Recipient::Member)
                        .collect(),
                    created_at: group.accepted_at,
                },
                content: EnvelopePayload::UserMessage(UserMessage {
                    input,
                    body: instruction,
                    resolved_quotes: vec![],
                    debate_id: Some(debate),
                    case_id: None,
                    contest_id: Some(debate),
                }),
            };
            // Preserve the original command identity, distinct from the confirmation request.
            let mut event = event;
            if let EnvelopePayload::UserMessage(m) = &event.content {
                event.base.id = m.input.message_id.to_string().parse().expect("UUID");
            }
            let seq = group.push(tx, event)?;
            squad_store::contests::associate(tx, debate, ContestKind::Debate, seq)?;
        }
        if plan.judge_only {
            state.judge_requested = true;
        }
        if !plan.judge_only {
            state.round = u32::from(!plan.opening_round_required);
        }
        state.state = if plan.judge_only {
            DebateStatus::Judging
        } else {
            DebateStatus::WaitingForDebaters
        };
    }
    let result_seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflightResult(DebatePreflightResult {
                debate_id: debate,
                plan_id,
                status,
                confirmation: (!expired && !cancel).then_some(confirmation),
                error: expired
                    .then(|| super::error(ErrorCode::DEBATE_PLAN_EXPIRED, "Debate plan expired")),
            }),
        ),
    )?;
    squad_store::contests::associate(tx, plan.debate_id, ContestKind::Debate, result_seq)?;
    state.state_version = version + 1;
    super::debates::save(tx, group, id, &state, Some(version))?;
    Ok(group.accepted(id))
}

fn reject(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    plan: &DebatePreflight,
    error: ErrorPayload,
    status: PreflightStatus,
) -> Result<RequestResult> {
    let mut state = squad_store::contests::debate(tx, plan.debate_id)?;
    let version = state.state_version;
    if plan.judge_only {
        let raw: String = tx.query_row(
            "SELECT pending_request_json FROM contests WHERE contest_id=?",
            [plan.debate_id.to_string()],
            |r| r.get(0),
        )?;
        state = serde_json::from_value(
            serde_json::from_str::<serde_json::Value>(&raw)?["priorState"].clone(),
        )?;
    } else {
        state.state = if status == PreflightStatus::Expired {
            DebateStatus::Expired
        } else {
            DebateStatus::Rejected
        };
    }
    let result_seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebatePreflightResult(DebatePreflightResult {
                debate_id: plan.debate_id,
                plan_id: plan.plan_id,
                status,
                confirmation: None,
                error: Some(error.clone()),
            }),
        ),
    )?;
    squad_store::contests::associate(tx, plan.debate_id, ContestKind::Debate, result_seq)?;
    state.state_version = version + 1;
    super::debates::save(tx, group, id, &state, Some(version))?;
    Ok(RequestResult::Rejected {
        request_id: id,
        error,
    })
}
