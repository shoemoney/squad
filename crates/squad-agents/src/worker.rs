//! Private serial workers. Durable work is authoritative; mailbox items are hints.
use crate::accounting::acknowledged;
use squad_protocol::*;
use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub type ExecutionFuture<'a> = Pin<Box<dyn Future<Output = Result<(), ReaderError>> + Send + 'a>>;
pub trait WorkExecutor: Send + Sync {
    /// The supplied grant has durably started its first provider-call attempt.
    fn execute<'a>(
        &'a self,
        context: &'a AgentContext,
        grant: ExecutionGrant,
        cancel: CancellationToken,
    ) -> ExecutionFuture<'a>;
}
pub fn spawn(
    context: AgentContext,
    executor: Arc<dyn WorkExecutor>,
) -> tokio::task::JoinHandle<Result<(), ReaderError>> {
    tokio::spawn(run(context, executor))
}
async fn presence(
    context: &AgentContext,
    status: PresenceStatus,
    work_id: Option<WorkId>,
    depth: usize,
) -> Result<(), ReaderError> {
    if let ExecutorKey::Member(nick) = &context.executor {
        acknowledged(
            context
                .submit
                .submit(ExecutorRequest::Presence(Presence {
                    nick: nick.clone(),
                    status: status.clone(),
                    work_id,
                    queue_depth: depth as u32,
                    error_code: (status == PresenceStatus::Error)
                        .then_some(ErrorCode::PROVIDER_ERROR),
                    observed_at: chrono::Utc::now(),
                }))
                .await,
        )?;
    }
    Ok(())
}
pub async fn start_attempt(
    context: &AgentContext,
    grant: &ExecutionGrant,
    attempt: u32,
) -> Result<ExecutionGrant, ReaderError> {
    match context
        .submit
        .submit(ExecutorRequest::StartAttempt {
            work_id: grant.work.work_id,
            attempt,
            fence: grant.fence,
        })
        .await
    {
        RequestResult::Internal(InternalResult::Started(started))
            if started.owner == context.executor
                && started.work.work_id == grant.work.work_id
                && started.work.attempt == attempt
                && started.epoch == grant.epoch
                && started.fence == grant.fence
                && started.binding == grant.binding =>
        {
            Ok(started)
        }
        _ => Err(ReaderError::StaleWork),
    }
}
struct PresenceObserver {
    inner: Arc<dyn ExecutorSubmit>,
    status: Arc<std::sync::Mutex<PresenceStatus>>,
}
impl ExecutorSubmit for PresenceObserver {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_> {
        if let ExecutorRequest::Presence(presence) = &request {
            *self.status.lock().expect("presence lock") = presence.status.clone();
        }
        self.inner.submit(request)
    }
}
pub async fn run(
    mut context: AgentContext,
    executor: Arc<dyn WorkExecutor>,
) -> Result<(), ReaderError> {
    let status = Arc::new(std::sync::Mutex::new(PresenceStatus::Idle));
    context.submit = Arc::new(PresenceObserver {
        inner: context.submit.clone(),
        status: status.clone(),
    });
    let mut completed = BTreeSet::new();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if context.cancel.is_cancelled() {
            presence(&context, PresenceStatus::Offline, None, 0).await?;
            return Ok(());
        }
        let mut records = context.work.unfinished().await?;
        completed.retain(|(id, attempt)| {
            records
                .iter()
                .any(|r| r.work_id == *id && r.attempt == *attempt)
        });
        records.retain(|r| {
            !completed.contains(&(r.work_id, r.attempt))
                && matches!(r.status, WorkStatus::Queued | WorkStatus::Running)
        });
        // WorkReader supplies durable FIFO order. Do not reorder same-millisecond
        // UUIDs, whose random tail is not an acceptance sequence.
        if matches!(context.executor, ExecutorKey::Case(_)) && records.is_empty() {
            return Ok(());
        }
        let depth = records.len().min(constants::AGENT_QUEUE_DEPTH);
        // Held ordinary jobs must not block eligible contest work. Admission and
        // hold release remain router decisions represented by `runnable`.
        let next = records
            .iter()
            .find(|r| r.runnable && r.kind != WorkKind::Reply)
            .or_else(|| {
                records
                    .iter()
                    .take(constants::AGENT_QUEUE_DEPTH)
                    .find(|r| r.runnable)
            });
        if let Some(record) = next {
            let id = record.work_id;
            let attempt = record.attempt;
            let grant = match context
                .submit
                .submit(ExecutorRequest::Claim { work_id: id })
                .await
            {
                RequestResult::Internal(InternalResult::Claimed(grant))
                    if grant.owner == context.executor
                        && context.executor.owns(&grant.work)
                        && grant.work.work_id == id
                        && grant.work.validate().is_ok() =>
                {
                    grant
                }
                _ => {
                    tokio::select! { _ = context.cancel.cancelled() => {}, _ = heartbeat.tick() => {} }
                    continue;
                }
            };
            let grant = match start_attempt(&context, &grant, grant.work.attempt).await {
                Ok(grant) => grant,
                Err(_) => {
                    tokio::select! { _ = context.cancel.cancelled() => {}, _ = heartbeat.tick() => {} }
                    continue;
                }
            };
            let active_depth = records
                .len()
                .saturating_sub(1)
                .min(constants::AGENT_QUEUE_DEPTH);
            presence(&context, PresenceStatus::Thinking, Some(id), active_depth).await?;
            let cancel = context.cancel.child_token();
            let future = executor.execute(&context, grant.clone(), cancel.clone());
            tokio::pin!(future);
            let mut renewal =
                tokio::time::interval(Duration::from_millis(constants::WORK_LEASE_RENEWAL_MS));
            renewal.tick().await;
            let mut stopping = false;
            let result = loop {
                tokio::select! {
                    biased;
                    _ = context.cancel.cancelled(), if !stopping => { stopping = true; cancel.cancel(); },
                    result = &mut future => break result,
                    _ = heartbeat.tick() => {
                        match context.work.get(id).await {
                            Ok(Some(current)) if matches!(current.status, WorkStatus::Running) => {},
                            _ => cancel.cancel(),
                        }
                        let observed_status = status.lock().expect("presence lock").clone();
                        if presence(&context, observed_status, Some(id), active_depth).await.is_err() { cancel.cancel(); }
                    },
                    _ = renewal.tick() => {
                        if !matches!(context.submit.submit(ExecutorRequest::RenewLease { work_id: id, fence: grant.fence }).await,
                            RequestResult::Internal(InternalResult::LeaseRenewed { work_id, fence, .. }) if work_id == id && fence == grant.fence) { cancel.cancel(); }
                    }
                }
            };
            completed.insert((id, attempt));
            presence(
                &context,
                if result.is_ok() {
                    PresenceStatus::Idle
                } else {
                    PresenceStatus::Error
                },
                None,
                active_depth,
            )
            .await?;
            if let Err(error @ ReaderError::StoreUnavailable { .. }) = result {
                return Err(error);
            }
            continue;
        }
        presence(&context, PresenceStatus::Idle, None, depth).await?;
        tokio::select! {
            biased;
            _ = context.cancel.cancelled() => {},
            wakeup = context.mailbox.recv() => if wakeup.is_none() { return Ok(()); },
            _ = heartbeat.tick() => {},
        }
    }
}
