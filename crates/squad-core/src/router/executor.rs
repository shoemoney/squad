//! Attempt lifecycle extensions. All transitions and billing run on the writer.
use super::{envelope, error};
use rusqlite::params;
use squad_protocol::*;
use squad_store::{
    Result, Store, StoreError,
    commit::{CommitGroup, CommitOutcome},
};

pub fn account(
    store: &Store,
    owner: ExecutorKey,
    epoch: u64,
    mut record: UsageRecord,
    max: i64,
) -> Result<CommitOutcome> {
    record.validate().map_err(|_| {
        StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Invalid accounting record",
        )
    })?;
    let id = EventId::new();
    let digest = squad_store::commit::digest(&(epoch, &record))?;
    store.commit(
        id,
        serde_json::to_string(&owner)?,
        digest,
        move |tx, group| {
            // Billing survives cancellation, restart and clear. Authenticate against
            // the original durable provider start rather than the current work lease.
            let valid: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM attempts WHERE work_id=? AND attempt=? AND epoch=?)",
                params![record.work_id.to_string(), record.attempt, epoch as i64],
                |r| r.get(0),
            )?;
            if !valid {
                return Err(StoreError::Domain(
                    ErrorCode::STALE_WORK_ATTEMPT,
                    "Accounting has no matching provider start",
                ));
            }
            let (day, started): (String, String) = tx.query_row(
                "SELECT day,started_at FROM attempts WHERE work_id=? AND attempt=?",
                params![record.work_id.to_string(), record.attempt],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            record.day = day;
            record.started_at = started.parse().map_err(|_| StoreError::Unavailable)?;
            super::accounting::reconcile(tx, &owner, &record)?;
            let record = stored_usage(tx, record)?;
            group.push(
                tx,
                envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::UsageRecord(record.clone()),
                ),
            )?;
            group.push(
                tx,
                envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::BudgetState(squad_store::accounting::budget(
                        tx,
                        &record.day,
                        max,
                    )?),
                ),
            )?;
            Ok(group.accepted(id))
        },
    )
}

fn terminal(
    tx: &rusqlite::Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    work: &Work,
    failure: ErrorPayload,
) -> Result<()> {
    let seq = group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::WorkState(WorkStateEvent {
                work_id: work.work_id,
                trigger_event_id: work.trigger_event_id,
                target: work.target.clone(),
                context_id: work.context_id.clone(),
                attempt: work.attempt,
                status: WorkStatus::Failed,
                error: Some(failure.clone()),
                debate_id: work.debate_id,
                case_id: work.case_id,
                contest_id: work.contest_id,
            }),
        ),
    )?;
    squad_store::work::finish(tx, work.work_id, WorkStatus::Failed, seq)?;
    squad_store::accounting::release_unstarted_work(tx, work.work_id)?;
    tx.execute(
        "UPDATE work SET error_json=? WHERE work_id=?",
        params![serde_json::to_string(&failure)?, work.work_id.to_string()],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn failed(
    store: &Store,
    owner: ExecutorKey,
    work_id: WorkId,
    attempt: u32,
    fence: u64,
    epoch: u64,
    failure: ErrorPayload,
    repair: Option<JudgeRepairData>,
) -> Result<CommitOutcome> {
    let id = EventId::new();
    let digest = squad_store::commit::digest(&(work_id, attempt, fence, epoch, &failure))?;
    store.commit(id, serde_json::to_string(&owner)?, digest, move |tx, group| {
        let work = squad_store::work::check_fence(tx, work_id, &owner, attempt, fence, epoch, group.accepted_at)?;
        if failure.work_id != Some(work_id) || failure.case_id != work.case_id || failure.contest_id != work.contest_id || failure.debate_id != work.debate_id {
            return Err(super::authorization::forbidden());
        }
        let repair_used: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM judge_repairs WHERE work_id=?1 UNION ALL SELECT 1 FROM mode_repairs WHERE work_id=?1)",[work_id.to_string()],|r|r.get(0))?;
        let failure_class=if matches!(failure.code,ErrorCode::JUDGE_INVALID_VERDICT|ErrorCode::JUDGE_INVALID_MODE_OUTPUT|ErrorCode::MODE_INVALID_OUTPUT) { crate::orchestrator::types::FailureClass::InvalidOutput } else if failure.code==ErrorCode::PROVIDER_TIMEOUT { crate::orchestrator::types::FailureClass::Timeout } else { crate::orchestrator::types::FailureClass::Transport };
        // Judge transport/deadline failures are terminal; only invalid output
        // receives a repair attempt. The shared policy still chooses the outcome.
        let budget=if matches!(work.kind,WorkKind::Judge|WorkKind::JudgeRound|WorkKind::SurveyExtract|WorkKind::SurveyReport) && failure_class!=crate::orchestrator::types::FailureClass::InvalidOutput { attempt as u8 } else {crate::orchestrator::rounds::attempt_limit(&work.kind)};
        let decision=if work.kind==WorkKind::JudgeTournament && attempt<3 { crate::orchestrator::types::AttemptDecision::Retry{work_id,next_attempt:attempt+1,repair:failure_class==crate::orchestrator::types::FailureClass::InvalidOutput} } else { crate::orchestrator::rounds::failure_policy(work_id,&work.kind,attempt,budget,repair_used,failure_class) };
        if work.kind.new_mode_kind().is_some()
            && let crate::orchestrator::types::AttemptDecision::Retry {repair: needs_repair,..}=decision {
                if needs_repair {
                    let repair=repair.ok_or(StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION,"Invalid mode output requires repair diagnostics"))?;
                    let started:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM attempts WHERE work_id=? AND attempt=? AND epoch=?)",params![work_id.to_string(),attempt,epoch as i64],|r|r.get(0))?;
                    if !started || repair_used || repair.attempt!=attempt+1 {return Err(super::authorization::forbidden());}
                    squad_store::judge_repairs::save_mode(tx,&work,&repair)?;
                } else if repair.is_some() {return Err(super::authorization::forbidden());}
                tx.execute("UPDATE work SET status='queued',lease_fence=lease_fence+1,lease_owner=NULL,lease_until=NULL,execution_deadline=NULL WHERE work_id=?",[work_id.to_string()])?;
                return Ok(group.accepted(id));
        }
        if work.kind.census_composition().is_some() && attempt < if work.kind.census_judge(){3}else{2} && !(work.kind.census_judge() && repair_used && failure.code==ErrorCode::JUDGE_INVALID_MODE_OUTPUT) {
            if let Some(repair)=repair {
                if !work.kind.census_judge() || repair.attempt!=attempt+1 || failure.code!=ErrorCode::JUDGE_INVALID_MODE_OUTPUT {return Err(super::authorization::forbidden());}
                tx.execute("UPDATE work SET attempt=? WHERE work_id=?",params![repair.attempt,work_id.to_string()])?;
                squad_store::judge_repairs::save(tx,work_id,work.case_id.ok_or(StoreError::Unavailable)?,&repair)?;
                tx.execute("UPDATE work SET attempt=? WHERE work_id=?",params![attempt,work_id.to_string()])?;
            }
            tx.execute("UPDATE work SET status='queued',lease_fence=lease_fence+1,lease_owner=NULL,lease_until=NULL WHERE work_id=?",[work_id.to_string()])?;
            return Ok(group.accepted(id));
        }
        if let Some(mut repair) = repair {
            if work.kind==WorkKind::JudgeTournament {
                let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='tournament.identities'",[work.contest_id.ok_or(StoreError::Unavailable)?.to_string()],|r|r.get(0))?;
                let value:serde_json::Value=serde_json::from_str(&raw)?;let tokens:Vec<String>=serde_json::from_value(value["tokens"].clone())?;
                repair.rejected_output=crate::orchestrator::tournament::redact(&repair.rejected_output,&tokens);
            }

            if !matches!(decision, crate::orchestrator::types::AttemptDecision::Retry { repair: true, .. }) { return Err(super::authorization::forbidden()); }
            let limit = u32::from(if work.kind == WorkKind::JudgeRound { constants::ROUND_JUDGE_ATTEMPTS } else { constants::FINAL_JUDGE_ATTEMPTS });
            if !matches!(owner, ExecutorKey::Case(_)) || !matches!(work.kind, WorkKind::Judge | WorkKind::JudgeRound | WorkKind::JudgeTournament | WorkKind::CrosscheckGoal | WorkKind::CrosscheckNormalize | WorkKind::CrosscheckRule | WorkKind::SurveyExtract | WorkKind::SurveyReport | WorkKind::ProbeReport) || !matches!(failure.code,ErrorCode::JUDGE_INVALID_VERDICT|ErrorCode::JUDGE_INVALID_MODE_OUTPUT) || repair.attempt != attempt + 1 || attempt >= limit {
                return Err(super::authorization::forbidden());
            }
            let started:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM attempts WHERE work_id=? AND attempt=? AND epoch=?)",params![work_id.to_string(),attempt,epoch as i64],|r|r.get(0))?;
            if !started { return Err(StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION,"Repair requires a durable provider start")); }
            // Consume the old attempt and authorize the repair atomically. Its
            // rejected body lives exclusively in this private table.
            tx.execute("UPDATE work SET status='queued',attempt=?,lease_fence=lease_fence+1,lease_owner=NULL,lease_until=NULL,execution_deadline=CASE WHEN kind LIKE 'probe.%' OR kind LIKE 'grill.%' THEN NULL ELSE execution_deadline END WHERE work_id=?", params![repair.attempt, work_id.to_string()])?;
            squad_store::judge_repairs::save(tx, work_id, work.case_id.ok_or(StoreError::Unavailable)?, &repair)?;
            // claim() increments attempt, so retain the consumed attempt count.
            tx.execute("UPDATE work SET attempt=? WHERE work_id=?", params![attempt, work_id.to_string()])?;
            return Ok(group.accepted(id));
        }
        if work.contest_id.is_some() && matches!(decision, crate::orchestrator::types::AttemptDecision::Retry { repair: false, .. }) {
            tx.execute("UPDATE work SET status='queued',lease_fence=lease_fence+1,lease_owner=NULL,lease_until=NULL,execution_deadline=CASE WHEN kind LIKE 'probe.%' OR kind LIKE 'grill.%' THEN NULL ELSE execution_deadline END WHERE work_id=?",[work_id.to_string()])?;
            return Ok(group.accepted(id));
        }
        terminal(tx, group, id, &work, failure)?;
        super::picks::refresh(tx,group,id)?;
        Ok(group.accepted(id))
    })
}

pub fn start(
    store: &Store,
    owner: ExecutorKey,
    work_id: WorkId,
    attempt: u32,
    fence: u64,
    input_tokens: Option<u64>,
    max: i64,
) -> Result<CommitOutcome> {
    let id = EventId::new();
    store.commit(id, serde_json::to_string(&owner)?, squad_store::commit::digest(&(work_id, attempt, fence))?, move |tx, group| {
        let epoch: i64 = tx.query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))?;
        let mut record = squad_store::work::record(tx, work_id)?.ok_or(StoreError::Unavailable)?;
        let mut work = squad_store::work::check_fence(tx, work_id, &owner, record.attempt, fence, epoch as u64, group.accepted_at)?;
        if attempt != work.attempt {
            // Only ordinary transport retries can advance through this operation.
            let prior_ended: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM attempts WHERE work_id=? AND attempt=? AND ended_at IS NOT NULL AND outcome='failed')", params![work_id.to_string(), work.attempt], |r| r.get(0))?;
            if work.kind != WorkKind::Reply || attempt != work.attempt + 1 || attempt > u32::from(constants::REPLY_ATTEMPTS) || !prior_ended {
                return Err(StoreError::Domain(ErrorCode::STALE_WORK_ATTEMPT, "Unauthorized attempt"));
            }
            tx.execute("UPDATE work SET attempt=? WHERE work_id=?", params![attempt, work_id.to_string()])?;
            work.attempt = attempt;
        }
        if work.kind==WorkKind::JudgeTournament || (work.kind==WorkKind::Reply && work.round.is_some()) {
            let ms=if work.kind==WorkKind::JudgeTournament {constants::TOURNAMENT_JUDGE_DEADLINE_MS}else{constants::TOTAL_TIMEOUT_MS};
            let deadline=record.deadline.unwrap_or(group.accepted_at+chrono::Duration::milliseconds(ms as i64));
            tx.execute("UPDATE work SET execution_deadline=COALESCE(execution_deadline,?) WHERE work_id=?",params![deadline.to_rfc3339(),work_id.to_string()])?;record.deadline=Some(deadline);
        }
        let (_, binding) = squad_store::work::template(tx, work_id)?.ok_or(StoreError::Unavailable)?;
        let grant = ExecutionGrant { binding, work, owner, fence, epoch: epoch as u64, lease_until: record.lease_until.ok_or(StoreError::Unavailable)?, deadline: record.deadline };
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        match squad_store::accounting::start_priced(tx, &grant, group.accepted_at, &day, input_tokens, max) {
            Ok(()) => Ok(RequestResult::Internal(InternalResult::Started(grant))),
            Err(StoreError::Domain(code, message)) => {
                let text = message.to_owned();
                let mut failure = error(code, text);
                failure.work_id = Some(work_id);
                terminal(tx, group, id, &grant.work, failure.clone())?;
                Ok(RequestResult::Rejected { request_id: id, error: failure })
            },
            Err(e) => Err(e),
        }
    })
}

/// Publish the merged durable snapshot, so a result with omitted usage or an
/// older pending snapshot cannot regress an already reconciled usage event.
pub fn stored_usage(db: &rusqlite::Connection, mut record: UsageRecord) -> Result<UsageRecord> {
    let (usage,provider,response,outcome,ended): (Option<String>,Option<String>,Option<String>,String,Option<String>)=db.query_row(
        "SELECT usage_json,provider_request_id,response_model,outcome,ended_at FROM attempts WHERE work_id=? AND attempt=?",
        params![record.work_id.to_string(),record.attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    )?;
    record.usage = usage.map(|json| serde_json::from_str(&json)).transpose()?;
    record.provider_request_id = provider;
    record.response_model = response
        .map(Text::new)
        .transpose()
        .map_err(|_| StoreError::Unavailable)?;
    record.outcome = serde_json::from_value(serde_json::Value::String(outcome))?;
    record.ended_at = ended
        .map(|value| value.parse())
        .transpose()
        .map_err(|_| StoreError::Unavailable)?;
    Ok(record)
}

/// Frozen candidate execution is deliberately separate from every prompt adapter.
pub fn executor_call(
    grant: &ExecutionGrant,
    prompt: &str,
    input: &str,
    published_output_limit: u32,
) -> std::result::Result<
    squad_agents::openrouter::ChatRequest,
    squad_agents::openrouter::TransportError,
> {
    squad_agents::agents::bare_executor_request(grant, prompt, input, published_output_limit)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorCallFailure {
    Retry { next_attempt: u32 },
    Missing,
}
pub fn executor_call_failure(attempt: u32) -> ExecutorCallFailure {
    if attempt == 1 {
        ExecutorCallFailure::Retry { next_attempt: 2 }
    } else {
        ExecutorCallFailure::Missing
    }
}
