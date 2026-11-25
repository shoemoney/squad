//! M1 open-mode harness. M3 can compose this with real executor factories.
use super::Router;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitOutcome};
use std::collections::BTreeSet;

/// Durable startup state to expose before spawning executors. Epoch is restored;
/// ownership fences advance on every open. Only clear advances the room epoch.
#[derive(Debug)]
pub struct RecoveryReport {
    pub epoch: u64,
    pub boundaries: squad_store::events::Boundaries,
    pub interrupted: Vec<WorkId>,
    pub eligible: Vec<WorkRecord>,
    pub cases: Vec<CaseId>,
}

/// Call once after acquiring the room lock and resolving startup models, before
/// runtime construction. Runtime workers need not mutate recovery state.
pub fn recover_on_open(store: &squad_store::Store) -> Result<RecoveryReport> {
    recover_on_open_mode(store, OpenMode::Interactive, chrono::Utc::now())
}

/// Mode-aware startup for OneShot, with an injected clock for recovery tests.
pub fn recover_on_open_mode(
    store: &squad_store::Store,
    mode: OpenMode,
    now: DateTime,
) -> Result<RecoveryReport> {
    let report=store.call(move |db| {
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let interrupted=squad_store::leases::recover(&tx,true,now)?;
        if mode==OpenMode::OneShot {
            // Preserve the original flag across repeated OneShot invocations.
            tx.execute_batch("UPDATE work SET scheduling_meta=json_set(scheduling_meta,'$.openModeDormant',runnable),runnable=0 WHERE status IN ('queued','running') AND json_type(scheduling_meta,'$.openModeDormant') IS NULL;")?;
        } else {
            tx.execute_batch("UPDATE work SET runnable=json_extract(scheduling_meta,'$.openModeDormant'),scheduling_meta=json_remove(scheduling_meta,'$.openModeDormant') WHERE json_type(scheduling_meta,'$.openModeDormant') IS NOT NULL;")?;
        }
        let epoch:i64=tx.query_row("SELECT epoch FROM room_meta",[],|r|r.get(0))?;
        let boundaries=squad_store::events::boundaries(&tx)?;
        let eligible=if mode==OpenMode::Interactive {squad_store::leases::eligible_at(&tx,now)?}else{Vec::new()};
        let cases=eligible.iter().map(|r|Ok(squad_store::work::template(&tx,r.work_id)?.and_then(|(w,_)|w.case_id))).collect::<Result<Vec<_>>>()?.into_iter().flatten().collect::<BTreeSet<_>>().into_iter().collect();
        tx.commit()?;
        Ok(RecoveryReport {epoch:epoch as u64,boundaries,interrupted,eligible,cases})
    })?;
    tracing::info!(
        epoch = report.epoch,
        interrupted = report.interrupted.len(),
        eligible = report.eligible.len(),
        "Room recovery committed"
    );
    Ok(report)
}

/// Case-bound lifecycle seam. Implementations bind scoped readers and submission
/// capabilities; tests can record spawn/retire calls without a provider runtime.
pub trait CaseExecutors {
    fn spawn(&mut self, case: CaseId) -> Result<()>;
    fn retire(&mut self, case: CaseId);
}

/// Adapts the shared runtime hook without depending on M2's implementation.
pub struct ScopedCaseExecutors<H, F> {
    pub hook: H,
    pub context: F,
}
impl<H, F> CaseExecutors for ScopedCaseExecutors<H, F>
where
    H: crate::runtime::contexts::CaseExecutorHook,
    F: FnMut(CaseId) -> AgentContext,
{
    fn spawn(&mut self, case: CaseId) -> Result<()> {
        self.hook
            .spawn(case, (self.context)(case))
            .map_err(|_| StoreError::Unavailable)
    }
    fn retire(&mut self, case: CaseId) {
        self.hook.retire(case);
    }
}

pub struct RuntimeHarness<H: CaseExecutors> {
    router: Router,
    mode: OpenMode,
    invocation_work: BTreeSet<WorkId>,
    cases: BTreeSet<CaseId>,
    hooks: H,
}
impl<H: CaseExecutors> RuntimeHarness<H> {
    /// Acquire the room lock before SQLite or startup recovery can mutate state.
    pub fn open(
        data_dir: &std::path::Path,
        mode: OpenMode,
        hooks: H,
        configure: impl FnOnce(squad_store::Store) -> Result<Router>,
    ) -> std::result::Result<Self, squad_store::writer::RoomOpenError> {
        let store = squad_store::Store::open_room(data_dir)?;
        let router = configure(store)?;
        Ok(Self::new(router, mode, hooks)?)
    }
    /// The caller opens a locked Store before constructing the router. Recovery
    /// mutates ownership only; receipts, frozen inputs and repair data stay intact.
    pub fn new(router: Router, mode: OpenMode, hooks: H) -> Result<Self> {
        recover_on_open_mode(&router.store, mode.clone(), chrono::Utc::now())?;
        let mut harness = Self {
            router,
            mode,
            invocation_work: BTreeSet::new(),
            cases: BTreeSet::new(),
            hooks,
        };
        harness.scan()?;
        Ok(harness)
    }
    /// Returns durable work to wake. Calling periodically repairs dropped wakes
    /// and expired ownership; no notification is needed to rediscover the queue.
    pub fn scan(&mut self) -> Result<Vec<WorkRecord>> {
        let mode = self.mode.clone();
        let allowed = self.invocation_work.clone();
        let work = self.router.store.call(move |db| {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            // Startup already fenced all previous owners. Any running lease here
            // belongs to this invocation, including in OneShot mode.
            squad_store::leases::recover(&tx, false, chrono::Utc::now())?;
            let records = squad_store::leases::eligible(&tx)?;
            tx.commit()?;
            Ok(records
                .into_iter()
                .filter(|w| mode == OpenMode::Interactive || allowed.contains(&w.work_id))
                .collect::<Vec<_>>())
        })?;
        let cases = work
            .iter()
            .filter_map(|record| record.executable.as_ref().and_then(|w| w.case_id))
            .collect::<BTreeSet<_>>();
        // Queued first attempts have no executable boundary yet, but still need a
        // case executor to claim them. Obtain their immutable case binding.
        let ids = work.iter().map(|w| w.work_id).collect::<Vec<_>>();
        let mut cases = cases;
        cases.extend(
            self.router
                .store
                .call(move |db| {
                    ids.into_iter()
                        .map(|id| {
                            Ok(squad_store::work::template(db, id)?.and_then(|(w, _)| w.case_id))
                        })
                        .collect::<Result<Vec<_>>>()
                })?
                .into_iter()
                .flatten(),
        );
        let retired = self.cases.difference(&cases).copied().collect::<Vec<_>>();
        for case in retired {
            self.hooks.retire(case);
            self.cases.remove(&case);
        }
        let spawned = cases.difference(&self.cases).copied().collect::<Vec<_>>();
        for case in spawned {
            self.hooks.spawn(case)?;
            self.cases.insert(case);
        }
        Ok(work)
    }
    pub fn process(
        &mut self,
        request: Request,
        owner: Option<ExecutorKey>,
    ) -> Result<CommitOutcome> {
        if self.mode == OpenMode::OneShot && owner.is_some() {
            let allowed = match &request {
                Request::Internal(InternalRequest::Executor(executor)) => match executor {
                    ExecutorRequest::Claim { work_id }
                    | ExecutorRequest::StartAttempt { work_id, .. }
                    | ExecutorRequest::RenewLease { work_id, .. }
                    | ExecutorRequest::Result { work_id, .. }
                    | ExecutorRequest::Failed { work_id, .. } => {
                        self.invocation_work.contains(work_id)
                    }
                    ExecutorRequest::Account { record, .. } => {
                        self.invocation_work.contains(&record.work_id)
                    }
                    ExecutorRequest::Delta { delta, .. } => {
                        self.invocation_work.contains(&delta.generation.work_id)
                    }
                    ExecutorRequest::Presence(_) => false,
                },
                _ => false,
            };
            if !allowed {
                return Err(StoreError::Domain(
                    ErrorCode::PRODUCER_FORBIDDEN,
                    "Work is dormant in this invocation",
                ));
            }
        }
        if self.mode == OpenMode::OneShot
            && let (
                Request::Internal(InternalRequest::Executor(ExecutorRequest::Claim { work_id })),
                Some(key),
            ) = (&request, &owner)
        {
            let id = *work_id;
            let key = key.clone();
            let scope = self.invocation_work.iter().copied().collect::<Vec<_>>();
            let result = self.router.store.call(move |db| {
                let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                let grant =
                    squad_store::leases::claim(&tx, id, &key, chrono::Utc::now(), Some(&scope))?;
                tx.commit()?;
                Ok(RequestResult::Internal(
                    grant.map_or(InternalResult::NoWork, InternalResult::Claimed),
                ))
            })?;
            return Ok(CommitOutcome {
                result,
                events: Vec::new(),
                duplicate: false,
            });
        }
        let before = self.router.store.call(|db| {
            Ok(
                db.query_row("SELECT COALESCE(MAX(rowid),0) FROM work", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        })?;
        let outcome = self.router.process(request, owner)?;
        let new = self.router.store.call(move |db| {
            let mut query = db.prepare("SELECT work_id FROM work WHERE rowid>?")?;
            let ids = query
                .query_map([before], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ids.into_iter()
                .map(|id| id.parse().map_err(|_| StoreError::Unavailable))
                .collect::<Result<Vec<WorkId>>>()
        })?;
        self.invocation_work.extend(new);
        if self.scan().is_err() {
            tracing::warn!(
                "Durable request committed; executor recovery will retry on the next scan"
            );
        }
        Ok(outcome)
    }
    /// Commit-only CLI mode deliberately creates no executable invocation set.
    /// Its accepted work remains dormant until the next interactive recovery.
    pub fn commit_only(&mut self, request: Request) -> Result<CommitOutcome> {
        if self.mode != OpenMode::OneShot {
            return Err(StoreError::Domain(
                ErrorCode::INVALID_COMMAND,
                "Commit-only submission requires OneShot mode",
            ));
        }
        self.router.process(request, None)
    }
    pub fn hooks(&self) -> &H {
        &self.hooks
    }
}
impl<H: CaseExecutors> Drop for RuntimeHarness<H> {
    fn drop(&mut self) {
        for case in &self.cases {
            self.hooks.retire(*case);
        }
    }
}
