//! Executor capabilities bind ownership and the room epoch, never a caller-selected context.
use crate::{Store, StoreError};
use squad_protocol::*;
#[derive(Clone)]
pub struct ScopedReaders {
    store: Store,
    owner: ExecutorKey,
    epoch: u64,
}
impl ScopedReaders {
    pub fn new(store: Store, owner: ExecutorKey, epoch: u64) -> Self {
        Self {
            store,
            owner,
            epoch,
        }
    }
    fn read<T: Send + 'static>(
        &self,
        job: impl FnOnce(&rusqlite::Connection, &ExecutorKey) -> std::result::Result<T, ReaderError>
        + Send
        + 'static,
    ) -> std::result::Result<T, ReaderError> {
        let owner = self.owner.clone();
        let epoch = self.epoch;
        self.store
            .call(move |db| {
                let current: i64 = db.query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))?;
                if epoch as i64 != current {
                    return Ok(Err(ReaderError::StaleWork));
                }
                Ok(job(db, &owner))
            })
            .map_err(map_error)?
    }
}
fn map_error(_: StoreError) -> ReaderError {
    ReaderError::StoreUnavailable {
        message: "Store unavailable".into(),
    }
}
fn owned(
    db: &rusqlite::Connection,
    owner: &ExecutorKey,
    id: WorkId,
) -> std::result::Result<WorkRecord, ReaderError> {
    let (work, _) = crate::work::template(db, id)
        .map_err(map_error)?
        .ok_or(ReaderError::NotFound)?;
    if !owner.owns(&work) {
        return Err(ReaderError::Forbidden);
    }
    if let Some(step) = work.step_id {
        let assignment = db
            .query_row(
                "SELECT assignment_json FROM contest_assignments WHERE step_id=? AND target=?",
                rusqlite::params![step.to_string(), work.target.to_string()],
                |r| r.get::<_, String>(0),
            )
            .map_err(|_| ReaderError::Forbidden)?;
        let assignment = serde_json::from_str(&assignment).map_err(|_| ReaderError::Forbidden)?;
        work.validate_assignment(owner, &assignment)
            .map_err(|_| ReaderError::Forbidden)?;
    }
    crate::work::record(db, id)
        .map_err(map_error)?
        .ok_or(ReaderError::NotFound)
}
impl HistoryReader for ScopedReaders {
    fn for_work(&self, id: WorkId) -> ReaderFuture<'_, Vec<HistoryEntry>> {
        Box::pin(async move {
            self.read(move |db, owner| {
                let record = owned(db, owner, id)?;
                if record.status != WorkStatus::Running || !record.runnable {
                    return Err(ReaderError::NotRunnable);
                }
                let (epoch, lease_owner, lease_until): (i64, String, String) = db
                    .query_row(
                        "SELECT epoch,lease_owner,lease_until FROM work WHERE work_id=?",
                        [id.to_string()],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .map_err(|_| ReaderError::StaleWork)?;
                let current: i64 = db
                    .query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))
                    .map_err(|_| ReaderError::StaleWork)?;
                if epoch != current
                    || lease_owner
                        != serde_json::to_string(owner).map_err(|_| ReaderError::Forbidden)?
                    || lease_until <= chrono::Utc::now().to_rfc3339()
                {
                    return Err(ReaderError::StaleWork);
                }
                let work = record.executable.ok_or(ReaderError::NotRunnable)?;
                crate::history::read(db, &work.context_id, work.history_through).map_err(map_error)
            })
        })
    }
}
impl WorkReader for ScopedReaders {
    fn prompt_values_for_work(
        &self,
        id: WorkId,
    ) -> ReaderFuture<'_, std::collections::BTreeMap<String, String>> {
        Box::pin(async move {
            self.read(move |db, owner| {
            let record = owned(db, owner, id)?;
            if record.status != WorkStatus::Running || !record.runnable { return Err(ReaderError::NotRunnable); }
            let work = record.executable.ok_or(ReaderError::NotRunnable)?;
            let mut values = std::collections::BTreeMap::new();
            if matches!(work.kind, WorkKind::CrosscheckMember | WorkKind::CrosscheckJudge) {
                let material: String = db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",rusqlite::params![work.contest_id.ok_or(ReaderError::Forbidden)?.to_string(),format!("crosscheck.input:{}",id)],|r|r.get(0)).map_err(|_|ReaderError::Forbidden)?;
                values.insert("crosscheck_material_json".into(),material);
                return Ok(values);
            }
            if let Some(case) = work.case_id {
                if !matches!(owner, ExecutorKey::Case(owned) if *owned == case) { return Err(ReaderError::Forbidden); }
                let (material, state): (String, String) = db.query_row("SELECT material_json,state_json FROM cases WHERE case_id=? AND epoch=(SELECT epoch FROM room_meta)", [case.to_string()], |r| Ok((r.get(0)?,r.get(1)?))).map_err(|_|ReaderError::NotFound)?;
                values.insert("case_material_json".into(), material);
                values.insert("case_state_json".into(), state);
            }
            if let Some(debate) = work.debate_id {
                let state = crate::contests::debate(db, debate).map_err(map_error)?;
                if let Target::Member(nick) = &work.target {
                    let opponent = state.debaters.iter().find(|n| *n != nick).ok_or(ReaderError::Forbidden)?;
                    if !state.debaters.contains(nick) { return Err(ReaderError::Forbidden); }
                    values.insert("opponent".into(), opponent.to_string());
                }
                values.insert("debate_id".into(), debate.to_string());
                let (_, binding)=crate::work::template(db,id).map_err(map_error)?.ok_or(ReaderError::NotFound)?;
                values.insert("model_slug".into(),binding.model.to_string());
                if let Target::Member(nick) = &work.target {
                    values.insert("nick".into(), nick.to_string());
                    values.insert("round_data_json".into(), serde_json::json!({"debate_id":debate,"round":work.round,"rounds":state.total_rounds,"self":nick,"opponent":values.get("opponent"),"closing_required":work.round.is_some_and(|r|r>0 && r==state.total_rounds)}).to_string());
                }
                values.insert("rounds".into(), state.total_rounds.to_string());
                if let Some(round) = work.round { values.insert("round".into(), round.to_string()); }
            }
            Ok(values)
        })
        })
    }

    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>> {
        Box::pin(async move {
            self.read(|db,owner| {
            let mut query=db.prepare("SELECT work_id FROM work WHERE status IN ('queued','running') AND epoch=(SELECT epoch FROM room_meta) ORDER BY trigger_event_seq,rowid").map_err(|_|ReaderError::StoreUnavailable {message:"Store unavailable".into()})?;
            let ids=query.query_map([],|r|r.get::<_,String>(0)).map_err(|_|ReaderError::StaleWork)?.collect::<std::result::Result<Vec<_>,_>>().map_err(|_|ReaderError::StaleWork)?;
            let mut records=Vec::new();for id in ids {let id=id.parse().map_err(|_|ReaderError::StaleWork)?;match owned(db,owner,id) {Ok(record)=>records.push(record),Err(ReaderError::Forbidden)=>{},Err(e)=>return Err(e)}}Ok(records)
        })
        })
    }
    fn get(&self, id: WorkId) -> ReaderFuture<'_, Option<WorkRecord>> {
        Box::pin(async move {
            self.read(move |db, owner| match owned(db, owner, id) {
                Ok(record) => Ok(Some(record)),
                Err(ReaderError::NotFound) => Ok(None),
                Err(e) => Err(e),
            })
        })
    }
    fn repair_for_attempt(
        &self,
        id: WorkId,
        attempt: u32,
    ) -> ReaderFuture<'_, Option<JudgeRepairData>> {
        Box::pin(async move {
            self.read(move |db, owner| {
                let record = owned(db, owner, id)?;
                let ExecutorKey::Case(case) = owner else {
                    return Err(ReaderError::Forbidden);
                };
                if attempt == 0 || attempt > record.attempt {
                    return Err(ReaderError::Forbidden);
                }
                crate::judge_repairs::get(db, id, attempt, *case).map_err(map_error)
            })
        })
    }
}
