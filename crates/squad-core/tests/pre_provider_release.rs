//! An executor that stops before its provider call must not leave a hold behind.
mod support;
use squad_agents::worker::{self, ExecutionFuture, WorkExecutor};
use squad_protocol::*;
use std::{sync::Arc, time::Duration};
use support::*;
use tokio_util::sync::CancellationToken;

struct NoRender;
impl PromptRenderer for NoRender {
    fn render(
        &self,
        _: &PromptRef,
        _: &std::collections::BTreeMap<String, String>,
    ) -> Result<String, ReaderError> {
        Ok(String::new())
    }
}
/// Fails after `StartAttempt` and before any provider accounting, like a new-mode
/// runtime whose frozen-JSON decode or render step errors.
struct FailsBeforeProvider(ReaderError);
impl WorkExecutor for FailsBeforeProvider {
    fn execute<'a>(
        &'a self,
        _: &'a AgentContext,
        _: ExecutionGrant,
        _: CancellationToken,
    ) -> ExecutionFuture<'a> {
        let error = self.0.clone();
        Box::pin(async move { Err(error) })
    }
}

fn holds(f: &Fixture) -> (i64, u32, String) {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    f.store
        .call(move |db| {
            let d = squad_store::accounting::daily(db, &day)?;
            let status: String =
                db.query_row("SELECT status FROM work ORDER BY rowid LIMIT 1", [], |r| {
                    r.get(0)
                })?;
            Ok((d.reserved_micros, d.unknown_calls, status))
        })
        .unwrap()
}

#[tokio::test]
async fn executor_error_before_the_provider_releases_the_hold_and_fails_the_work() {
    let f = Fixture::new(2, 20_000_000).await;
    f.send("@member0 hello").await;
    let cancel = CancellationToken::new();
    let context = squad_core::runtime::contexts::agent_context(
        f.store.clone(),
        &f.bus,
        ExecutorKey::Member(nick("member0")),
        1,
        Arc::new(NoRender),
        cancel.clone(),
    );
    let handle = worker::spawn(
        context,
        Arc::new(FailsBeforeProvider(ReaderError::NotRunnable)),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (reserved, unknown, status) = holds(&f);
            if status == "failed" && reserved == 0 && unknown == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("hold leaked: {:?}", holds(&f)));
    let settled: (String, bool) = f
        .store
        .call(|db| {
            Ok(
                db.query_row("SELECT outcome, unresolved FROM attempts", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?,
            )
        })
        .unwrap();
    assert_eq!(settled, ("cancelled".to_string(), false));
    cancel.cancel();
    handle.await.unwrap().unwrap();
}

#[tokio::test]
async fn store_outage_inside_an_executor_still_stops_the_worker() {
    let f = Fixture::new(2, 20_000_000).await;
    f.send("@member0 hello").await;
    let context = squad_core::runtime::contexts::agent_context(
        f.store.clone(),
        &f.bus,
        ExecutorKey::Member(nick("member0")),
        1,
        Arc::new(NoRender),
        CancellationToken::new(),
    );
    let outage = ReaderError::StoreUnavailable {
        message: "down".into(),
    };
    let handle = worker::spawn(context, Arc::new(FailsBeforeProvider(outage)));
    let result = tokio::time::timeout(Duration::from_secs(10), handle)
        .await
        .expect("worker exits")
        .unwrap();
    assert!(matches!(result, Err(ReaderError::StoreUnavailable { .. })));
}
