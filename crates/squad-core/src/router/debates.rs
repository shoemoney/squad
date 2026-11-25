//! Versioned debate lifecycle commits. The engine chooses signals; this gate
//! checks their durable evidence and never runs a round strategy.
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};

pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid debate transition",
    )
}
pub fn save(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &DebateState,
    expected: Option<u64>,
) -> Result<()> {
    state.validate().map_err(|_| invalid())?;
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DebateState(state.clone()),
        ),
    )?;
    squad_store::contests::save_debate(tx, state, expected, seq)?;
    squad_store::contests::associate(tx, state.debate_id, ContestKind::Debate, seq)?;
    if state.state.is_terminal() {
        let mut reservations=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
        let rows = reservations
            .query_map([state.debate_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (reservation, day, amount) in rows {
            tx.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![amount, day],
            )?;
            tx.execute(
                "DELETE FROM reservations WHERE reservation_id=?",
                [reservation],
            )?;
        }
        squad_store::history::release(tx, state.debate_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'", [state.debate_id.to_string()])?;
    }
    Ok(())
}
pub fn prepare(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    request: &Request,
    bindings: &std::collections::BTreeMap<String, ExecutorBinding>,
) -> Result<()> {
    let Request::UserMessage {
        parsed:
            ParsedInput::Debate {
                debaters,
                rounds,
                judge_requested,
                ..
            },
        ..
    } = request
    else {
        return Err(invalid());
    };
    let state = DebateState {
        debate_id: ContestId::new(),
        state: DebateStatus::Preparing,
        debaters: debaters.clone(),
        round: 0,
        total_rounds: u32::from(*rounds),
        judge_model: bindings
            .get("judge")
            .map(|b| b.model.to_string())
            .unwrap_or_default(),
        judge_requested: *judge_requested,
        state_version: 1,
        terminal_reason: None,
        case_id: None,
        case_ids: vec![],
    };
    save(tx, group, id, &state, None)?;
    tx.execute(
        "UPDATE contests SET pending_request_json=? WHERE contest_id=?",
        params![serde_json::to_string(request)?, state.debate_id.to_string()],
    )?;
    let Request::UserMessage { input, .. } = request else {
        return Err(invalid());
    };
    let mut sources = Vec::new();
    for source in input.reply_to.iter().flatten() {
        sources.push(
            squad_store::events::by_id(tx, *source)?.ok_or(StoreError::Domain(
                ErrorCode::INVALID_REFERENCE,
                "Invalid solution reference",
            ))?,
        );
    }
    if sources.is_empty() {
        for nick in debaters {
            let seq:Option<i64>=tx.query_row("SELECT MAX(seq) FROM events WHERE kind='message.final' AND sender=? AND NOT EXISTS(SELECT 1 FROM superseded WHERE superseded_event_id=events.id)",[nick.as_str()],|r|r.get(0)).optional()?.flatten();
            if let Some(seq) = seq {
                sources.push(squad_store::events::at(tx, seq)?);
            }
        }
    }
    let quotes = super::references::quotes(tx, input.quote.as_deref().unwrap_or_default())?;
    squad_store::contest_steps::private_map(
        tx,
        state.debate_id,
        "admission",
        &serde_json::json!({"request":request,"sources":sources,"quotes":quotes}),
    )?;
    Ok(())
}
fn evidence(db: &Connection, event: EventId, state: &DebateState, kind: &str) -> Result<()> {
    let event = squad_store::events::by_id(db, event)?.ok_or_else(invalid)?;
    let valid = match (&event.content, kind) {
        (EnvelopePayload::DebateTurn(t), "turn") => {
            t.debate_id == state.debate_id
                && t.round == state.round
                && state.debaters.contains(&t.speaker)
        }
        (EnvelopePayload::DebateRoundScore(s), "score") => s.validate_against(state).is_ok(),
        (EnvelopePayload::Verdict(v), "verdict") => v
            .validate_against(state, &squad_store::round_scores::tally(db, state)?)
            .is_ok(),
        _ => false,
    };
    if !valid {
        return Err(invalid());
    }
    Ok(())
}
pub fn transition(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let (contest, version, signal) = submission.into_debate().map_err(|_| invalid())?;
    let mut state = squad_store::contests::debate(tx, contest)?;
    if state.state_version != version {
        return Err(invalid());
    }
    if matches!(
        signal,
        DebateTransition::Confirm
            | DebateTransition::JudgeOnlyConfirmed
            | DebateTransition::Cancel
            | DebateTransition::JudgeOnlyCancelled
            | DebateTransition::Expire
            | DebateTransition::JudgeOnlyExpired
    ) {
        let plan: Option<String> = tx.query_row(
            "SELECT plan_id FROM contests WHERE contest_id=?",
            [contest.to_string()],
            |r| r.get(0),
        )?;
        let plan = plan.ok_or_else(invalid)?.parse().map_err(|_| invalid())?;
        if matches!(
            signal,
            DebateTransition::Expire | DebateTransition::JudgeOnlyExpired
        ) {
            let expired: bool = tx.query_row(
                "SELECT expires_at<=? FROM contests WHERE contest_id=?",
                params![group.accepted_at.to_rfc3339(), contest.to_string()],
                |r| r.get(0),
            )?;
            if !expired {
                return Err(invalid());
            }
        }
        super::preflight::resolve(
            tx,
            group,
            id,
            contest,
            plan,
            matches!(
                signal,
                DebateTransition::Cancel | DebateTransition::JudgeOnlyCancelled
            ),
        )?;
        return Ok(());
    }
    if matches!(
        signal,
        DebateTransition::OpeningBarrierCommitted
            | DebateTransition::NumberedBarrierCommitted
            | DebateTransition::JudgedFinalBarrierCommitted
            | DebateTransition::UnjudgedFinalBarrierCommitted
    ) {
        let committed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM contest_steps WHERE contest_id=? AND committed=1 AND updated_seq>(SELECT updated_seq FROM contests WHERE contest_id=?))",params![contest.to_string(),contest.to_string()],|r|r.get(0))?;
        if !committed {
            return Err(invalid());
        }
    }
    use DebateStatus as S;
    use DebateTransition as T;
    let next = match signal {
        T::Prepare if state.state == S::Preparing => S::Preparing,
        T::PlanReady | T::JudgeOnlyPlanReady if state.state == S::Preparing => {
            let ready: bool = tx.query_row(
                "SELECT plan_json IS NOT NULL FROM contests WHERE contest_id=?",
                [contest.to_string()],
                |r| r.get(0),
            )?;
            if !ready {
                return Err(invalid());
            }
            S::AwaitingConfirmation
        }
        T::PlanRejected if state.state == S::Preparing => S::Rejected,
        T::Confirm if state.state == S::AwaitingConfirmation => S::WaitingForDebaters,
        T::Cancel if state.state == S::AwaitingConfirmation => S::Cancelled,
        T::Expire if state.state == S::AwaitingConfirmation => S::Expired,
        T::WaitForDebaters if state.state == S::WaitingForDebaters => S::WaitingForDebaters,
        T::DebatersReady if state.state == S::WaitingForDebaters => {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL AND target IN (?,?))",params![state.debaters[0].as_str(),state.debaters[1].as_str()],|r|r.get(0))?;
            if pending {
                return Err(invalid());
            }
            S::RoundRunning
        }
        T::RoundEntered if state.state == S::RoundRunning => {
            let count:i64=tx.query_row("SELECT COUNT(*) FROM work WHERE debate_id=? AND round=? AND kind='debate.turn' AND history_through IS NOT NULL",params![contest.to_string(),state.round],|r|r.get(0))?;
            if count != 2 {
                return Err(invalid());
            }
            S::RoundRunning
        }
        T::SlotResolved { outcome_event_id } if state.state == S::RoundRunning => {
            evidence(tx, outcome_event_id, &state, "turn")?;
            S::RoundRunning
        }
        T::RoundResolved if state.state == S::RoundRunning => {
            let count:i64=tx.query_row("SELECT COUNT(DISTINCT json_extract(payload,'$.speaker')) FROM events WHERE kind='debate.turn' AND json_extract(payload,'$.debateId')=? AND json_extract(payload,'$.round')=?",params![contest.to_string(),state.round],|r|r.get(0))?;
            if count != 2 {
                return Err(invalid());
            }
            S::RoundCommitting
        }
        T::OpeningBarrierCommitted if state.state == S::RoundCommitting && state.round == 0 => {
            state.round = 1;
            S::RoundRunning
        }
        T::NumberedBarrierCommitted
            if state.state == S::RoundCommitting
                && state.round > 0
                && state.round < state.total_rounds =>
        {
            state.round += 1;
            S::RoundRunning
        }
        T::JudgedFinalBarrierCommitted
            if state.state == S::RoundCommitting
                && state.round == state.total_rounds
                && state.judge_requested =>
        {
            S::Judging
        }
        T::UnjudgedFinalBarrierCommitted
            if state.state == S::RoundCommitting
                && state.round == state.total_rounds
                && !state.judge_requested =>
        {
            S::CompleteUnjudged
        }
        T::VerdictAccepted { verdict_event_id } if state.state == S::Judging => {
            evidence(tx, verdict_event_id, &state, "verdict")?;
            S::Completed
        }
        T::FinalJudgingFailed if state.state == S::Judging => S::Failed,
        T::StopRequested if !state.state.is_terminal() => S::Stopping,
        T::StopCommitted if state.state == S::Stopping => S::Stopped,
        T::UnrecoverableFailure if !state.state.is_terminal() => S::Failed,
        T::RoundScoreAccepted { score_event_id }
            if matches!(
                state.state,
                S::RoundRunning | S::RoundCommitting | S::Judging
            ) =>
        {
            evidence(tx, score_event_id, &state, "score")?;
            state.state.clone()
        }
        T::RoundScoringFinished if state.state == S::Judging => {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE debate_id=? AND kind='judge.round' AND status IN ('queued','running'))",[contest.to_string()],|r|r.get(0))?;
            if pending {
                return Err(invalid());
            }
            S::Judging
        }
        T::JudgeOnlyRequested
            if matches!(state.state, S::Stopped | S::CompleteUnjudged | S::Failed) =>
        {
            tx.execute(
                "UPDATE contests SET pending_request_json=? WHERE contest_id=?",
                params![
                    serde_json::json!({"priorState":state}).to_string(),
                    contest.to_string()
                ],
            )?;
            S::Preparing
        }
        T::JudgeOnlyConfirmed if state.state == S::AwaitingConfirmation => S::Judging,
        T::JudgeOnlyCancelled | T::JudgeOnlyExpired | T::JudgeOnlyPlanRejected
            if matches!(state.state, S::Preparing | S::AwaitingConfirmation) =>
        {
            let pending: String = tx.query_row(
                "SELECT pending_request_json FROM contests WHERE contest_id=?",
                [contest.to_string()],
                |r| r.get(0),
            )?;
            let prior: DebateState = serde_json::from_value(
                serde_json::from_str::<serde_json::Value>(&pending)?["priorState"].clone(),
            )?;
            state = prior;
            state.state.clone()
        }
        _ => return Err(invalid()),
    };
    state.state = next;
    state.state_version = version + 1;
    save(tx, group, id, &state, Some(version))
}
