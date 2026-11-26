//! Durable engine adapter. Each move commits before its successor is inspected.
use super::{
    cases,
    contests::{self, EngineAction, ExecutionSnapshot},
    machine, preflight, rounds,
    types::*,
};
use crate::{
    prompts::PromptPack,
    router::{Router, envelope},
};
use rusqlite::OptionalExtension;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitOutcome};
use std::collections::BTreeMap;

impl ContestRouter for Router {
    fn produce(&mut self, e: ProducerEnvelope) -> Result<CommitOutcome> {
        Router::produce(self, e)
    }
    fn transition(&mut self, id: EventId, s: ContestTransitionSubmission) -> Result<CommitOutcome> {
        Router::transition(self, id, s)
    }
    fn barrier(&mut self, id: EventId, c: BarrierCommit) -> Result<CommitOutcome> {
        Router::barrier(self, id, c)
    }
    fn allocate_work(
        &mut self,
        id: EventId,
        c: ContestId,
        v: u64,
        w: Vec<Work>,
    ) -> Result<CommitOutcome> {
        Router::allocate_work(self, id, c, v, w)
    }
    fn allocate_step(
        &mut self,
        id: EventId,
        s: ContestStep,
        p: Vec<HistoryProjection>,
        w: Vec<Work>,
        m: BTreeMap<String, serde_json::Value>,
    ) -> Result<CommitOutcome> {
        Router::allocate_step(self, id, s, p, w, m)
    }
    fn open_debate_case(
        &mut self,
        e: ProducerEnvelope,
        b: DebateAdjudicationBundle,
    ) -> Result<CommitOutcome> {
        Router::open_debate_case(self, e, b)
    }
    fn grill_state(&self, id: ContestId) -> Result<GrillState> {
        self.store.call(move |db| squad_store::grill::state(db, id))
    }
    fn debate_state(&self, id: DebateId) -> Result<DebateState> {
        Router::debate_state(self, id)
    }
}
fn policy(code: ErrorCode) -> StoreError {
    StoreError::Domain(code, "Contest policy rejected the move")
}
fn carrier(payload: EnvelopePayload) -> ProducerEnvelope {
    envelope(EventId::new(), Actor::System, vec![Recipient::Me], payload)
}
struct Snapshot {
    state: DebateState,
    plan: Option<DebatePlan>,
    prior: Option<DebateState>,
    events: Vec<CommittedEnvelope>,
    work: Vec<AttemptState>,
    committed: Vec<u32>,
    scores: Vec<DebateRoundScore>,
    earlier: [bool; 2],
}
impl Router {
    fn engine_snapshot(&self, id: ContestId) -> Result<Snapshot> {
        self.store.call(move |db| {
            let state=squad_store::contests::debate(db,id)?;
            let prior:Option<String>=db.query_row("SELECT json_extract(pending_request_json,'$.priorState') FROM contests WHERE contest_id=?",[id.to_string()],|r|r.get(0))?;
            let prior=prior.map(|raw|serde_json::from_str(&raw)).transpose()?;
            let raw:Option<String>=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='engine.plan'",[id.to_string()],|r|r.get(0)).optional()?;
            let plan=raw.map(|s|serde_json::from_str(&s)).transpose()?;
            let mut q=db.prepare("SELECT event_seq FROM contest_events WHERE contest_id=? ORDER BY event_seq")?;
            let events=q.query_map([id.to_string()],|r|r.get::<_,i64>(0))?.map(|r|squad_store::events::at(db,r?)).collect::<Result<Vec<_>>>()?;
            let mut q=db.prepare("SELECT work_id,completed_event_seq FROM work WHERE debate_id=? ORDER BY rowid")?;
            let ids=q.query_map([id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut work=Vec::new();
            for (id,seq) in ids {
                let id=id.parse().map_err(|_|StoreError::Unavailable)?;
                let record=squad_store::work::record(db,id)?.ok_or(StoreError::Unavailable)?;
                let mut item=squad_store::work::template(db,id)?.ok_or(StoreError::Unavailable)?.0;
                item.attempt=record.attempt;
                let phase=if let Some(seq)=seq { let e=squad_store::events::at(db,seq)?; if matches!(e.content,EnvelopePayload::DebateTurn(_)|EnvelopePayload::DebateRoundScore(_)|EnvelopePayload::Verdict(_)){AttemptPhase::Accepted(e.base.id)}else{AttemptPhase::Exhausted} }else{match record.status {WorkStatus::Queued=>AttemptPhase::Queued,WorkStatus::Running=>AttemptPhase::Running,WorkStatus::Cancelled=>AttemptPhase::Cancelled,_=>AttemptPhase::Exhausted}};
                work.push(AttemptState{work:item,deadline:record.deadline,phase,repair_used:false});
            }
            let committed=events.iter().filter_map(|e|if let EnvelopePayload::ContestReport(r)=&e.content {r.data.get("round").and_then(|v|v.as_u64()).map(|r|r as u32)}else{None}).filter(|r|*r>0).collect();
            let scores=squad_store::round_scores::retained(db,id)?;
            let mut earlier=[false;2];
            for (i,n) in state.debaters.iter().enumerate(){earlier[i]=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get(0))?;}
            Ok(Snapshot{state,plan,prior,events,work,committed,scores,earlier})
        })
    }
    /// Called on the serialized router thread after every committed input and at
    /// interactive startup. No public Internal ingress is used by the engine.
    pub fn drive_contests(&mut self) -> Result<Vec<CommitOutcome>> {
        if self.engine_config.is_none() {
            return Ok(vec![]);
        }
        let ids=self.store.call(|db|{
            let mut q=db.prepare("SELECT contest_id FROM contests WHERE active=1 AND kind='debate' AND json_type(state_json,'$.debaters')='array'")?;
            Ok(q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        let mut outcomes = self.drive_tournaments()?;
        outcomes.extend(self.drive_grills()?);
        outcomes.extend(self.drive_census()?);
        outcomes.extend(self.drive_surveys()?);
        outcomes.extend(self.drive_probe_contests()?);
        for id in ids {
            let id = id.parse().map_err(|_| StoreError::Unavailable)?;
            if self
                .engine_scope
                .as_ref()
                .is_some_and(|scope| !scope.contains(&id))
            {
                continue;
            }
            outcomes.extend(contests::drive_script(
                || -> Result<Option<CommitOutcome>> {
                    let s = self.engine_snapshot(id)?;
                    if s.state.state.is_terminal() {
                        return Ok(None);
                    }
                    let failed_state = s.state.clone();
                    match self.engine_move(s) {
                        Ok(moved) => Ok(moved),
                        Err(error) => {
                            tracing::error!(?error,contest_id=%id,"Contest engine move failed");
                            self.fail_engine(failed_state, error).map(Some)
                        }
                    }
                },
            )?);
        }
        outcomes.extend(self.drive_crosschecks()?);
        Ok(outcomes)
    }
    fn fail_engine(&self, mut state: DebateState, failure: StoreError) -> Result<CommitOutcome> {
        let id = EventId::new();
        let code = if let StoreError::Domain(code, _) = failure {
            code
        } else {
            ErrorCode::INTERNAL_ERROR
        };
        self.store.commit(
            id,
            "contest-engine".into(),
            squad_store::commit::digest(&(state.debate_id, state.state_version, code))?,
            move |tx, group| {
                let version = state.state_version;
                let prior:Option<String>=tx.query_row("SELECT json_extract(pending_request_json,'$.priorState') FROM contests WHERE contest_id=?",[state.debate_id.to_string()],|r|r.get(0))?;
                if state.state==DebateStatus::Preparing && let Some(prior)=prior {
                    state=serde_json::from_str(&prior)?;
                } else {
                    state.state=if state.state==DebateStatus::Preparing {DebateStatus::Rejected}else{DebateStatus::Failed};
                    state.terminal_reason=Some(format!("{code:?}"));
                }
                state.state_version=version+1;
                let mut error = crate::router::error(code, "Contest could not advance");
                error.debate_id = Some(state.debate_id);
                error.contest_id = Some(state.debate_id);
                group.push(tx, carrier(EnvelopePayload::Error(error.clone())))?;
                crate::router::debates::save(tx, group, id, &state, Some(version))?;
                Ok(RequestResult::Rejected {
                    request_id: id,
                    error,
                })
            },
        )
    }
    fn engine_move(&mut self, s: Snapshot) -> Result<Option<CommitOutcome>> {
        if s.state.state == DebateStatus::Preparing {
            let plan = match &s.plan {
                Some(plan) if plan.prior_terminal == s.prior => plan.clone(),
                _ => self.freeze_plan(&s)?,
            };
            return self
                .produce(carrier(EnvelopePayload::DebatePreflight(plan.preflight)))
                .map(Some);
        }
        let Some(plan) = &s.plan else { return Ok(None) };
        if s.state.state == DebateStatus::AwaitingConfirmation
            && chrono::Utc::now() >= plan.preflight.expires_at
        {
            return machine::submit(
                self,
                EventId::new(),
                &s.state,
                if plan.preflight.judge_only {
                    DebateTransition::JudgeOnlyExpired
                } else {
                    DebateTransition::Expire
                },
            )
            .map(Some);
        }
        if plan.preflight.judge_only && s.state.state == DebateStatus::Judging {
            let confirmed=s.events.iter().rev().find(|e|matches!(&e.content,EnvelopePayload::DebatePreflightResult(r) if r.plan_id==plan.preflight.plan_id && r.status==PreflightStatus::Confirmed)).map(|e|e.commit.seq.get()).ok_or(StoreError::Unavailable)?;
            if !s.events.iter().any(|e| {
                e.commit.seq.get() > confirmed
                    && matches!(e.content, EnvelopePayload::CaseOpened(_))
            }) {
                return self.open_engine_case(&s).map(Some);
            }
        }
        if s.state.state == DebateStatus::WaitingForDebaters && !s.earlier.iter().any(|v| *v) {
            if !s.events.iter().any(|e| {
                matches!(
                    e.content,
                    EnvelopePayload::DebateOpening(_) | EnvelopePayload::DebateSetup(_)
                )
            }) {
                let payload = if let Some(solutions) = &plan.sources.solutions {
                    EnvelopePayload::DebateSetup(setup(plan, solutions.clone()))
                } else {
                    EnvelopePayload::DebateOpening(DebateOpening {
                        debate_id: s.state.debate_id,
                        debaters: s.state.debaters.clone(),
                        topic_message_id: plan.sources.topic_message_id,
                        topic_text: Text::new(plan.sources.topic.clone())
                            .map_err(|_| policy(ErrorCode::DEBATE_SOURCE_MISSING))?,
                        instruction: plan.sources.instruction.clone(),
                        rounds: plan.preflight.rounds,
                        round: 0,
                    })
                };
                return self.produce(carrier(payload)).map(Some);
            }
            if s.state.judge_requested
                && s.state.case_id.is_none()
                && plan.sources.solutions.is_some()
            {
                return self.open_engine_case(&s).map(Some);
            }
        }
        let round_work = s
            .work
            .iter()
            .filter(|w| w.work.kind == WorkKind::DebateTurn && w.work.round == Some(s.state.round))
            .cloned()
            .collect::<Vec<_>>();
        let scoring_work = s
            .work
            .iter()
            .filter(|w| w.work.kind == WorkKind::JudgeRound)
            .cloned()
            .collect::<Vec<_>>();
        if s.state.state == DebateStatus::Judging
            && s.work.iter().any(|w| {
                w.work.kind == WorkKind::Judge
                    && w.work.case_id == s.state.case_id
                    && w.phase == AttemptPhase::Exhausted
            })
        {
            return machine::submit(
                self,
                EventId::new(),
                &s.state,
                DebateTransition::FinalJudgingFailed,
            )
            .map(Some);
        }
        let next = contests::next(
            &ExecutionSnapshot {
                state: &s.state,
                round_work: &round_work,
                scoring_work: &scoring_work,
                committed_rounds: &s.committed,
                scores: &s.scores,
                final_work_exists: s
                    .work
                    .iter()
                    .any(|w| w.work.kind == WorkKind::Judge && w.work.case_id == s.state.case_id),
                manual_judge: plan.preflight.judge_only,
                earlier_unfinished: s.earlier,
            },
            chrono::Utc::now(),
        )
        .map_err(policy)?;
        match next {
            EngineAction::Wait | EngineAction::Attempt(AttemptDecision::Wake(_)) => Ok(None),
            EngineAction::Transition(t) => {
                machine::submit(self, EventId::new(), &s.state, t).map(Some)
            }
            EngineAction::AllocateRound(round) => {
                let work = (0..2)
                    .map(|i| {
                        rounds::prepare_turn(
                            plan,
                            engine_work(
                                &s,
                                Target::Member(s.state.debaters[i].clone()),
                                WorkKind::DebateTurn,
                                Some(round),
                            )?,
                        )
                        .map_err(policy)
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.allocate_work(
                    EventId::new(),
                    s.state.debate_id,
                    s.state.state_version,
                    work,
                )
                .map(Some)
            }
            EngineAction::CommitRoundBarrier(round) => {
                if round == 0 {
                    if !s
                        .events
                        .iter()
                        .any(|e| matches!(e.content, EnvelopePayload::DebateSetup(_)))
                    {
                        let turns = turns(&s, 0)?;
                        let turns = turns.map(|e| match e.content {
                            EnvelopePayload::DebateTurn(t) => t,
                            _ => unreachable!(),
                        });
                        return self
                            .produce(carrier(EnvelopePayload::DebateSetup(
                                rounds::opening_setup(plan, &turns).map_err(policy)?,
                            )))
                            .map(Some);
                    }
                    if s.state.judge_requested && s.state.case_id.is_none() {
                        return self.open_engine_case(&s).map(Some);
                    }
                }
                self.engine_barrier(&s).map(Some)
            }
            EngineAction::AllocateRoundScore(round) => {
                self.refresh_bundle(&s)?;
                rounds::allocate_judge(
                    self,
                    EventId::new(),
                    &s.state,
                    plan,
                    engine_work(&s, Target::Judge, WorkKind::JudgeRound, Some(round))?,
                )
                .map(Some)
            }
            EngineAction::AllocateFinalJudge => {
                self.refresh_bundle(&s)?;
                rounds::allocate_judge(
                    self,
                    EventId::new(),
                    &s.state,
                    plan,
                    engine_work(&s, Target::Judge, WorkKind::Judge, None)?,
                )
                .map(Some)
            }
            EngineAction::Attempt(AttemptDecision::Forfeit(id)) => {
                let w = &s
                    .work
                    .iter()
                    .find(|w| w.work.work_id == id)
                    .ok_or(StoreError::Unavailable)?
                    .work;
                let Target::Member(nick) = &w.target else {
                    return Err(policy(ErrorCode::INVALID_REFERENCE));
                };
                self.produce(carrier(EnvelopePayload::DebateTurn(DebateTurn {
                    generation: GenerationIdentity {
                        work_id: id,
                        message_id: w.message_id,
                        trigger_event_id: w.trigger_event_id,
                        attempt: w.attempt,
                    },
                    debate_id: s.state.debate_id,
                    round: w.round.ok_or(StoreError::Unavailable)?,
                    speaker: nick.clone(),
                    outcome: rounds::forfeit(ForfeitReason::Error),
                })))
                .map(Some)
            }
            EngineAction::Attempt(AttemptDecision::Unscored(id)) => {
                let w = &s
                    .work
                    .iter()
                    .find(|w| w.work.work_id == id)
                    .ok_or(StoreError::Unavailable)?
                    .work;
                let score = super::judge::round_record(
                    &s.state,
                    w.round.ok_or(StoreError::Unavailable)?,
                    None,
                    &s.scores,
                )
                .map_err(policy)?;
                self.produce(carrier(EnvelopePayload::DebateRoundScore(score)))
                    .map(Some)
            }
            EngineAction::Attempt(AttemptDecision::JudgeFailed(_)) => machine::submit(
                self,
                EventId::new(),
                &s.state,
                DebateTransition::FinalJudgingFailed,
            )
            .map(Some),
            _ => Ok(None),
        }
    }
    fn freeze_plan(&self, s: &Snapshot) -> Result<DebatePlan> {
        let config = self
            .engine_config
            .as_ref()
            .ok_or(StoreError::Unavailable)?
            .clone();
        let id = s.state.debate_id;
        let state = s.state.clone();
        let bindings = self.bindings_for_engine();
        let prior = s.prior.clone();
        let retained = s.events.iter().find_map(|e| {
            if let EnvelopePayload::DebateSetup(setup) = &e.content {
                Some(setup.clone())
            } else {
                None
            }
        });
        let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
            .map_err(|_| policy(ErrorCode::CONFIG_INVALID))?;
        self.store.call(move |db|{
            let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='admission'",[id.to_string()],|r|r.get(0))?;
            let admission:serde_json::Value=serde_json::from_str(&raw)?;
            let request:Request=serde_json::from_value(admission["request"].clone())?;
            let Request::UserMessage{input,parsed:ParsedInput::Debate{instruction,..},..}=request else{return Err(policy(ErrorCode::INVALID_COMMAND))};
            let events:Vec<CommittedEnvelope>=serde_json::from_value(admission["sources"].clone())?;
            // Selections name committed events; a reply's solution identity is its generation message id.
            let selected_ids:BTreeMap<EventId,MessageId>=events.iter().filter_map(|e|if let EnvelopePayload::MessageFinal(m)=&e.content{Some((e.base.id,m.completed.generation.message_id))}else{None}).collect();
            let mut replies=Vec::new();
            for e in events {
                if let EnvelopePayload::MessageFinal(m)=e.content {
                    let topic=squad_store::events::by_id(db,m.in_reply_to_message_id.to_string().parse().map_err(|_|StoreError::Unavailable)?)?.and_then(|e|if let EnvelopePayload::UserMessage(m)=e.content{Some(m.body)}else{None}).ok_or(policy(ErrorCode::DEBATE_SOURCE_MISSING))?;
                    let Actor::Member(nick)=e.base.sender else{continue};
                    replies.push(super::sources::SourceReply{solution:DebateSolution{nick,message_id:m.completed.generation.message_id,text:m.content},topic_message_id:m.in_reply_to_message_id,topic,accepted_order:e.commit.seq.get() as u64,completed:true,superseded:false,ordinary_to_human:true});
                }
            }
            let explicit:Vec<MessageId>=input.reply_to.unwrap_or_default().into_iter().map(|id|selected_ids.get(&id).copied().unwrap_or_else(||MessageId::from_event(id))).collect();
            let mut sources=super::sources::select(&state.debaters,&replies,&explicit,u64::MAX,input.message_id,&instruction).map_err(policy)?;
            if prior.is_some() {sources.solutions=Some(retained.ok_or(policy(ErrorCode::DEBATE_SOURCE_MISSING))?.solutions);}
            let judge_requested=state.judge_requested || prior.is_some();
            let freeze_model=|nick:&str|->Result<FrozenModel>{
                let binding=bindings.get(nick).ok_or(policy(ErrorCode::MODEL_UNAVAILABLE))?;
                let (length,pricing):(i64,String)=db.query_row("SELECT context_length,pricing_json FROM catalog WHERE slug=?",[binding.model.as_str()],|r|Ok((r.get(0)?,r.get(1)?)))?;
                let pricing:serde_json::Value=serde_json::from_str(&pricing)?;
                let price=|key:&str| pricing[key].as_str().and_then(|v|v.parse::<f64>().ok()).or_else(||pricing[key].as_f64()).unwrap_or(0.0)*1e6;
                Ok(FrozenModel{nick:MemberNick::new(nick).ok(),resolved_slug:binding.model.to_string(),context_tokens:length as u64,input_usd_per_million:price("prompt"),output_usd_per_million:price("completion"),reasoning_tokens:match binding.effort{ReasoningEffort::Low=>constants::REASONING_LOW_TOKENS,ReasoningEffort::Medium=>constants::REASONING_MEDIUM_TOKENS,ReasoningEffort::High=>constants::REASONING_HIGH_TOKENS},reasoning_effort:Some(binding.effort)})
            };
            let members=[freeze_model(state.debaters[0].as_str())?,freeze_model(state.debaters[1].as_str())?];
            let (judge,warning)=if judge_requested {
                let default=freeze_model("judge")?;
                let fallbacks=config.judge.conflict_fallbacks.iter().filter_map(|n|freeze_model(n.as_str()).ok()).collect::<Vec<_>>();
                let (j,w)=super::judge::select_model(&default,&fallbacks,&members).map_err(policy)?;(Some(j),w)
            }else{(None,None)};
            let hashes = pack.composition_hashes().map_err(|_| policy(ErrorCode::CONFIG_INVALID))?;
            let mut plan=DebatePlan{preflight:DebatePreflight{judge_only:prior.is_some(),debate_id:id,plan_id:PlanId::new(),expires_at:chrono::Utc::now(),estimate:DebateEstimate{estimated_usd:0.,reserved_usd:0.,remaining_usd:0.,model_calls:0},judge_model:judge.as_ref().map(|j|j.resolved_slug.clone()),warning:warning.map(Text::new).transpose().map_err(|_|StoreError::Unavailable)?,confirmation_required:false,debaters:state.debaters.clone(),rounds:state.total_rounds,judge_requested,opening_round_required:sources.solutions.is_none()},sources,members,judge,prompts:FrozenPrompts{pack_version:pack.version().into(),composition_hashes:hashes,turn_words:config.prompts.limits.turn_words,final_turn_words:config.prompts.limits.final_turn_words},script_version:"debate-v1".into(),calls:vec![],prior_terminal:prior};
            let mut bounds=[0;2];
            for (i,n) in state.debaters.iter().enumerate(){let size:i64=db.query_row("SELECT COALESCE(SUM(length(content)),0) FROM history WHERE context_id=?",[n.as_str()],|r|r.get(0))?;bounds[i]=size as u64+plan.sources.topic.len() as u64+plan.sources.instruction.len() as u64+4096;}
            let judge_bound=plan.sources.topic.len() as u64+8192;
            preflight::freeze(&mut plan,preflight::ContextBounds{members:bounds,judge:judge_bound},f64::MAX,config.budget.debate_confirm_usd,chrono::Utc::now()).map_err(policy)?;
            let tx=db.transaction()?;
            tx.execute("INSERT INTO contest_private_maps VALUES (?,'engine.plan',?) ON CONFLICT(contest_id,scope_key) DO UPDATE SET map_json=excluded.map_json",rusqlite::params![id.to_string(),serde_json::to_string(&plan)?])?;
            tx.commit()?;Ok(plan)
        })
    }
    fn bindings_for_engine(&self) -> BTreeMap<String, ExecutorBinding> {
        self.engine_bindings()
    }
    fn open_engine_case(&mut self, s: &Snapshot) -> Result<CommitOutcome> {
        let plan = s.plan.as_ref().ok_or(StoreError::Unavailable)?;
        let setup = retained_setup(s)?;
        let mut state = s.state.clone();
        let case = CaseId::new();
        state.case_id = Some(case);
        state.case_ids.push(case);
        let transcript = s
            .events
            .iter()
            .filter_map(|e| {
                if let EnvelopePayload::DebateTurn(t) = &e.content {
                    (t.round > 0 && s.committed.contains(&t.round)).then_some(t.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let bundle = cases::bundle(
            plan,
            &state,
            &setup,
            &transcript,
            &s.scores,
            &s.committed,
            cases::canonical_rubric("v1").map_err(policy)?,
        )
        .map_err(policy)?;
        let opened = cases::debate(
            case,
            EventId::new(),
            Text::new(state.judge_model.clone())
                .map_err(|_| policy(ErrorCode::MODEL_UNAVAILABLE))?,
            setup,
            Text::new("v1").expect("literal"),
        )
        .map_err(policy)?;
        self.open_debate_case(carrier(EnvelopePayload::CaseOpened(opened)), bundle)
    }
    fn refresh_bundle(&self, s: &Snapshot) -> Result<()> {
        let plan = s.plan.as_ref().ok_or(StoreError::Unavailable)?;
        let transcript = s
            .events
            .iter()
            .filter_map(|e| {
                if let EnvelopePayload::DebateTurn(t) = &e.content {
                    (t.round > 0 && s.committed.contains(&t.round)).then_some(t.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let bundle = cases::bundle(
            plan,
            &s.state,
            &retained_setup(s)?,
            &transcript,
            &s.scores,
            &s.committed,
            cases::canonical_rubric("v1").map_err(policy)?,
        )
        .map_err(policy)?;
        let source = s.events.last().cloned().ok_or(StoreError::Unavailable)?;
        let case = s.state.case_id.ok_or(StoreError::Unavailable)?;
        self.store.call(move |db| {
            let tx = db.transaction()?;
            squad_store::history::project(
                &tx,
                &squad_store::history::attributed(
                    &source,
                    cases::context(case),
                    "case.bundle",
                    HistoryRole::User,
                    None,
                    serde_json::to_string(&bundle)?,
                ),
            )?;
            tx.commit()?;
            Ok(())
        })
    }
    fn engine_barrier(&mut self, s: &Snapshot) -> Result<CommitOutcome> {
        let turns = turns(s, s.state.round)?;
        let projections = if s.state.round == 0 {
            vec![]
        } else {
            rounds::barrier_projections(&s.state, &turns).map_err(policy)?
        };
        let state = s.state.clone();
        // Persist barrier allocation, deliveries and state advance in one commit.
        let id = EventId::new();
        let step = ContestStep {
            contest_id: state.debate_id,
            step_id: StepId::new(),
            ordinal: state.round + 1,
            script_version: Text::new("debate-v1").expect("literal"),
            move_name: ContestMove::Barrier,
            inputs: turns.iter().map(|e| e.base.id).collect(),
            assignments: vec![],
        };
        self.store.commit(
            id,
            "contest-engine".into(),
            squad_store::commit::digest(&step)?,
            move |tx, group| {
                let current = squad_store::contests::debate(tx, state.debate_id)?;
                if current.state_version != state.state_version {
                    return Err(policy(ErrorCode::INVALID_STATE_TRANSITION));
                }
                let seq = group.push(tx, carrier(EnvelopePayload::ContestStep(step.clone())))?;
                squad_store::contest_steps::insert(tx, &step, seq)?;
                let report = ContestReport {
                    contest_id: state.debate_id,
                    step_id: step.step_id,
                    artifact_event_ids: vec![],
                    skipped_work_ids: vec![],
                    data: serde_json::json!({"round":state.round}),
                };
                let seq = group.push(tx, carrier(EnvelopePayload::ContestReport(report)))?;
                squad_store::contest_steps::barrier(tx, step.step_id, seq)?;
                squad_store::contests::associate(tx, state.debate_id, ContestKind::Debate, seq)?;
                for p in projections {
                    squad_store::history::project(tx, &p)?;
                }
                let signal = if state.round == 0 {
                    DebateTransition::OpeningBarrierCommitted
                } else if state.round < state.total_rounds {
                    DebateTransition::NumberedBarrierCommitted
                } else if state.judge_requested {
                    DebateTransition::JudgedFinalBarrierCommitted
                } else {
                    DebateTransition::UnjudgedFinalBarrierCommitted
                };
                crate::router::debates::transition(
                    tx,
                    group,
                    id,
                    machine::submission(&state, signal),
                )?;
                Ok(group.accepted(id))
            },
        )
    }
}
fn retained_setup(s: &Snapshot) -> Result<DebateSetup> {
    s.events
        .iter()
        .find_map(|e| {
            if let EnvelopePayload::DebateSetup(v) = &e.content {
                Some(v.clone())
            } else {
                None
            }
        })
        .ok_or(StoreError::Unavailable)
}
fn setup(p: &DebatePlan, solutions: [DebateSolution; 2]) -> DebateSetup {
    DebateSetup {
        debate_id: p.preflight.debate_id,
        topic_message_id: p.sources.topic_message_id,
        topic_text: p.sources.topic.clone(),
        solutions,
        instruction: p.sources.instruction.clone(),
        rounds: p.preflight.rounds,
        judge_model: p.preflight.judge_model.clone().unwrap_or_default(),
        judge_swapped: p.preflight.warning.is_some(),
    }
}
fn turns(s: &Snapshot, round: u32) -> Result<[CommittedEnvelope; 2]> {
    s.state.debaters.iter().map(|n|s.events.iter().find(|e|matches!(&e.content,EnvelopePayload::DebateTurn(t) if t.round==round && &t.speaker==n)).cloned().ok_or(StoreError::Unavailable)).collect::<Result<Vec<_>>>()?.try_into().map_err(|_|StoreError::Unavailable)
}
fn engine_work(s: &Snapshot, target: Target, kind: WorkKind, round: Option<u32>) -> Result<Work> {
    let context = match &target {
        Target::Member(n) => ContextId(n.to_string()),
        Target::Judge => cases::context(s.state.case_id.ok_or(StoreError::Unavailable)?),
    };
    Ok(Work {
        work_id: WorkId::new(),
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind,
        target,
        context_id: context,
        trigger_event_id: s.events.last().ok_or(StoreError::Unavailable)?.base.id,
        history_through: HistoryPosition::new(1).map_err(|_| StoreError::Unavailable)?,
        attempt: 1,
        debate_id: Some(s.state.debate_id),
        round,
        replay: None,
        case_id: None,
        prompt_ref: None,
        max_output_tokens: None,
        contest_id: Some(s.state.debate_id),
        step_id: None,
    }
    .with_case(s.state.case_id))
}
trait CaseWork {
    fn with_case(self, case: Option<CaseId>) -> Self;
}
impl CaseWork for Work {
    fn with_case(mut self, case: Option<CaseId>) -> Self {
        if self.target == Target::Judge {
            self.case_id = case;
        }
        self
    }
}
