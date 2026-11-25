//! Request-scoped dispatch for one-shot opens. Dormant work remains untouched.
use crate::Result;
use rusqlite::Connection;
use squad_protocol::*;
#[derive(Clone, Default)]
pub struct DispatchScope {
    pub work_ids: Vec<WorkId>,
}
pub fn claim_scoped(
    db: &Connection,
    id: WorkId,
    key: &ExecutorKey,
    now: DateTime,
    scope: &DispatchScope,
) -> Result<Option<ExecutionGrant>> {
    crate::leases::claim_with_scope(db, id, key, now, Some(&scope.work_ids))
}
