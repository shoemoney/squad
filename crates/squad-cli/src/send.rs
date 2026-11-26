use crate::{
    args::Send,
    output::Output,
    room_api::{Progress, RoomApi, RoomError},
};
use serde_json::json;
use squad_protocol::{
    parser::{SlashAction, SubmissionMeta},
    *,
};
use std::{collections::BTreeSet, io, time::Duration};

fn io_error(_: io::Error) -> RoomError {
    RoomError {
        code: ErrorCode::NOT_READY,
        message: "Could not write command output.".into(),
        retryable: false,
    }
}

/// Runs a command and always closes the room, including on Ctrl+C and output errors.
/// The caller must open using OpenPolicy::send(args.no_wait).
pub async fn run<R: RoomApi>(
    room: &R,
    args: &Send,
    output: &mut Output<'_>,
    confirm: impl FnMut() -> Option<bool>,
) -> Result<u8, RoomError> {
    run_until(room, args, output, confirm, tokio::signal::ctrl_c()).await
}

/// Injectable interruption source for deterministic cleanup checks.
pub async fn run_until<R: RoomApi>(
    room: &R,
    args: &Send,
    output: &mut Output<'_>,
    mut confirm: impl FnMut() -> Option<bool>,
    interrupt: impl std::future::Future<Output = io::Result<()>>,
) -> Result<u8, RoomError> {
    let mut tap = crate::buslog::BodyTap::start(room.subscribe());
    let result = tokio::select! {
        result = execute(room, args, output, &mut confirm) => result,
        signal = interrupt => {
            signal.map(|()| 130).map_err(|_| RoomError { code: ErrorCode::NOT_READY, message: "Could not install interrupt handler.".into(), retryable: false })
        }
    };
    if let Some(tap) = tap.as_mut() {
        tap.drain();
    }
    let shutdown = room.shutdown().await;
    match result {
        Ok(code) => {
            shutdown?;
            Ok(code)
        }
        Err(error) => {
            let _ = shutdown;
            Err(error)
        }
    }
}

async fn execute<R: RoomApi>(
    room: &R,
    args: &Send,
    output: &mut Output<'_>,
    confirm: &mut impl FnMut() -> Option<bool>,
) -> Result<u8, RoomError> {
    let mut feed = room.subscribe();
    let state = room.state().await;
    let context = R::parse_context(&state);
    let id = EventId::new();
    let mut meta = SubmissionMeta::default();
    if args.quote.len() > 12 {
        output
            .error(
                Some(id),
                ErrorCode::INVALID_REFERENCE,
                squad_protocol::parser::errors::Rule::TooManyQuotes.template(),
                false,
            )
            .map_err(io_error)?;
        return Ok(1);
    }
    for seq in &args.quote {
        let Some(record) = context
            .records
            .iter()
            .find(|record| u64::try_from(record.seq.get()).ok() == Some(*seq))
        else {
            output
                .error(
                    Some(id),
                    ErrorCode::INVALID_REFERENCE,
                    &squad_protocol::parser::errors::Rule::QuoteUnavailable
                        .template()
                        .replace("{id}", &seq.to_string()),
                    false,
                )
                .map_err(io_error)?;
            return Ok(1);
        };
        meta.quote.push(record.event_id);
    }
    let parsed = match squad_protocol::parse(&args.message, &meta, &context) {
        Ok(parsed) => parsed,
        Err(error) => {
            output
                .error(Some(id), error.code, &error.message, false)
                .map_err(io_error)?;
            return Ok(1);
        }
    };
    // Local commands retain their schemas and never invent durable receipts.
    if let ParsedInput::Slash { action, .. } = &parsed {
        match action {
            SlashAction::Who => {
                if output.json {
                    output.record(json!({"type":"command.result","requestId":id,"result":{"command":"who","state":state}})).map_err(io_error)?;
                } else {
                    writeln!(
                        output.stdout,
                        "{}",
                        serde_json::to_string_pretty(&state).expect("state serializes")
                    )
                    .map_err(io_error)?;
                }
                return Ok(0);
            }
            SlashAction::Export {
                scope,
                id: contest_id,
            } => {
                let markdown = room.export(scope.clone(), *contest_id).await?;
                let name = serde_json::to_value(scope).expect("scope serializes");
                let filename = contest_id.map_or_else(
                    || "squad-room.md".to_owned(),
                    |id| format!("squad-{}-{id}.md", name.as_str().unwrap()),
                );
                let path = std::env::current_dir().map_err(io_error)?.join(&filename);
                std::fs::write(&path, markdown).map_err(io_error)?;
                if output.json {
                    let mut value = json!({"type":"export.ready","requestId":id,"scope":scope,"filename":filename,"path":path});
                    if let Some(id) = contest_id {
                        value["id"] = json!(id);
                    }
                    output.record(value).map_err(io_error)?;
                } else {
                    writeln!(output.stdout, "{}", path.display()).map_err(io_error)?;
                }
                return Ok(0);
            }
            _ => {}
        }
    }
    let is_help = matches!(
        &parsed,
        ParsedInput::Slash {
            action: SlashAction::Help,
            ..
        }
    );
    let request = match Request::from_parsed_with_context(id, parsed, &meta, &context, args.yes) {
        Ok(request) => request,
        Err(error) => {
            output
                .error(Some(id), error.code, &error.message, false)
                .map_err(io_error)?;
            return Ok(1);
        }
    };
    // Capture a high watermark after subscribing, before creating this request.
    let mut watermark = room
        .page_events(None, 1)
        .await?
        .last()
        .map(|event| event.commit.seq);
    let mut result = if args.no_wait {
        room.submit_commit_only(request).await
    } else {
        room.submit(request).await
    };
    let mut requested_reserve = None;
    let request_id = loop {
        match result {
            RequestResult::Rejected { request_id, error } => {
                let message = if error.code == ErrorCode::DAILY_MAX_REACHED {
                    reservation_diagnostic(
                        &serde_json::to_value(room.state().await).unwrap_or_default(),
                        requested_reserve,
                        error.text.as_str(),
                    )
                } else {
                    error.text.as_str().to_owned()
                };
                output
                    .error(Some(request_id), error.code, &message, error.retryable)
                    .map_err(io_error)?;
                return Ok(1);
            }
            RequestResult::Internal(InternalResult::Text { text }) => {
                if output.json && is_help {
                    output.record(json!({"type":"command.result","requestId":id,"result":{"command":"help","text":text}})).map_err(io_error)?;
                } else if output.json {
                    return Err(RoomError {
                        code: ErrorCode::INVALID_ENVELOPE,
                        message: "Unexpected local command response.".into(),
                        retryable: false,
                    });
                } else {
                    writeln!(output.stdout, "{text}").map_err(io_error)?;
                }
                return Ok(0);
            }
            RequestResult::PendingConfirmation {
                request_id: pending_id,
                preflight,
                ..
            } => {
                requested_reserve = Some(match &preflight {
                    PendingPreflight::Debate(p) => p.estimate.reserved_usd,
                    PendingPreflight::Contest(p) => p.estimate.reserved_usd,
                });
                let request_id = pending_id;
                output.preflight(request_id, &preflight).map_err(io_error)?;
                output.stdout.flush().map_err(io_error)?;
                let accepted = if args.yes { Some(true) } else { confirm() };
                if accepted.is_none() {
                    // PRD 05: a noninteractive refusal never admits the contest. Cancel the
                    // stored pending plan so the --yes re-run is not rejected as busy.
                    let cancel = match &preflight {
                        PendingPreflight::Debate(p) => Request::DebateCancel {
                            request_id: EventId::new(),
                            debate_id: p.debate_id,
                            plan_id: p.plan_id,
                        },
                        PendingPreflight::Contest(p) => Request::ContestCancel {
                            request_id: EventId::new(),
                            contest_id: p.contest_id,
                            plan_id: p.plan_id,
                        },
                    };
                    let _ = room.submit_commit_only(cancel).await;
                    output
                        .error(
                            Some(request_id),
                            ErrorCode::INVALID_COMMAND,
                            "Confirmation required. Re-run with --yes in noninteractive mode.",
                            false,
                        )
                        .map_err(io_error)?;
                    return Ok(1);
                }
                let next_id = EventId::new();
                let yes = accepted == Some(true);
                let request = match &preflight {
                    PendingPreflight::Debate(p) if yes => Request::DebateConfirm {
                        request_id: next_id,
                        debate_id: p.debate_id,
                        plan_id: p.plan_id,
                    },
                    PendingPreflight::Debate(p) => Request::DebateCancel {
                        request_id: next_id,
                        debate_id: p.debate_id,
                        plan_id: p.plan_id,
                    },
                    PendingPreflight::Contest(p) if yes => Request::ContestConfirm {
                        request_id: next_id,
                        contest_id: p.contest_id,
                        plan_id: p.plan_id,
                    },
                    PendingPreflight::Contest(p) => Request::ContestCancel {
                        request_id: next_id,
                        contest_id: p.contest_id,
                        plan_id: p.plan_id,
                    },
                };
                result = if args.no_wait {
                    room.submit_commit_only(request).await
                } else {
                    room.submit(request).await
                };
                if !yes && !matches!(result, RequestResult::Rejected { .. }) {
                    if output.json {
                        let value = match preflight {
                            PendingPreflight::Debate(p) => {
                                json!({"type":"debate.cancelled","requestId":next_id,"debateId":p.debate_id,"planId":p.plan_id})
                            }
                            PendingPreflight::Contest(p) => {
                                json!({"type":"contest.cancelled","requestId":next_id,"contestId":p.contest_id,"planId":p.plan_id})
                            }
                        };
                        output.record(value).map_err(io_error)?;
                    } else {
                        writeln!(output.stdout, "Cancelled.").map_err(io_error)?;
                    }
                    return Ok(0);
                }
            }
            RequestResult::Accepted {
                request_id: accepted_id,
                event_ids,
                cursor,
            } => {
                // Automatically admitted plans still print their committed preflight
                // before the acknowledgement. Never synthesize estimates locally.
                let state = room.state().await;
                for event in recent_events(room, watermark).await? {
                    if R::relevant(&state, accepted_id, &event) {
                        let preflight = match event.content {
                            EnvelopePayload::DebatePreflight(p) => {
                                Some(PendingPreflight::Debate(p))
                            }
                            EnvelopePayload::CrosscheckPreflight(p) => {
                                Some(PendingPreflight::Contest(p.base))
                            }
                            EnvelopePayload::ContestPreflight(p) => {
                                Some(PendingPreflight::Contest(p))
                            }
                            _ => None,
                        };
                        if let Some(preflight) = preflight {
                            output
                                .preflight(accepted_id, &preflight)
                                .map_err(io_error)?;
                        }
                    }
                }
                output
                    .ack(accepted_id, event_ids, cursor)
                    .map_err(io_error)?;
                if args.no_wait {
                    return Ok(0);
                }
                break accepted_id;
            }
            RequestResult::Internal(_) => {
                return Err(RoomError {
                    code: ErrorCode::INVALID_ENVELOPE,
                    message: "Unexpected internal command response.".into(),
                    retryable: false,
                });
            }
        }
    };
    let mut usage = std::collections::BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut rounds = std::collections::BTreeMap::<ContestId, u32>::new();
    loop {
        let state = room.state().await;
        // Capture progress before history, then refresh relevance after reading it:
        // terminal progress cannot outrun the final page, and newly linked events
        // cannot be discarded using a relationship snapshot older than the event.
        let progress = R::progress(&state, request_id);
        let events = recent_events(room, watermark).await?;
        let state = room.state().await;
        for event in events {
            if R::relevant(&state, request_id, &event) && seen.insert(event.base.id) {
                let latest_round = match &event.content {
                    EnvelopePayload::DebateState(state) => {
                        let round = rounds.entry(state.debate_id).or_default();
                        *round = (*round).max(state.round);
                        Some(*round)
                    }
                    EnvelopePayload::DebateTurn(turn) => {
                        let round = rounds.entry(turn.debate_id).or_default();
                        *round = (*round).max(turn.round);
                        Some(*round)
                    }
                    EnvelopePayload::DebateRoundScore(score) => {
                        rounds.get(&score.debate_id).copied()
                    }
                    _ => None,
                };
                if let EnvelopePayload::UsageRecord(record) = &event.content {
                    usage.insert((record.work_id, record.attempt), event.clone());
                    if !args.verbose {
                        watermark = Some(event.commit.seq);
                        continue;
                    }
                }
                output
                    .event_with_round(&event, latest_round)
                    .map_err(io_error)?;
            }
            watermark = Some(event.commit.seq);
        }
        if progress != Progress::Running {
            if !usage.is_empty() {
                let mut total = 0.0;
                let mut unknown = 0;
                for event in usage.values() {
                    if let EnvelopePayload::UsageRecord(record) = &event.content {
                        match record.usage.as_ref().and_then(|u| u.cost_usd) {
                            Some(cost) => total += cost,
                            None => unknown += 1,
                        }
                    }
                    if !args.verbose {
                        output.event(event).map_err(io_error)?;
                    }
                }
                if output.json {
                    output.record(json!({"type":"usage.total","calls":usage.len(),"knownCostUsd":total,"unknownCostCalls":unknown})).map_err(io_error)?;
                } else {
                    writeln!(
                        output.stdout,
                        "Usage: {} calls; known cost ${total:.6}; unknown-cost calls: {unknown}",
                        usage.len()
                    )
                    .map_err(io_error)?;
                }
            }
            return Ok(u8::from(progress == Progress::Failed));
        }
        // Each evaluation re-reads and re-links the whole history, so wait for the
        // feed (a safety poll covers a missed wakeup) and coalesce bursts into one pass.
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500)) => {},
            received = feed.recv() => {
                if matches!(received, Err(tokio::sync::broadcast::error::RecvError::Closed)) {
                    return Err(RoomError { code: ErrorCode::NOT_READY, message: "Room feed closed before request completion.".into(), retryable: true });
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                while let Ok(_item) = feed.try_recv() {}
            }
        }
    }
}

/// Read all committed events after a high watermark via the backward paging API.
async fn recent_events(
    room: &impl RoomApi,
    watermark: Option<Cursor>,
) -> Result<Vec<CommittedEnvelope>, RoomError> {
    let mut before = None;
    let mut events = Vec::new();
    loop {
        let page = room.page_events(before, 500).await?;
        if page.is_empty() {
            break;
        }
        let oldest = page[0].commit.seq;
        events.extend(
            page.into_iter()
                .filter(|event| watermark.is_none_or(|cursor| event.commit.seq > cursor)),
        );
        if watermark.is_some_and(|cursor| oldest <= cursor) {
            break;
        }
        before = Some(oldest);
    }
    events.sort_by_key(|event| event.commit.seq);
    Ok(events)
}

fn reservation_diagnostic(
    state: &serde_json::Value,
    requested: Option<f64>,
    fallback: &str,
) -> String {
    // Core admission rejections already carry spent, cap and required reservation.
    if fallback.starts_with("Daily max of $") && fallback.contains("(Spent $") {
        return fallback.to_owned();
    }
    let spend = &state["dailySpend"];
    let (Some(spent), Some(limit)) = (spend["spentUsd"].as_f64(), spend["limitUsd"].as_f64())
    else {
        return fallback.to_owned();
    };
    let max_micros = (limit * 1e6).round() as i64;
    let spent_micros = (spent * 1e6).round() as i64;
    match requested {
        Some(amount) => squad_store::accounting::daily_max_text(
            max_micros,
            Some((spent_micros, (amount * 1e6).round() as i64)),
        ),
        None => format!(
            "{} (Spent ${spent:.6} today; this request's conservative reservation will not fit.)",
            squad_store::accounting::daily_max_text(max_micros, None)
        ),
    }
}

#[cfg(test)]
mod diagnostic_tests {
    #[test]
    fn low_spend_reservation_refusal_does_not_claim_cap_was_spent() {
        let text = super::reservation_diagnostic(
            &serde_json::json!({"dailySpend":{"spentUsd":0.18,"limitUsd":2.5}}),
            Some(3.0),
            "DAILY_MAX_REACHED",
        );
        insta::assert_snapshot!(text, @"Daily max of $2.50 reached. Raise SQUAD_DAILY_MAX_USD or wait until tomorrow. (Spent $0.180000 today; this request needs up to $3.000000 reserved.)");
    }
}
