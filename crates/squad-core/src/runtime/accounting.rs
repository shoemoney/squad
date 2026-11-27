//! Retry delayed provider receipts independently of work cancellation.
use crate::bus::InProcessBus;
use squad_agents::{accounting::AttemptAccounting, openrouter::OpenRouterClient};
use squad_protocol::*;
use squad_store::Store;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn unavailable() -> ReaderError {
    ReaderError::StoreUnavailable {
        message: "Billing reconciliation unavailable".into(),
    }
}

fn pending(store: &Store) -> Result<Vec<(u64, UsageRecord)>, ReaderError> {
    // Startup identity probes have a separate persistence owner and may retain
    // an unresolved alias rather than a concrete executable-work identity.
    store
        .call(|db| {
            let mut query = db.prepare(
                "SELECT epoch, json_object(
                    'workId',work_id,'attempt',attempt,'target',target,
                    'contextId',context_id,'providerRequestId',provider_request_id,
                    'model',model,'responseModel',response_model,'day',day,
                    'startedAt',started_at,'endedAt',ended_at,'outcome',outcome,
                    'usage',json(usage_json),'estimatedCostUsd',estimated_cost_usd,
                    'debateId',debate_id,'caseId',case_id,'contestId',contest_id)
                 FROM attempts a
                 WHERE unresolved=1 AND provider_request_id IS NOT NULL
                   AND a.context_id <> ('probe:' || a.target)
                   AND (ended_at IS NOT NULL OR NOT EXISTS (
                       SELECT 1 FROM work w WHERE w.work_id=a.work_id
                       AND w.attempt=a.attempt AND w.epoch=a.epoch
                       AND w.status='running'))
                 ORDER BY a.rowid",
            )?;
            let rows = query
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(epoch, json)| Ok((epoch as u64, serde_json::from_str(&json)?)))
                .collect()
        })
        .map_err(|_| unavailable())
}

async fn reconcile_pending(
    store: &Store,
    bus: &InProcessBus,
    client: &OpenRouterClient,
    cancel: &CancellationToken,
) -> Result<(), ReaderError> {
    for (epoch, record) in pending(store)? {
        let owner = match &record.target {
            Target::Member(nick) => ExecutorKey::Member(nick.clone()),
            Target::Judge => ExecutorKey::Case(record.case_id.ok_or_else(unavailable)?),
        };
        // Accounting authenticates the original attempt/epoch, independently of
        // the current work lease. No conversational work is reopened.
        let mut account = AttemptAccounting::from_record(record, 0, epoch)?;
        let resolved = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(()),
            result = tokio::time::timeout(Duration::from_secs(10), account.reconcile(client)) => {
                matches!(result, Ok(Ok(())))
            }
        };
        if resolved
            && account
                .record
                .usage
                .as_ref()
                .is_some_and(|u| u.cost_usd.is_some())
        {
            account.submit(bus.executor_submit(owner).as_ref()).await?;
        }
    }
    Ok(())
}

pub(super) fn spawn(
    store: Store,
    bus: InProcessBus,
    client: OpenRouterClient,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Result<(), ReaderError>> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                _ = interval.tick() => {}
            }
            reconcile_pending(&store, &bus, &client, &cancel).await?;
        }
    })
}

/// Workers first persist their final cancellation snapshots; then try available
/// receipts once more. Unknown charges remain durable for the next room open.
pub(super) async fn finish(store: &Store, bus: &InProcessBus, client: &OpenRouterClient) {
    let cancel = CancellationToken::new();
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        reconcile_pending(store, bus, client, &cancel),
    )
    .await;
}
