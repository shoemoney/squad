//! MCP operations over the same room, receipts, and budget ledger as the CLI.
use super::types::{ConfirmArgs, RunArgs, StartArgs};
use crate::{adapter::State, room_api::RoomApi};
use serde_json::{Value, json};
use squad_core::{room::RoomHandle, state::ContestActivity};
use squad_protocol::{parser::SubmissionMeta, *};
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

pub struct Backend {
    config: Result<SquadConfig, String>,
    allow_self_confirm: bool,
    room: Mutex<RoomSlot>,
}

enum RoomSlot {
    Unopened,
    Open(Box<RoomHandle>),
    Closed,
}

#[derive(Debug)]
pub struct BackendError {
    pub code: ErrorCode,
    pub message: String,
}

impl BackendError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn conflict() -> Self {
        Self::new(
            ErrorCode::IDEMPOTENCY_CONFLICT,
            "This request_id already belongs to a different submission.",
        )
    }

    fn not_found() -> Self {
        Self::new(ErrorCode::CONTEST_NOT_FOUND, "Tournament run not found.")
    }

    fn expired() -> Self {
        Self::new(
            ErrorCode::CONTEST_PLAN_EXPIRED,
            "The tournament cost estimate has expired.",
        )
    }

    fn rejected(error: &ErrorPayload) -> Self {
        use ErrorCode::*;
        let message = match error.code {
            IDEMPOTENCY_CONFLICT => return Self::conflict(),
            DAILY_MAX_REACHED => "The tournament reservation exceeds the remaining daily budget.",
            CONTEST_BUSY => "A contest is already active in this room.",
            CONTEST_PLAN_EXPIRED => "The tournament cost estimate has expired.",
            MODEL_UNAVAILABLE => "A tournament entrant or the judge is unavailable.",
            INVALID_STATE_TRANSITION => "The tournament has changed state.",
            NOTHING_TO_STOP => "The tournament is no longer active.",
            _ => "Squad rejected the tournament operation.",
        };
        Self::new(error.code, message)
    }
}

impl Backend {
    pub fn new(config: Result<SquadConfig, String>, allow_self_confirm: bool) -> Self {
        Self {
            config,
            allow_self_confirm,
            room: Mutex::new(RoomSlot::Unopened),
        }
    }

    fn check_confirmation(&self, confirm: bool) -> Result<(), BackendError> {
        if confirm && !self.allow_self_confirm {
            return Err(BackendError::new(
                ErrorCode::INVALID_COMMAND,
                "MCP self-confirmation is disabled. A human must confirm in the TUI, or restart the server with --allow-self-confirm.",
            ));
        }
        Ok(())
    }

    async fn room(&self) -> Result<MutexGuard<'_, RoomSlot>, BackendError> {
        let mut slot = self.room.lock().await;
        match &*slot {
            RoomSlot::Closed => {
                return Err(BackendError::new(
                    ErrorCode::NOT_READY,
                    "The MCP room has shut down.",
                ));
            }
            RoomSlot::Open(_) => return Ok(slot),
            RoomSlot::Unopened => {}
        }
        let config = self.config.as_ref().map_err(|_| {
            BackendError::new(
                ErrorCode::CONFIG_INVALID,
                "Valid Squad configuration and an OpenRouter API key are required.",
            )
        })?;
        let room = RoomHandle::open(config.clone(), "main", OpenMode::Interactive)
            .await
            .map_err(|error| {
                BackendError::new(error.code, "Could not open the configured room.")
            })?;
        *slot = RoomSlot::Open(Box::new(room));
        Ok(slot)
    }

    pub async fn roster(&self) -> Result<Value, BackendError> {
        let slot = self.room().await?;
        let state =
            slot.handle().try_state().await.map_err(|error| {
                BackendError::new(error.code, "Could not read the room roster.")
            })?;
        Ok(json!({
            "room": state.room,
            "participants": state.participants,
            "models": state.models,
            "dailySpend": state.daily_spend,
            "trophies": state.trophies,
        }))
    }

    pub async fn list_runs(&self) -> Result<Value, BackendError> {
        let slot = self.room().await?;
        let state = read_snapshot(slot.handle()).await?;
        let mut runs: Vec<_> = project_runs(&state.events).into_values().collect();
        runs.sort_unstable_by_key(|run| std::cmp::Reverse(run.started_at));
        let mut summaries = Vec::with_capacity(runs.len());
        for run in &runs {
            summaries.push(summarize(slot.handle(), run).await?);
        }
        Ok(json!({"runs": summaries}))
    }

    pub async fn start_tournament(&self, args: StartArgs) -> Result<Value, BackendError> {
        self.check_confirmation(args.confirm)?;
        let raw = tournament_command(&args)?;
        let id = match args.request_id.as_deref() {
            Some(key) => request_id(key)?,
            None => EventId::new(),
        };
        let slot = self.room().await?;
        let room = slot.handle();
        let state = expire_and_read_state(room).await?;
        let runs = project_runs(&state.events);

        if let Some(run) = runs.values().find(|run| run.id == id) {
            // Use the original entrants so the receipt remains comparable after
            // roster changes, and submit the caller's requested confirmation.
            let preflight = run.preflight.ok_or_else(BackendError::conflict)?;
            let context = parser::ParseContext {
                roster: preflight.participants.clone(),
                ..Default::default()
            };
            let request = start_request(id, &raw, &context, args.confirm)
                .map_err(|_| BackendError::conflict())?;
            check_submission(room.submit(request).await)?;
            return summary_for(room, id).await;
        }

        let context = RoomHandle::parse_context(&state);
        let request = start_request(id, &raw, &context, args.confirm)?;
        check_submission(room.submit(request).await)?;
        summary_for(room, id).await
    }

    pub async fn confirm(&self, args: ConfirmArgs) -> Result<Value, BackendError> {
        self.check_confirmation(args.confirm)?;
        let id = run_id(&args.run_id)?;
        let slot = self.room().await?;
        let room = slot.handle();
        let state = expire_and_read_state(room).await?;
        let runs = project_runs(&state.events);
        let run = find_run(&runs, id)?;
        let confirmation = match submit_confirmation(room, run, args.confirm).await {
            Err(error) if error.code != ErrorCode::INVALID_STATE_TRANSITION => return Err(error),
            confirmation => confirmation,
        };
        let summary = summary_for(room, id).await?;
        if summary["status"] == "expired" {
            return Err(BackendError::expired());
        }
        confirmation?;
        Ok(summary)
    }

    pub async fn cancel(&self, args: RunArgs) -> Result<Value, BackendError> {
        let id = run_id(&args.run_id)?;
        let slot = self.room().await?;
        let room = slot.handle();
        let state = expire_and_read_state(room).await?;
        let runs = project_runs(&state.events);
        let run = find_run(&runs, id)?;
        let cancellation = match run.status() {
            "pending_confirmation" => submit_confirmation(room, run, false).await,
            "running" => {
                let request = Request::ContestStop {
                    request_id: EventId::new(),
                    contest_id: run.state.contest_id,
                };
                check_submission(room.submit(request).await)
            }
            _ => return summarize(room, run).await,
        };
        match cancellation {
            Ok(()) => summary_for(room, id).await,
            Err(error)
                if matches!(
                    error.code,
                    ErrorCode::NOTHING_TO_STOP | ErrorCode::INVALID_STATE_TRANSITION
                ) =>
            {
                let state = read_snapshot(room).await?;
                let runs = project_runs(&state.events);
                let current = find_run(&runs, id)?;
                if current.state.state.is_terminal() {
                    return summarize(room, current).await;
                }
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn status(&self, args: RunArgs) -> Result<Value, BackendError> {
        let id = run_id(&args.run_id)?;
        let slot = self.room().await?;
        summary_for(slot.handle(), id).await
    }

    pub async fn result(&self, args: RunArgs) -> Result<Value, BackendError> {
        let id = run_id(&args.run_id)?;
        let slot = self.room().await?;
        let room = slot.handle();
        let state = read_snapshot(room).await?;
        let runs = project_runs(&state.events);
        let run = find_run(&runs, id)?;
        if matches!(run.status(), "running" | "pending_confirmation") {
            return Err(BackendError::new(
                ErrorCode::NOT_READY,
                "The tournament is still in progress.",
            ));
        }
        let contest_id = run.state.contest_id;
        let winner_plan = run.champion.and_then(|champion| {
            // Only select an answer from a round whose ranking was accepted.
            // If the winner forfeited the last round, use their last ranked answer.
            state.events.iter().rev().find_map(|event| {
                let EnvelopePayload::TournamentEntry(entry) = &event.content else {
                    return None;
                };
                if entry.contest_id == contest_id
                    && entry.entrant == champion.champion
                    && run.rankings.iter().any(|ranking| {
                        ranking.round == entry.round && ranking.replay == entry.replay
                    })
                    && let DebateTurnOutcome::Completed { content, .. } = &entry.outcome
                {
                    return Some(content);
                }
                None
            })
        });
        let markdown = RoomApi::export(room, ExportScope::Tournament, Some(contest_id))
            .await
            .map_err(|error| BackendError::new(error.code, "Could not export the tournament."))?;
        let mut result = summarize(room, run).await?;
        result["champion"] = json!(run.champion.map(|c| &c.champion));
        result["winnerPlan"] = json!(winner_plan);
        result["rankings"] = json!(run.rankings);
        result["placements"] = json!(run.champion.map(|c| &c.placements));
        result["markdown"] = json!(markdown);
        Ok(result)
    }

    pub async fn shutdown(&self) -> Result<(), BackendError> {
        let mut slot = self.room.lock().await;
        if let RoomSlot::Open(room) = std::mem::replace(&mut *slot, RoomSlot::Closed) {
            RoomApi::shutdown(room.as_ref())
                .await
                .map_err(|error| BackendError::new(error.code, "Could not shut down the room."))?;
        }
        Ok(())
    }
}

impl RoomSlot {
    fn handle(&self) -> &RoomHandle {
        match self {
            Self::Open(room) => room,
            _ => unreachable!("room access requires an open room"),
        }
    }
}

fn request_id(raw: &str) -> Result<EventId, BackendError> {
    let key = Uuid::parse_str(raw)
        .map_err(|_| BackendError::new(ErrorCode::INVALID_COMMAND, "request_id must be a UUID."))?;
    stable_request_id("squad:mcp:tournament:start", &key.to_string())
}

fn stable_request_id(namespace: &str, key: &str) -> Result<EventId, BackendError> {
    // Protocol receipts accept UUIDv7 only. Derive a distinct stable
    // identity for each operation on a client key or run.
    let digest = squad_store::commit::digest(&(namespace, key))
        .map_err(|_| BackendError::new(ErrorCode::INTERNAL_ERROR, "Could not create run ID."))?;
    format!(
        "{}-{}-7{}-8{}-{}",
        &digest[..8],
        &digest[8..12],
        &digest[13..16],
        &digest[17..20],
        &digest[20..32],
    )
    .parse()
    .map_err(|_| BackendError::new(ErrorCode::INTERNAL_ERROR, "Could not create run ID."))
}

fn run_id(raw: &str) -> Result<EventId, BackendError> {
    raw.parse().map_err(|_| {
        BackendError::new(ErrorCode::INVALID_COMMAND, "run_id must be a Squad run ID.")
    })
}

fn tournament_command(args: &StartArgs) -> Result<String, BackendError> {
    if args.prompt.trim().is_empty() {
        return Err(BackendError::new(
            ErrorCode::INVALID_COMMAND,
            "A brief is required.",
        ));
    }
    let mut raw = match &args.participants {
        None => "@all".to_owned(),
        Some(names) => {
            let mut seen = BTreeSet::new();
            let mut mentions = Vec::new();
            for name in names {
                let nick: MemberNick = name.parse().map_err(|_| {
                    BackendError::new(
                        ErrorCode::UNKNOWN_RECIPIENT,
                        "Invalid participant nickname.",
                    )
                })?;
                if !seen.insert(nick.clone()) {
                    return Err(BackendError::new(
                        ErrorCode::INVALID_COMMAND,
                        "Participant nicknames must be distinct.",
                    ));
                }
                mentions.push(format!("@{nick}"));
            }
            if mentions.len() < 3 {
                return Err(BackendError::new(
                    ErrorCode::TOURNAMENT_ENTRANTS_REQUIRED,
                    "A tournament requires at least three entrants.",
                ));
            }
            mentions.join(" ")
        }
    };
    raw.push_str(" $tournament ");
    for ch in args.prompt.chars() {
        // Treat the brief as text even if it contains Squad control syntax.
        if matches!(ch, '@' | '$' | '\\' | '`') {
            raw.push('\\');
        }
        raw.push(ch);
    }
    Ok(raw)
}

fn start_request(
    id: EventId,
    raw: &str,
    context: &parser::ParseContext,
    confirm: bool,
) -> Result<Request, BackendError> {
    let meta = SubmissionMeta::default();
    let parsed = parse(raw, &meta, context).map_err(|error| match error.code {
        ErrorCode::CONTEST_BUSY => {
            BackendError::new(error.code, "A contest is already active in this room.")
        }
        ErrorCode::UNKNOWN_RECIPIENT => {
            BackendError::new(error.code, "A participant is absent from the room roster.")
        }
        ErrorCode::TOURNAMENT_ENTRANTS_REQUIRED => {
            BackendError::new(error.code, "A tournament requires at least three entrants.")
        }
        ErrorCode::MESSAGE_TOO_LARGE => BackendError::new(error.code, "The brief is too long."),
        _ => BackendError::new(error.code, "Invalid tournament brief."),
    })?;
    if !matches!(parsed, ParsedInput::Tournament { .. }) {
        return Err(BackendError::new(
            ErrorCode::INVALID_COMMAND,
            "Invalid tournament brief.",
        ));
    }
    Request::from_parsed_with_context(id, parsed, &meta, context, confirm)
        .map_err(|error| BackendError::new(error.code, "Invalid tournament request."))
}

async fn expire_and_read_state(room: &RoomHandle) -> Result<State, BackendError> {
    let state = read_snapshot(room).await?;
    let Some(ContestState::Tournament(active)) = &state.room.active_contest else {
        return Ok(state);
    };
    if active.state != TournamentStatus::Preflight {
        return Ok(state);
    }
    let plan = state.events.iter().rev().find_map(|event| {
        if let EnvelopePayload::TournamentPreflight(preflight) = &event.content
            && preflight.base.contest_id == active.contest_id
        {
            return Some(&preflight.base);
        }
        None
    });
    let Some(plan) = plan.filter(|plan| plan.expires_at <= chrono::Utc::now()) else {
        return Ok(state);
    };
    let request = Request::ContestExpire {
        request_id: EventId::new(),
        contest_id: plan.contest_id,
        plan_id: plan.plan_id,
    };
    if let Err(error) = check_submission(room.submit(request).await)
        && error.code != ErrorCode::INVALID_STATE_TRANSITION
    {
        return Err(error);
    }
    read_snapshot(room).await
}

async fn read_snapshot(room: &RoomHandle) -> Result<State, BackendError> {
    let state = RoomApi::state(room).await;
    if state.read_failed {
        return Err(BackendError::new(
            ErrorCode::UNAVAILABLE,
            "Could not read tournament history.",
        ));
    }
    Ok(state)
}

fn check_submission(result: RequestResult) -> Result<(), BackendError> {
    match result {
        RequestResult::Accepted { .. } | RequestResult::PendingConfirmation { .. } => Ok(()),
        RequestResult::Rejected { error, .. } => Err(BackendError::rejected(&error)),
        RequestResult::Internal(_) => Err(BackendError::new(
            ErrorCode::INTERNAL_ERROR,
            "Unexpected tournament receipt.",
        )),
    }
}

async fn submit_confirmation(
    room: &RoomHandle,
    run: &Run<'_>,
    confirm: bool,
) -> Result<(), BackendError> {
    let preflight = run.preflight.ok_or_else(|| {
        BackendError::new(
            ErrorCode::NOT_READY,
            "Tournament cost estimate is unavailable.",
        )
    })?;
    let request_id = stable_request_id("squad:mcp:tournament:confirm", &run.id.to_string())?;
    let contest_id = preflight.contest_id;
    let plan_id = preflight.plan_id;
    let request = if confirm {
        Request::ContestConfirm {
            request_id,
            contest_id,
            plan_id,
        }
    } else {
        Request::ContestCancel {
            request_id,
            contest_id,
            plan_id,
        }
    };
    check_submission(room.submit(request).await)
}

async fn summary_for(room: &RoomHandle, id: EventId) -> Result<Value, BackendError> {
    let state = read_snapshot(room).await?;
    let runs = project_runs(&state.events);
    summarize(room, find_run(&runs, id)?).await
}

async fn summarize(room: &RoomHandle, run: &Run<'_>) -> Result<Value, BackendError> {
    let activity = room
        .contest_activity(run.state.contest_id)
        .await
        .map_err(|error| BackendError::new(error.code, "Could not read tournament activity."))?;
    Ok(run.summary(&activity))
}

fn find_run<'a>(
    runs: &'a BTreeMap<ContestId, Run<'a>>,
    id: EventId,
) -> Result<&'a Run<'a>, BackendError> {
    runs.values()
        .find(|run| run.id == id)
        .ok_or_else(BackendError::not_found)
}

struct Run<'a> {
    id: EventId,
    started_at: Cursor,
    state: &'a TournamentState,
    preflight: Option<&'a ContestPreflight>,
    preflight_result: Option<&'a ContestPreflightResult>,
    errors: BTreeSet<String>,
    rankings: Vec<&'a TournamentRanking>,
    champion: Option<&'a TournamentChampion>,
}

impl<'a> Run<'a> {
    fn new(event: &CommittedEnvelope, state: &'a TournamentState) -> Self {
        Self {
            id: event.base.correlation_id,
            started_at: event.commit.seq,
            state,
            preflight: None,
            preflight_result: None,
            errors: BTreeSet::new(),
            rankings: Vec::new(),
            champion: None,
        }
    }

    fn status(&self) -> &'static str {
        match self.state.state {
            TournamentStatus::Preflight => "pending_confirmation",
            TournamentStatus::Complete => "complete",
            TournamentStatus::Failed => "failed",
            TournamentStatus::Stopped
                if self
                    .preflight_result
                    .is_some_and(|r| r.status == PreflightStatus::Expired) =>
            {
                "expired"
            }
            TournamentStatus::Stopped => "cancelled",
            _ => "running",
        }
    }

    fn update_state(&mut self, state: &'a TournamentState) {
        if state.terminal_reason.as_deref() == Some("DAILY_MAX_REACHED") {
            self.errors
                .insert(format!("{:?}", ErrorCode::DAILY_MAX_REACHED));
        }
        self.state = state;
    }

    fn summary(&self, activity: &ContestActivity) -> Value {
        json!({
            "runId": self.id,
            "contestId": self.state.contest_id,
            "status": self.status(),
            "phase": self.state.state,
            "round": self.state.round,
            "replay": self.state.replay,
            "participants": self.preflight.map(|p| &p.participants),
            "estimatedUsd": self.preflight.map(|p| p.estimate.estimated_usd),
            "reservedUsd": self.preflight.map(|p| p.estimate.reserved_usd),
            "expiresAt": self.preflight.map(|p| p.expires_at),
            "work": activity.work,
            "usage": activity.usage,
            "errors": self.errors,
        })
    }
}

fn project_runs(events: &[CommittedEnvelope]) -> BTreeMap<ContestId, Run<'_>> {
    let mut runs = BTreeMap::new();
    for event in events {
        let contest_id = match &event.content {
            EnvelopePayload::TournamentState(state)
            | EnvelopePayload::ContestState(ContestState::Tournament(state)) => {
                let run = runs
                    .entry(state.contest_id)
                    .or_insert_with(|| Run::new(event, state));
                run.update_state(state);
                continue;
            }
            EnvelopePayload::TournamentPreflight(preflight) => Some(preflight.base.contest_id),
            EnvelopePayload::ContestPreflightResult(result) => Some(result.contest_id),
            EnvelopePayload::WorkState(work) => work.contest_id,
            EnvelopePayload::Error(error) => error.contest_id,
            EnvelopePayload::TournamentRanking(ranking) => Some(ranking.contest_id),
            EnvelopePayload::TournamentChampion(champion) => Some(champion.contest_id),
            _ => None,
        };
        let Some(run) = contest_id.and_then(|id| runs.get_mut(&id)) else {
            continue;
        };
        match &event.content {
            EnvelopePayload::TournamentPreflight(preflight) => {
                run.id = event.base.correlation_id;
                run.preflight = Some(&preflight.base);
            }
            EnvelopePayload::ContestPreflightResult(result) => {
                run.preflight_result = Some(result);
                if let Some(error) = &result.error {
                    run.errors.insert(format!("{:?}", error.code));
                }
            }
            EnvelopePayload::WorkState(work) => {
                if let Some(error) = &work.error {
                    run.errors.insert(format!("{:?}", error.code));
                }
            }
            EnvelopePayload::Error(error) => {
                run.errors.insert(format!("{:?}", error.code));
            }
            EnvelopePayload::TournamentRanking(ranking) => run.rankings.push(ranking),
            EnvelopePayload::TournamentChampion(champion) => run.champion = Some(champion),
            _ => {}
        }
    }
    runs
}
