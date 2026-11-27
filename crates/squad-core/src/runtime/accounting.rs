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
            Ok(rows
                .into_iter()
                .filter_map(|(epoch, json)| match serde_json::from_str(&json) {
                    Ok(record) => Some((epoch as u64, record)),
                    Err(e) => {
                        tracing::warn!(error=?e, "Billing reconciliation skipped an unreadable attempt row");
                        None
                    }
                })
                .collect())
        })
        .map_err(|_| unavailable())
}

const RETRY_BASE: Duration = Duration::from_secs(5);
const RETRY_CAP: Duration = Duration::from_secs(300);
const RETRY_LIMIT: u32 = 12;

struct Retry {
    tries: u32,
    next: tokio::time::Instant,
}

/// Per-attempt exponential backoff kept in memory. After `RETRY_LIMIT` failures an
/// attempt is left alone for the rest of this process; its conservative hold stays
/// durable and the next room open tries it again.
#[derive(Default)]
pub(super) struct Retries(std::collections::HashMap<(String, u32), Retry>);

impl Retries {
    fn due(&self, key: &(String, u32)) -> bool {
        self.0
            .get(key)
            .is_none_or(|r| r.tries < RETRY_LIMIT && tokio::time::Instant::now() >= r.next)
    }
    fn failed(&mut self, key: (String, u32)) {
        let entry = self.0.entry(key).or_insert(Retry {
            tries: 0,
            next: tokio::time::Instant::now(),
        });
        entry.tries += 1;
        let delay = RETRY_BASE
            .saturating_mul(1u32 << entry.tries.min(16))
            .min(RETRY_CAP);
        entry.next = tokio::time::Instant::now() + delay;
    }
}

/// Settle one attempt. Returns whether the receipt was resolved and submitted.
async fn reconcile_row(
    bus: &InProcessBus,
    client: &OpenRouterClient,
    cancel: &CancellationToken,
    epoch: u64,
    record: UsageRecord,
) -> Result<bool, ReaderError> {
    let owner = match &record.target {
        Target::Member(nick) => ExecutorKey::Member(nick.clone()),
        Target::Judge => ExecutorKey::Case(record.case_id.ok_or_else(unavailable)?),
    };
    // Accounting authenticates the original attempt/epoch, independently of
    // the current work lease. No conversational work is reopened.
    let mut account = AttemptAccounting::from_record(record, 0, epoch)?;
    let resolved = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Ok(false),
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
        return Ok(true);
    }
    Ok(false)
}

/// A bad row or a transient store error never ends the room: it is logged,
/// skipped and retried later with backoff. Only cancellation stops the loop.
pub(super) async fn reconcile_pending(
    store: &Store,
    bus: &InProcessBus,
    client: &OpenRouterClient,
    cancel: &CancellationToken,
    retries: &mut Retries,
) {
    let rows = match pending(store) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(error=?e, "Billing reconciliation could not read pending attempts");
            return;
        }
    };
    for (epoch, record) in rows {
        if cancel.is_cancelled() {
            return;
        }
        let key = (record.work_id.to_string(), record.attempt);
        if !retries.due(&key) {
            continue;
        }
        match reconcile_row(bus, client, cancel, epoch, record).await {
            Ok(true) => {
                retries.0.remove(&key);
            }
            Ok(false) => {
                if !cancel.is_cancelled() {
                    retries.failed(key);
                }
            }
            Err(e) => {
                tracing::warn!(work_id=%key.0, attempt=key.1, error=?e, "Billing reconciliation skipped an attempt");
                retries.failed(key);
            }
        }
    }
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
        let mut retries = Retries::default();
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                _ = interval.tick() => {}
            }
            reconcile_pending(&store, &bus, &client, &cancel, &mut retries).await;
        }
    })
}

/// Workers first persist their final cancellation snapshots; then try available
/// receipts once more. Unknown charges remain durable for the next room open.
pub(super) async fn finish(store: &Store, bus: &InProcessBus, client: &OpenRouterClient) {
    let cancel = CancellationToken::new();
    let _ = tokio::time::timeout(
        Duration::from_secs(2),
        reconcile_pending(store, bus, client, &cancel, &mut Retries::default()),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    fn bus_and_store(dir: &std::path::Path) -> (Store, InProcessBus) {
        let store = Store::open(dir.join("room.sqlite")).unwrap();
        let binding = ExecutorBinding {
            model: Text::new("fixture/model").unwrap(),
            effort: ReasoningEffort::Medium,
        };
        let bindings = BTreeMap::from([
            ("member0".to_string(), binding.clone()),
            ("judge".to_string(), binding),
        ]);
        let bus = InProcessBus::start(
            crate::router::Router::new(
                store.clone(),
                vec![MemberNick::new("member0").unwrap()],
                bindings,
                20_000_000,
            )
            .unwrap(),
        );
        (store, bus)
    }

    fn insert_unresolved(store: &Store, started: &str, ended: &str) {
        let (s, e) = (started.to_owned(), ended.to_owned());
        store
            .call(move |db| {
                db.execute(
                    "INSERT INTO attempts(work_id,attempt,epoch,target,context_id,provider_request_id,model,day,started_at,ended_at,outcome,unresolved)
                     VALUES (?1,1,(SELECT epoch FROM room_meta),'member0','ctx-1','gen-1','fixture/model','2026-11-27',?2,?3,'unknown',1)",
                    rusqlite::params![WorkId::new().to_string(), s, e],
                )?;
                Ok(())
            })
            .unwrap();
    }

    #[tokio::test]
    async fn poisoned_row_never_ends_the_reconciler_task() {
        let dir = tempfile::tempdir().unwrap();
        let (store, bus) = bus_and_store(dir.path());
        // ended_at precedes started_at: rebuilding the record fails validation.
        insert_unresolved(
            &store,
            "2026-11-27T10:00:10+00:00",
            "2026-11-27T10:00:00+00:00",
        );
        let client = OpenRouterClient::new("http://127.0.0.1:9/api/v1/", "k").unwrap();
        let cancel = CancellationToken::new();
        let handle = spawn(store, bus, client, cancel.clone());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!handle.is_finished(), "a bad row must not finish the task");
        cancel.cancel();
        assert!(handle.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn unresolved_rows_back_off_instead_of_polling_every_tick() {
        let dir = tempfile::tempdir().unwrap();
        let (store, bus) = bus_and_store(dir.path());
        insert_unresolved(
            &store,
            "2026-11-27T10:00:00+00:00",
            "2026-11-27T10:00:10+00:00",
        );
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = OpenRouterClient::new(&format!("{}/api/v1/", server.uri()), "k").unwrap();
        let cancel = CancellationToken::new();
        let mut retries = Retries::default();
        for _ in 0..3 {
            reconcile_pending(&store, &bus, &client, &cancel, &mut retries).await;
        }
        let seen = server.received_requests().await.unwrap().len();
        assert!(seen >= 1, "the first pass must try the provider");
        // The two later immediate passes were backed off, so they cost nothing more.
        reconcile_pending(&store, &bus, &client, &cancel, &mut retries).await;
        assert_eq!(server.received_requests().await.unwrap().len(), seen);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_grow_and_stop_after_the_limit() {
        let key = ("w".to_string(), 1);
        let mut retries = Retries::default();
        assert!(retries.due(&key));
        retries.failed(key.clone());
        assert!(!retries.due(&key));
        tokio::time::advance(Duration::from_secs(11)).await;
        assert!(retries.due(&key), "first backoff is 10 s");
        for _ in 0..RETRY_LIMIT {
            retries.failed(key.clone());
            tokio::time::advance(RETRY_CAP * 2).await;
        }
        assert!(!retries.due(&key), "gives up after the limit");
    }
}
