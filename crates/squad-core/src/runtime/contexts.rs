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
