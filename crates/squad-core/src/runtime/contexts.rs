//! Capability construction belongs to core; agent code cannot choose or replace its readers.
use crate::bus::InProcessBus;
use squad_protocol::*;
use squad_store::{Store, readers::ScopedReaders};
use std::sync::Arc;
pub fn agent_context(
    store: Store,
    bus: &InProcessBus,
    executor: ExecutorKey,
    epoch: u64,
    render: Arc<dyn PromptRenderer>,
    cancel: tokio_util::sync::CancellationToken,
) -> AgentContext {
    let readers = Arc::new(ScopedReaders::new(store, executor.clone(), epoch));
    AgentContext {
        mailbox: bus.register(executor.clone()),
        history: readers.clone(),
        work: readers,
        submit: bus.executor_submit(executor.clone()),
        executor,
        render,
        cancel,
    }
}
/// M2 provides the executor implementation, while recovery can use a deterministic double.
pub trait CaseExecutorHook: Send + Sync {
    fn spawn(&self, case: CaseId, context: AgentContext) -> Result<(), ReaderError>;
    fn retire(&self, case: CaseId);
}

/// One-shot capability filter: a committed request runs only after explicit wake.
#[derive(Clone)]
pub(super) struct Scope {
    pub ids: Arc<std::sync::Mutex<Vec<WorkId>>>,
}
pub(super) fn scoped(mut context: AgentContext, store: Store, scope: Scope) -> AgentContext {
    context.work = Arc::new(ScopedWork {
        inner: context.work,
        scope: scope.clone(),
    });
    context.submit = Arc::new(ScopedSubmit {
        inner: context.submit,
        store,
        scope,
        owner: context.executor.clone(),
    });
    context
}
struct ScopedWork {
    inner: Arc<dyn WorkReader>,
    scope: Scope,
}
impl WorkReader for ScopedWork {
    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        Box::pin(async {
            let mut work = self.inner.unfinished().await?;
            let ids = self.scope.ids.lock().expect("dispatch scope");
            work.retain(|w| ids.contains(&w.work_id));
            Ok(work)
        })
    }
    fn get(&self, id: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        self.inner.get(id)
    }
    fn prompt_values_for_work(
        &self,
        id: WorkId,
    ) -> ReaderFuture<'_, std::collections::BTreeMap<String, String>> {
        self.inner.prompt_values_for_work(id)
    }
    fn repair_for_attempt(
        &self,
        id: WorkId,
        attempt: u32,
    ) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        self.inner.repair_for_attempt(id, attempt)
    }
}
struct ScopedSubmit {
    inner: Arc<dyn ExecutorSubmit>,
    store: Store,
    scope: Scope,
    owner: ExecutorKey,
}
impl ExecutorSubmit for ScopedSubmit {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_> {
        Box::pin(async move {
            if let ExecutorRequest::Claim { work_id } = request {
                let scope = squad_store::dispatch::DispatchScope {
                    work_ids: self.scope.ids.lock().expect("dispatch scope").clone(),
                };
                let owner = self.owner.clone();
                return match self.store.call(move |db| {
                    let tx =
                        db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                    let grant = squad_store::dispatch::claim_scoped(
                        &tx,
                        work_id,
                        &owner,
                        chrono::Utc::now(),
                        &scope,
                    )?;
                    tx.commit()?;
                    Ok(grant)
                }) {
                    Ok(grant) => RequestResult::Internal(
                        grant.map_or(InternalResult::NoWork, InternalResult::Claimed),
                    ),
                    Err(e) => crate::router::rejected(EventId::new(), e),
                };
            }
            self.inner.submit(request).await
        })
    }
}
