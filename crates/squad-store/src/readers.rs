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
    pub fn artifact_for_work(&self,id:WorkId,event:EventId) -> Result<serde_json::Value,ReaderError> {
        self.read(move |db,owner| {owned(db,owner,id)?;crate::contest_steps::artifact_for_work(db,id,event).map_err(|_|ReaderError::Forbidden)})
    }

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
                let mut history = crate::history::read(db, &work.context_id, work.history_through)
                    .map_err(map_error)?;
                if work.kind == WorkKind::Reply && work.round.is_some() {
                    history.retain(|entry| {
                        let HistoryMetadata::Attributed {
                            origin_event_id,
                            projection_key,
                            ..
                        } = &entry.meta
                        else {
                            return true;
                        };
                        if !projection_key.as_str().starts_with("tournament.") {
                            return true;
                        }
                        let Ok(Some(source)) = crate::events::by_id(db, *origin_event_id) else {
                            return false;
                        };
                        match source.content {
                            EnvelopePayload::TournamentResult(r) => {
                                Some(r.contest_id) == work.contest_id
                                    && Some(r.round + 1) == work.round
                            }
                            EnvelopePayload::TournamentEntry(e) => {
                                Some(e.contest_id) == work.contest_id
                            }
                            EnvelopePayload::TournamentState(s) => {
                                Some(s.contest_id) == work.contest_id
                            }
                            _ => false,
                        }
                    });
                }
                Ok(history)
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
            if work.kind.census_composition().is_some() {
                let (fence,epoch):(i64,i64)=db.query_row("SELECT lease_fence,epoch FROM work WHERE work_id=?",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|ReaderError::StaleWork)?;
                crate::work::check_fence(db,id,owner,work.attempt,fence as u64,epoch as u64,chrono::Utc::now()).map_err(|_|ReaderError::StaleWork)?;
                return crate::census::input(db,id).map_err(map_error);
            }
            let mut values = std::collections::BTreeMap::new();
            if work.kind.grill_composition().is_some(){
                let (fence,epoch):(i64,i64)=db.query_row("SELECT lease_fence,epoch FROM work WHERE work_id=?",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|ReaderError::StaleWork)?;
                crate::work::check_fence(db,id,owner,work.attempt,fence as u64,epoch as u64,chrono::Utc::now()).map_err(|_|ReaderError::StaleWork)?;
                let contest=work.contest_id.ok_or(ReaderError::Forbidden)?;
                let state=crate::grill::state(db,contest).map_err(map_error)?;
                if state.state.is_terminal(){return Err(ReaderError::StaleWork);}
                let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",rusqlite::params![contest.to_string(),format!("grill.work:{id}")],|r|r.get(0)).map_err(|_|ReaderError::Forbidden)?;
                let material:serde_json::Value=serde_json::from_str(&raw).map_err(|_|ReaderError::NotRunnable)?;
                values.insert("grill_rendered".into(),material["rendered"].to_string());
                values.insert("grill_input".into(),material["input"].to_string());
                return Ok(values);
            }
            if matches!(work.kind, WorkKind::CrosscheckInventory | WorkKind::CrosscheckCell | WorkKind::CrosscheckAudit | WorkKind::CrosscheckGoal | WorkKind::CrosscheckNormalize | WorkKind::CrosscheckRule) {
                let state=crate::crosscheck::state(db,work.contest_id.ok_or(ReaderError::Forbidden)?).map_err(map_error)?;
                let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE work_id=? AND lease_owner=? AND lease_until>?)",rusqlite::params![id.to_string(),serde_json::to_string(owner).map_err(|_|ReaderError::Forbidden)?,chrono::Utc::now().to_rfc3339()],|r|r.get(0)).map_err(|_|ReaderError::Forbidden)?;
                if state.status.is_terminal() || state.step_id!=work.step_id || !valid {return Err(ReaderError::StaleWork);}

                let material: String = db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",rusqlite::params![work.contest_id.ok_or(ReaderError::Forbidden)?.to_string(),format!("crosscheck.input:{}",id)],|r|r.get(0)).map_err(|_|ReaderError::Forbidden)?;
                let material:serde_json::Value=serde_json::from_str(&material).map_err(|_|ReaderError::Forbidden)?;
                values.insert("crosscheck_material_json".into(),material["value"].to_string());
                return Ok(values);
            }
            if let Some(contest)=work.contest_id
                && matches!(work.kind, WorkKind::SurveyExtract | WorkKind::SurveyVote | WorkKind::SurveyRevote | WorkKind::SurveyReport) {
                    let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key=?",rusqlite::params![contest.to_string(),format!("survey.work:{}",work.work_id)],|r|r.get(0)).map_err(|_|ReaderError::Forbidden)?;
                    return serde_json::from_str(&raw).map_err(|_|ReaderError::Forbidden);
            }
            if work.kind.is_probe() {
                values.insert("mode_data_json".into(), crate::probe::input(db,&work).map_err(map_error)?.to_string());
                let raw:String=db.query_row("SELECT plan_json FROM contests WHERE contest_id=?",[work.contest_id.ok_or(ReaderError::NotRunnable)?.to_string()],|r|r.get(0)).map_err(|_|ReaderError::NotFound)?;
                let plan:ProbePreflight=serde_json::from_str(&raw).map_err(|_|ReaderError::NotRunnable)?;
                // Only the selected composition and public word limits cross the capability.
                let id=work.kind.probe_composition().ok_or(ReaderError::NotRunnable)?;
                values.insert("probe_prompt_json".into(),plan.base.settings["prompts"][id].to_string());
                values.insert("room_words".into(),plan.base.settings["roomWords"].to_string());
                values.insert("plan_words".into(),plan.base.settings["planWords"].to_string());
                return Ok(values);
            }
            if let Some(case) = work.case_id {
                if !matches!(owner, ExecutorKey::Case(owned) if *owned == case) { return Err(ReaderError::Forbidden); }
                let (material, state): (String, String) = db.query_row("SELECT material_json,state_json FROM cases WHERE case_id=? AND epoch=(SELECT epoch FROM room_meta)", [case.to_string()], |r| Ok((r.get(0)?,r.get(1)?))).map_err(|_|ReaderError::NotFound)?;
                values.insert("case_material_json".into(), material);
                values.insert("case_state_json".into(), state);
                // A follow-up carries its own frozen invocation, distinct from the case's original one.
                let followup: Option<String> = db.query_row("SELECT json_extract(e.payload,'$.judgeInvocationId') FROM case_events c JOIN events e ON c.event_seq=e.seq WHERE c.case_id=? AND e.kind='case.followup' AND json_extract(e.payload,'$.workId')=?",rusqlite::params![case.to_string(),work.work_id.to_string()],|r|r.get(0)).map(Some).or_else(|e| if matches!(e, rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(e) }).map_err(|_|ReaderError::NotRunnable)?;
                if let Some(invocation) = followup {
                    values.insert("case_followup_invocation".into(), invocation);
                }
            }
            if work.kind==WorkKind::Reply && work.round.is_some() {
                let s=crate::tournaments::validate_execution(db,&work).map_err(map_error)?;
                values.insert("contest_id".into(),s.contest_id.to_string());
                values.insert("round".into(),s.round.to_string());
                values.insert("entrants_remaining".into(),s.survivors.len().to_string());
            }
            if work.kind==WorkKind::Reply && let Some(round)=work.round {
                let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='tournament.member_prompts'",[work.contest_id.ok_or(ReaderError::Forbidden)?.to_string()],|r|r.get(0)).map_err(|_|ReaderError::NotFound)?;
                let frozen:serde_json::Value=serde_json::from_str(&raw).map_err(|_|ReaderError::Forbidden)?;
                values.insert("tournament_frozen_system".into(),frozen["systems"][work.target.to_string()].as_str().ok_or(ReaderError::Forbidden)?.into());
                values.insert("tournament_frozen_instruction".into(),frozen["instructions"][round.to_string()].as_str().ok_or(ReaderError::Forbidden)?.into());
            }
            if work.kind==WorkKind::JudgeTournament {
                crate::tournaments::validate_execution(db,&work).map_err(map_error)?;
                let raw:String=db.query_row("SELECT state_json FROM tournament_rounds WHERE contest_id=? AND round=? AND replay=? AND judge_work_id=?",rusqlite::params![work.contest_id.unwrap().to_string(),work.round,work.replay,work.work_id.to_string()],|r|r.get(0)).map_err(|_|ReaderError::Forbidden)?;
                values.insert("tournament_frozen_judge".into(),raw);
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
                if let Some((work,_))=crate::work::template(db,id).map_err(map_error)? && work.kind.new_mode_kind().is_some() {
                    if attempt==0 || attempt>record.attempt {return Err(ReaderError::Forbidden);}
                    return crate::judge_repairs::get_mode(db,id,attempt).map_err(map_error);
                }
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
