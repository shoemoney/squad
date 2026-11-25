//! Probe is a three-step script over the shared allocation/barrier hooks.
use crate::router::{Router, envelope};
use rusqlite::params;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitOutcome};
use std::collections::BTreeMap;
impl Router {
    pub(super) fn drive_probe_contests(&mut self) -> Result<Vec<CommitOutcome>> {
        let ids = self.store.call(|db| {
            let mut q =
                db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='probe'")?;
            Ok(q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut outcomes = vec![];
        for raw in ids {
            let id: ContestId = raw.parse().map_err(|_| StoreError::Unavailable)?;
            if self.engine_scope.as_ref().is_some_and(|s| !s.contains(&id)) {
                continue;
            }
            for _ in 0..12 {
                let state = self
                    .store
                    .call(move |db| squad_store::probe::state(db, id))?;
                if state.state.is_terminal() {
                    break;
                }
                let blocked=self.store.call(move|db|{
                    let expired:bool=db.query_row("SELECT expires_at<=? AND json_extract(state_json,'$.state')='PREFLIGHT' FROM contests WHERE contest_id=?",params![chrono::Utc::now().to_rfc3339(),id.to_string()],|r|r.get(0))?;
                    let daily:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE contest_id=? AND status='failed' AND json_extract(error_json,'$.code')='DAILY_MAX_REACHED')",[id.to_string()],|r|r.get(0))?;Ok(expired||daily)
                })?;
                if blocked {
                    outcomes.push(self.transition(
                        EventId::new(),
                        ContestTransitionSubmission {
                            contest_id: id,
                            kind: ContestKind::Probe,
                            expected_state_version: state.state_version,
                            transition: ContestTransition::Probe(ProbeTransition::Fail),
                        },
                    )?);
                    break;
                }
                if state.state == ProbeStatus::Preflight {
                    break;
                }
                if let Some(step) = state.step_id {
                    let pending=self.store.call(move|db|{
                        let tx=db.transaction()?;
                        // Recovery consumes interrupted attempts; exhausted queued
                        // work must settle instead of waiting for a nonexistent wake.
                        tx.execute("UPDATE work SET status='failed',runnable=0 WHERE step_id=? AND status='queued' AND attempt>=CASE kind WHEN 'probe.report' THEN 3 ELSE 2 END",[step.to_string()])?;
                        tx.execute("UPDATE work SET status='queued',lease_owner=NULL,lease_until=NULL,lease_fence=lease_fence+1,completed_event_seq=NULL WHERE step_id=? AND status='failed' AND kind<>'probe.report' AND attempt<2 AND COALESCE(json_extract(error_json,'$.code'),'') NOT IN ('DAILY_MAX_REACHED','WORK_CANCELLED')",[step.to_string()])?;
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
                            data: serde_json::json!({"probeBarrier":true}),
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
                    if state.state == ProbeStatus::WaitingForParticipants {
                        let waiting=self.store.call(move|db|Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM work w JOIN probe_roles p ON p.nick=w.target WHERE p.contest_id=? AND p.role<>'sitting_out' AND w.kind='reply' AND w.status IN ('queued','running') AND w.held_by_contest_id IS NULL)",[id.to_string()],|r|r.get::<_,bool>(0))?))?;
                        if waiting {
                            break;
                        }
                    }
                    let setup = self
                        .store
                        .call(move |db| squad_store::probe::setup(db, id))?;
                    let actors = match state.state {
                        ProbeStatus::WaitingForParticipants => {
                            let nicks = setup.participants.clone();
                            let earlier=self.store.call(move|db|{for n in nicks {let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get(0))?;if pending {return Ok(true);}}Ok(false)})?;
                            if earlier {
                                break;
                            }
                            setup
                                .participants
                                .iter()
                                .map(|n| {
                                    (
                                        Target::Member(n.clone()),
                                        if setup.explainers.contains(n) {
                                            WorkKind::ProbeExplain
                                        } else {
                                            WorkKind::ProbeCheck
                                        },
                                    )
                                })
                                .collect::<Vec<_>>()
                        }
                        ProbeStatus::Predicting => {
                            let explainers = setup.explainers.clone();
                            self.store.call(move|db|{let mut actors=vec![];for n in explainers {let accepted:bool=db.query_row("SELECT initial_event_seq IS NOT NULL FROM probe_roles WHERE contest_id=? AND nick=?",params![id.to_string(),n.as_str()],|r|r.get(0))?;if accepted {actors.push((Target::Member(n),WorkKind::ProbePredict));}}Ok(actors)})?
                        }
                        ProbeStatus::Reporting => vec![(Target::Judge, WorkKind::ProbeReport)],
                        _ => break,
                    };
                    let step_id = StepId::new();
                    let case = CaseId::new();
                    let trigger=self.store.call(move|db|{let raw:String=db.query_row("SELECT e.id FROM contest_events c JOIN events e ON e.seq=c.event_seq WHERE c.contest_id=? ORDER BY e.seq DESC LIMIT 1",[id.to_string()],|r|r.get(0))?;raw.parse().map_err(|_|StoreError::Unavailable)})?;
                    let work = actors
                        .into_iter()
                        .map(|(target, kind)| {
                            let context_id = match &target {
                                Target::Member(n) => {
                                    ContextId(format!("contest:{id}:{step_id}:{n}"))
                                }
                                Target::Judge => ContextId(format!("judge:{case}")),
                            };
                            Work {
                                work_id: WorkId::new(),
                                message_id: MessageId::new(),
                                in_reply_to_message_id: None,
                                prompt_ref: Some(PromptRef {
                                    pack_version: Text::new("v1").expect("literal"),
                                    template_id: Text::new(
                                        kind.probe_composition().expect("probe"),
                                    )
                                    .expect("literal"),
                                    template_hash: Text::new("probe-v1").expect("literal"),
                                }),
                                max_output_tokens: Some(if target == Target::Judge {
                                    constants::FINAL_JUDGE_VISIBLE_TOKENS
                                } else {
                                    constants::DEBATE_TURN_VISIBLE_TOKENS
                                }),
                                case_id: (target == Target::Judge).then_some(case),
                                kind,
                                target,
                                context_id,
                                trigger_event_id: trigger,
                                history_through: HistoryPosition::new(1).expect("positive"),
                                attempt: 1,
                                debate_id: None,
                                round: None,
                                replay: None,
                                contest_id: Some(id),
                                step_id: Some(step_id),
                            }
                        })
                        .collect::<Vec<_>>();
                    let step = ContestStep {
                        contest_id: id,
                        step_id,
                        ordinal: match state.state {
                            ProbeStatus::WaitingForParticipants => 1,
                            ProbeStatus::Predicting => 2,
                            _ => 3,
                        },
                        script_version: Text::new("probe-v1").expect("literal"),
                        move_name: if state.state == ProbeStatus::Reporting {
                            ContestMove::Report
                        } else {
                            ContestMove::Fanout
                        },
                        inputs: vec![],
                        assignments: work
                            .iter()
                            .map(|w| ContestAssignment {
                                target: w.target.clone(),
                                context_id: w.context_id.clone(),
                                payload_refs: vec![],
                                work_id: Some(w.work_id),
                            })
                            .collect(),
                    };
                    outcomes.push(self.allocate_step(
                        EventId::new(),
                        step,
                        vec![],
                        work,
                        BTreeMap::new(),
                    )?);
                }
            }
        }
        Ok(outcomes)
    }
}
