//! Drive the sequential grill script through the shared trusted router hooks.
use crate::router::{Router, envelope, grill};
use rusqlite::{OptionalExtension, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitOutcome};
use std::collections::BTreeMap;
impl Router {
    pub(crate) fn drive_grills(&mut self) -> Result<Vec<CommitOutcome>> {
        let ids=self.store.call(|db|{let mut q=db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='grill' AND json_type(state_json,'$.leader')='text'")?;Ok(q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)})?;
        if ids.is_empty() {
            self.grill_active = false;
        }
        let mut outcomes = vec![];
        for raw in ids {
            let id: ContestId = raw.parse().map_err(|_| StoreError::Unavailable)?;
            if self.engine_scope.as_ref().is_some_and(|s| !s.contains(&id)) {
                continue;
            }
            for _ in 0..512 {
                let state = self
                    .store
                    .call(move |db| squad_store::grill::state(db, id))?;
                if state.state.is_terminal() {
                    self.grill_active = false;
                    break;
                }
                if state.state == GrillStatus::Preflight {
                    let expired = self.store.call(move |db| {
                        let raw: String = db.query_row(
                            "SELECT plan_json FROM contests WHERE contest_id=?",
                            [id.to_string()],
                            |r| r.get(0),
                        )?;
                        Ok(serde_json::from_str::<ContestPreflight>(&raw)?)
                    })?;
                    if chrono::Utc::now() >= expired.expires_at {
                        let max = self.daily_max;
                        outcomes.push(self.store.commit(
                            EventId::new(),
                            "contest-engine".into(),
                            format!("expire:{}", expired.plan_id),
                            move |tx, g| {
                                grill::resolve(
                                    tx,
                                    g,
                                    EventId::new(),
                                    id,
                                    expired.plan_id,
                                    false,
                                    max,
                                )
                            },
                        )?);
                    }
                    break;
                }
                if state.state == GrillStatus::WaitingForParticipants {
                    let waiting=self.store.call(move|db|{for n in squad_store::grill::setup(db,id)?.setup.participants(){if db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL AND json_type(scheduling_meta,'$.openModeDormant') IS NULL)",[n.as_str()],|r|r.get::<_,bool>(0))?{return Ok(true);}}Ok(false)})?;
                    if waiting {
                        break;
                    }
                    outcomes.push(self.transition(
                        EventId::new(),
                        ContestTransitionSubmission {
                            contest_id: id,
                            kind: ContestKind::Grill,
                            expected_state_version: state.state_version,
                            transition: ContestTransition::Grill(
                                GrillTransition::ParticipantsReady,
                            ),
                        },
                    )?);
                    continue;
                }
                if let Some(step) = self.store.call(move |db| grill::current_step(db, id))? {
                    let pending=self.store.call(move|db|{
                        // Reads precede the writes: lock first so a second connection makes this wait, not fail.
                        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                        let now=chrono::Utc::now();
                        let expired:Option<String>=tx.query_row("SELECT work_id FROM work WHERE step_id=? AND status='running' AND (execution_deadline<=? OR lease_until<=?)",params![step.to_string(),now.to_rfc3339(),now.to_rfc3339()],|r|r.get(0)).optional()?;
                        if let Some(raw)=expired{
                            let wid:WorkId=raw.parse().map_err(|_|StoreError::Unavailable)?;
                            let w=squad_store::work::template(&tx,wid)?.ok_or(StoreError::Unavailable)?.0;
                            let mut failure=crate::router::error(ErrorCode::PROVIDER_TIMEOUT,"Grill attempt deadline or lease expired");failure.work_id=Some(wid);failure.contest_id=w.contest_id;
                            tx.execute("UPDATE work SET error_json=? WHERE work_id=?",params![serde_json::to_string(&failure)?,raw])?;
                            squad_store::leases::recover_work(&tx,wid,now)?;
                        }
                        tx.execute("UPDATE work SET status='failed',runnable=0 WHERE step_id=? AND status='queued' AND attempt>=2",[step.to_string()])?;
                        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE step_id=? AND status IN ('queued','running'))",[step.to_string()],|r|r.get(0))?;
                        tx.commit()?;Ok(pending)
                    })?;
                    if pending {
                        break;
                    }
                    let event = envelope(
                        EventId::new(),
                        Actor::System,
                        vec![Recipient::Me],
                        EnvelopePayload::ContestReport(ContestReport {
                            contest_id: id,
                            step_id: step,
                            artifact_event_ids: vec![],
                            skipped_work_ids: vec![],
                            data: serde_json::json!({"grillBarrier":true}),
                        }),
                    );
                    outcomes.push(self.barrier(
                        EventId::new(),
                        BarrierCommit {
                            contest_id: id,
                            expected_state_version: state.state_version,
                            step_id: step,
                            barrier_event: event,
                            projections: vec![],
                            next_work: vec![],
                        },
                    )?);
                } else {
                    let frozen = self.store.call(move |db| grill::provenance(db, id))?;
                    let step_id = StepId::new();
                    let trigger=self.store.call(move|db|{let raw:String=db.query_row("SELECT e.id FROM contest_events c JOIN events e ON e.seq=c.event_seq WHERE c.contest_id=? ORDER BY e.seq DESC LIMIT 1",[id.to_string()],|r|r.get(0))?;raw.parse().map_err(|_|StoreError::Unavailable)})?;
                    let prompt = &frozen["prompts"][grill::prompt_key(&state)];
                    let work = grill::work(
                        &state,
                        step_id,
                        trigger,
                        frozen["packVersion"]
                            .as_str()
                            .ok_or(StoreError::Unavailable)?,
                        prompt["hash"].as_str().ok_or(StoreError::Unavailable)?,
                    )?;
                    let ordinal=self.store.call(move|db|Ok(db.query_row("SELECT COALESCE(MAX(ordinal),0)+1 FROM contest_steps WHERE contest_id=?",[id.to_string()],|r|r.get(0))?))?;
                    let step = ContestStep {
                        contest_id: id,
                        step_id,
                        ordinal,
                        script_version: Text::new("grill-v1").expect("literal"),
                        move_name: ContestMove::Turns,
                        inputs: vec![],
                        assignments: vec![ContestAssignment {
                            target: work.target.clone(),
                            context_id: work.context_id.clone(),
                            payload_refs: vec![],
                            work_id: Some(work.work_id),
                        }],
                    };
                    outcomes.push(self.allocate_step(
                        EventId::new(),
                        step,
                        vec![],
                        vec![work],
                        BTreeMap::new(),
                    )?);
                }
            }
        }
        Ok(outcomes)
    }
}
