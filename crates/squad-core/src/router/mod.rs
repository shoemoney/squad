//! Serialized authority for all room mutations. Executors receive scoped ingress only.
pub mod accounting;
pub mod authorization;
pub mod cases;
pub mod census;
pub mod clear;
pub mod contests;
pub mod debates;
pub mod ephemeral;
pub mod executor;
pub mod picks;
pub mod preflight;
pub mod recovery;
pub mod references;
pub mod regen;
pub mod retry;
pub mod stop;
pub mod variables;
pub mod verdict;
use rusqlite::{OptionalExtension, params};
use squad_protocol::*;
use squad_store::{Result, Store, StoreError, commit::CommitOutcome};
use std::collections::BTreeMap;

pub struct Router {
    pub store: Store,
    pub engine_config: Option<SquadConfig>,
    pub engine_scope: Option<std::collections::BTreeSet<ContestId>>,
    roster: Vec<MemberNick>,
    bindings: BTreeMap<String, ExecutorBinding>,
    daily_max: i64,
    pub variable_defaults: squad_store::clear::Defaults,
    confirmation_threshold: f64,
    pub ephemeral: ephemeral::Ephemeral,
}
pub fn envelope(
    correlation: EventId,
    sender: Actor,
    recipients: Vec<Recipient>,
    content: EnvelopePayload,
) -> ProducerEnvelope {
    ProducerEnvelope {
        base: EnvelopeBase {
            version: 1,
            id: EventId::new(),
            room_id: MainRoom::Main,
            correlation_id: correlation,
            causation_id: Some(correlation),
            sender,
            recipients,
            created_at: chrono::Utc::now(),
        },
        content,
    }
}
pub fn error(code: ErrorCode, text: impl Into<String>) -> ErrorPayload {
    ErrorPayload {
        code,
        text: Text::new(text).expect("nonempty error"),
        retryable: false,
        related_event_id: None,
        work_id: None,
        debate_id: None,
        case_id: None,
        earliest_cursor: None,
        retry_action: None,
        contest_id: None,
    }
}
pub fn rejected(id: EventId, err: StoreError) -> RequestResult {
    let code = match &err {
        StoreError::Domain(code, _) => *code,
        _ => ErrorCode::INTERNAL_ERROR,
    };
    RequestResult::Rejected {
        request_id: id,
        error: error(code, err.to_string()),
    }
}
impl Router {
    pub fn new(
        store: Store,
        roster: Vec<MemberNick>,
        bindings: BTreeMap<String, ExecutorBinding>,
        daily_max: i64,
    ) -> Result<Self> {
        if roster.is_empty()
            || roster.len() > 12
            || roster
                .iter()
                .enumerate()
                .any(|(i, n)| roster[..i].contains(n))
            || daily_max < 0
        {
            return Err(StoreError::Domain(
                ErrorCode::CONFIG_INVALID,
                "Invalid router configuration",
            ));
        }
        Ok(Self {
            store,
            engine_config: None,
            engine_scope: None,
            roster,
            bindings,
            daily_max,
            variable_defaults: Default::default(),
            confirmation_threshold: constants::DEFAULT_DEBATE_CONFIRM_USD,
            ephemeral: Default::default(),
        })
    }
    pub fn parse_context(&self) -> Result<parser::ParseContext> {
        let roster = self.roster.clone();
        self.store.call(move |db| snapshot(db, roster))
    }
    pub fn roster(&self) -> &[MemberNick] {
        &self.roster
    }
    pub(crate) fn engine_bindings(&self) -> BTreeMap<String, ExecutorBinding> {
        self.bindings.clone()
    }
    pub fn process(
        &mut self,
        request: Request,
        owner: Option<ExecutorKey>,
    ) -> Result<CommitOutcome> {
        let human = owner.is_none();
        let original_id = request_id(&request);
        let mut outcome = self.process_request(request, owner)?;
        if human
            && !outcome.duplicate
            && let Some(scope) = &mut self.engine_scope
        {
            for event in &outcome.events {
                if let EnvelopePayload::CensusState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::DebateState(s) = &event.content {
                    scope.insert(s.debate_id);
                }
            }
        }
        let initial = outcome.events.len();
        let contests: std::collections::BTreeSet<_> = outcome
            .events
            .iter()
            .filter_map(|e| {
                if let EnvelopePayload::DebateState(s) = &e.content {
                    Some(s.debate_id)
                } else {
                    None
                }
            })
            .collect();
        match if outcome.events.is_empty() {
            Ok(Vec::new())
        } else {
            self.drive_contests()
        } {
            Ok(moves) => {
                for moved in moves {
                    if matches!(
                        moved.result,
                        RequestResult::PendingConfirmation { .. } | RequestResult::Rejected { .. }
                    ) && human && match &moved.result {
                        RequestResult::PendingConfirmation{preflight:PendingPreflight::Debate(p),..}=>contests.contains(&p.debate_id),
                        RequestResult::Rejected{error,..}=>error.debate_id.is_some_and(|id|contests.contains(&id)) || moved.events.iter().any(|e|matches!(&e.content,EnvelopePayload::DebateState(s) if contests.contains(&s.debate_id))),
                        _=>false,
                    }
                    {
                        outcome.result = moved.result;
                    }
                    outcome.events.extend(moved.events);
                }
            }
            Err(e) => tracing::error!(error=%e,"Contest engine failed to advance"),
        }
        if !outcome.duplicate
            && let RequestResult::Accepted {
                event_ids, cursor, ..
            } = &mut outcome.result
        {
            event_ids.extend(outcome.events[initial..].iter().map(|e| e.base.id));
            if let Some(last) = outcome.events.last() {
                *cursor = Some(last.commit.seq);
            }
        }
        if human && !outcome.duplicate && self.engine_config.is_some() {
            match &mut outcome.result {
                RequestResult::Accepted { request_id, .. }
                | RequestResult::PendingConfirmation { request_id, .. }
                | RequestResult::Rejected { request_id, .. } => *request_id = original_id,
                _ => {}
            }
            // Store the composed human receipt, including an automatic engine
            // preflight, so submission retries preserve its public result.
            let result = serde_json::to_string(&outcome.result)?;
            self.store.call(move |db| { db.execute("UPDATE receipts SET result_json=? WHERE request_id=? AND producer_identity='me'",rusqlite::params![result,original_id.to_string()])?; Ok(()) })?;
        }
        Ok(outcome)
    }
    fn process_request(
        &mut self,
        request: Request,
        owner: Option<ExecutorKey>,
    ) -> Result<CommitOutcome> {
        let trace_id = request_id(&request);
        tracing::debug!(request_id=%trace_id,"Router request received");
        if let Some(owner) = owner {
            if let Request::Internal(InternalRequest::Executor(request)) = request {
                let outcome = self.executor(request, owner)?;
                trace_outcome(trace_id, &outcome);
                return Ok(outcome);
            }
            return Err(authorization::forbidden());
        }
        if matches!(request, Request::Internal(_)) {
            return Err(authorization::forbidden());
        }
        if matches!(
            request,
            Request::Who | Request::Help | Request::Export { .. }
        ) {
            let text = match request {
                Request::Who => self
                    .roster
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                Request::Help => "Address a member with @nick, or use /stop, /clear, /set".into(),
                _ => {
                    return Err(StoreError::Domain(
                        ErrorCode::COMMAND_NOT_IMPLEMENTED,
                        "Use the consistent store export reader",
                    ));
                }
            };
            return Ok(CommitOutcome {
                result: RequestResult::Internal(InternalResult::Text { text }),
                events: Vec::new(),
                duplicate: false,
            });
        }
        let id = request_id(&request);
        let digest = squad_store::commit::digest(&request)?;
        let roster = self.roster.clone();
        let bindings = self.bindings.clone();
        let clear = matches!(request, Request::Clear { .. });
        let defaults = self.variable_defaults;
        let config = self.engine_config.clone();
        let max = self.daily_max;
        let threshold = self.confirmation_threshold;
        let outcome=self.store.commit(id,"me".into(),digest,move |tx,group| {
            match request {
                Request::UserMessage {input,parsed,preconfirmed}=> {
                    let meta=parser::SubmissionMeta {quote:input.quote.clone().unwrap_or_default(),reply_to:input.reply_to.clone().unwrap_or_default()};
                    let ctx=snapshot(tx,roster.clone())?;
                    let authoritative=match parse(input.text.as_str(),&meta,&ctx) {Ok(parsed)=>parsed,Err(e)=>return Ok(RequestResult::Rejected {request_id:id,error:error(e.code,e.message)})};
                    if authoritative!=parsed {return Err(StoreError::Domain(ErrorCode::INVALID_COMMAND,"Parsed input does not match authoritative state"));}
                    if matches!(parsed,ParsedInput::QuestionFirst {..}) {picks::collect(tx,group,id,input,parsed,&bindings)?;return Ok(group.accepted(id));}
                    if let ParsedInput::Debate {input:conversation,..}=&parsed {
                        variables::apply(tx,group,id,&conversation.variable_updates)?;
                        let request=Request::UserMessage {input,parsed,preconfirmed};
                        debates::prepare(tx,group,id,&request,&bindings)?;
                        return Ok(group.accepted(id));
                    }
                    if matches!(parsed,ParsedInput::Census {..}) {
                        return census::prepare(tx,group,id,input,parsed,preconfirmed,&bindings,config.as_ref().ok_or(StoreError::Domain(ErrorCode::NOT_READY,"Census requires runtime configuration"))?,max,threshold);
                    }
                    if let ParsedInput::Judge {input:conversation,question,..}=&parsed {
                        let binding=bindings.get("judge").ok_or(StoreError::Domain(ErrorCode::CONFIG_INVALID,"Judge binding is unavailable"))?;
                        let quotes=references::quotes(tx,&meta.quote)?;
                        variables::apply(tx,group,id,&conversation.variable_updates)?;
                        if !squad_store::cases::followup(tx,group,id,input.clone(),question.clone(),quotes.clone(),binding)? {cases::standalone(tx,group,id,input,question.clone(),quotes,binding)?;}
                        return Ok(group.accepted(id));
                    }
                    let ParsedInput::Message {input:conversation,recipients,body}=parsed else {return Err(StoreError::Domain(ErrorCode::COMMAND_NOT_IMPLEMENTED,"Contest admission is not implemented"));};
                    if !meta.reply_to.is_empty() {return Err(StoreError::Domain(ErrorCode::INVALID_REFERENCE,"replyTo is only valid for debate"));}
                    let recipients=authorization::recipients(&roster,&recipients)?;
                    let quotes=references::quotes(tx,&meta.quote)?;
                    variables::apply(tx,group,id,&conversation.variable_updates)?;
                    let content=references::content(&body,&quotes);
                    let event=ProducerEnvelope {base:EnvelopeBase {version:1,id,room_id:MainRoom::Main,correlation_id:id,causation_id:None,sender:Actor::Me,recipients:recipients.clone(),created_at:group.accepted_at},content:EnvelopePayload::UserMessage(UserMessage {input:input.clone(),body,resolved_quotes:quotes,debate_id:None,case_id:None,contest_id:None})};
                    let seq=group.push(tx,event)?;
                    let event=squad_store::events::at(tx,seq.get())?;
                    for recipient in recipients {
                        let Recipient::Member(nick)=recipient else {continue};
                        if !bindings.contains_key(nick.as_str()) {
                            let mut failure=error(ErrorCode::MODEL_UNAVAILABLE,format!("Model unavailable for @{nick}"));failure.related_event_id=Some(id);
                            group.push(tx,envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::Error(failure)))?;
                            continue;
                        }
                        let pending:i64=tx.query_row("SELECT COUNT(*) FROM work WHERE target=? AND status IN ('queued','running')",[nick.as_str()],|r|r.get(0))?;
                        if pending>=constants::AGENT_QUEUE_DEPTH as i64 {
                            let mut failure=error(ErrorCode::AGENT_QUEUE_FULL,format!("Agent queue for @{nick} is full"));failure.related_event_id=Some(id);
                            group.push(tx,envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::Error(failure)))?;
                            continue;
                        }
                        let held:Option<String>=tx.query_row("SELECT contest_id FROM contests WHERE active=1 AND json_extract(state_json,'$.state') IN ('WAITING_FOR_DEBATERS','ROUND_RUNNING','ROUND_COMMITTING','JUDGING') AND EXISTS(SELECT 1 FROM json_each(json_extract(state_json,'$.debaters')) WHERE value=?)",[nick.as_str()],|r|r.get(0)).optional()?;
                        let held=if held.is_some(){held}else{tx.query_row("SELECT c.contest_id FROM contests c JOIN census_runs r ON r.contest_id=c.contest_id WHERE c.active=1 AND json_extract(c.state_json,'$.status') NOT IN ('PREFLIGHT','WAITING_FOR_PLAN') AND EXISTS(SELECT 1 FROM json_each(json_extract(r.setup_json,'$.participants')) WHERE value=?)",[nick.as_str()],|r|r.get(0)).optional()?};
                        let held=held.map(|id|id.parse().map_err(|_|StoreError::Unavailable)).transpose()?;
                        let projection=squad_store::history::attributed(&event,ContextId(nick.to_string()),"ordinary.input",HistoryRole::User,None,content.clone());
                        if let Some(contest)=held {squad_store::history::defer(tx,contest,&projection)?;}else{squad_store::history::project(tx,&projection)?;}
                        let work=Work {work_id:WorkId::new(),message_id:MessageId::new(),in_reply_to_message_id:Some(input.message_id),kind:WorkKind::Reply,target:Target::Member(nick.clone()),context_id:ContextId(nick.to_string()),trigger_event_id:id,history_through:HistoryPosition::new(1).expect("positive"),attempt:1,debate_id:None,round:None,replay:None,case_id:None,prompt_ref:None,max_output_tokens:None,contest_id:None,step_id:None};
                        squad_store::work::insert(tx,&work,&bindings[nick.as_str()],held)?;
                    }
                }
                Request::DebateConfirm {debate_id,plan_id,..}|Request::ContestConfirm {contest_id:debate_id,plan_id,..}=>{
                    if squad_store::schema::validate_contest_kind(tx,debate_id,ContestKind::Census)? { return census::resolve(tx,group,id,debate_id,plan_id,false); }
                    return preflight::resolve(tx,group,id,debate_id,plan_id,false);
                },
                Request::DebateCancel {debate_id,plan_id,..}|Request::ContestCancel {contest_id:debate_id,plan_id,..}=>{
                    if squad_store::schema::validate_contest_kind(tx,debate_id,ContestKind::Census)? { return census::resolve(tx,group,id,debate_id,plan_id,true); }
                    return preflight::resolve(tx,group,id,debate_id,plan_id,true);
                },
                Request::ContestPick {contest_id,nicks,..}=>{
                    let ctx=snapshot(tx,roster.clone())?;
                    if ctx.pending_pick.as_ref().is_none_or(|p|p.contest_id!=contest_id) {return Err(StoreError::Domain(ErrorCode::CONTEST_NOT_FOUND,"Contest not found"));}
                    let raw=nicks.iter().map(|n|format!("@{n}")).collect::<Vec<_>>().join(" ");
                    if let Err(e)=parse(&raw,&parser::SubmissionMeta::default(),&ctx) {return Ok(RequestResult::Rejected {request_id:id,error:error(e.code,e.message)});}
                    picks::pick(tx,group,id,contest_id,nicks,bindings.get("judge").map(|b|b.model.to_string()).unwrap_or_default())?;
                },
                Request::Judge {debate_id,..}=>{
                    let debate_id=if let Some(id)=debate_id {id}else {
                        let raw:Option<String>=tx.query_row("SELECT contest_id FROM contests WHERE kind='debate' AND json_extract(state_json,'$.state') IN ('STOPPED','COMPLETE_UNJUDGED','FAILED') ORDER BY updated_seq DESC LIMIT 1",[],|r|r.get(0)).optional()?;
                        raw.ok_or(StoreError::Domain(ErrorCode::DEBATE_NOT_FOUND,"Debate not found"))?.parse().map_err(|_|StoreError::Unavailable)?
                    };
                    let state=squad_store::contests::debate(tx,debate_id)?;
                    debates::transition(tx,group,id,ContestTransitionSubmission {contest_id:debate_id,kind:ContestKind::Debate,expected_state_version:state.state_version,transition:ContestTransition::Debate(DebateTransition::JudgeOnlyRequested)})?;
                },
                Request::CensusContinue {contest_id,expected_state_version,plan_event_id,replacement_text,..}=>census::continue_plan(tx,group,id,contest_id,expected_state_version,plan_event_id,replacement_text)?,
                Request::SetVars {updates,..}=>variables::apply(tx,group,id,&updates)?,
                Request::Stop {target,..}=>stop::apply(tx,group,id,target)?,
                Request::Retry {work_id,..}=>retry::apply(tx,group,id,work_id)?,
                Request::Regen {message_id,..}=>regen::apply(tx,group,id,message_id)?,
                Request::Clear {..}=>clear::apply(tx,group,id,defaults)?,

                _=>return Err(StoreError::Domain(ErrorCode::COMMAND_NOT_IMPLEMENTED,"Operation is not implemented")),
            }
            Ok(group.accepted(id))
        })?;
        if clear && !outcome.duplicate {
            self.ephemeral = Default::default();
            if self.store.vacuum_after_clear().is_err() {
                tracing::warn!("Clear committed; deferred VACUUM remains pending");
            }
        }
        trace_outcome(trace_id, &outcome);
        Ok(outcome)
    }
    fn executor(&mut self, request: ExecutorRequest, owner: ExecutorKey) -> Result<CommitOutcome> {
        let max = self.daily_max;
        match request {
            ExecutorRequest::Claim { work_id } => {
                let result = self.store.call(move |db| {
                    let tx =
                        db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                    let grant =
                        squad_store::leases::claim(&tx, work_id, &owner, chrono::Utc::now(), None)?;
                    tx.commit()?;
                    Ok(RequestResult::Internal(
                        grant.map_or(InternalResult::NoWork, InternalResult::Claimed),
                    ))
                })?;
                Ok(CommitOutcome {
                    result,
                    events: Vec::new(),
                    duplicate: false,
                })
            }
            ExecutorRequest::StartAttempt {
                work_id,
                attempt,
                fence,
            } => executor::start(&self.store, owner, work_id, attempt, fence, max),
            ExecutorRequest::Account { epoch, record, .. } => {
                executor::account(&self.store, owner, epoch, record, max)
            }
            ExecutorRequest::Failed {
                work_id,
                attempt,
                fence,
                epoch,
                error,
                repair,
            } => executor::failed(
                &self.store,
                owner,
                work_id,
                attempt,
                fence,
                epoch,
                error,
                repair,
            ),
            ExecutorRequest::RenewLease { work_id, fence } => {
                let result = self.store.call(move |db| {
                    let tx =
                        db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                    let record =
                        squad_store::work::record(&tx, work_id)?.ok_or(StoreError::Unavailable)?;
                    let epoch: i64 =
                        tx.query_row("SELECT epoch FROM room_meta", [], |r| r.get(0))?;
                    squad_store::work::check_fence(
                        &tx,
                        work_id,
                        &owner,
                        record.attempt,
                        fence,
                        epoch as u64,
                        chrono::Utc::now(),
                    )?;
                    let lease = chrono::Utc::now()
                        + chrono::Duration::milliseconds(constants::WORK_LEASE_MS as i64);
                    tx.execute(
                        "UPDATE work SET lease_until=? WHERE work_id=?",
                        params![lease.to_rfc3339(), work_id.to_string()],
                    )?;
                    tx.commit()?;
                    Ok(RequestResult::Internal(InternalResult::LeaseRenewed {
                        work_id,
                        fence,
                        lease_until: lease,
                    }))
                })?;
                Ok(CommitOutcome {
                    result,
                    events: Vec::new(),
                    duplicate: false,
                })
            }
            ExecutorRequest::Result {
                work_id,
                attempt,
                fence,
                epoch,
                mut output,
            } => {
                // Use stable work-attempt result identity, independent of mutable source resolution.
                let id = work_id
                    .to_string()
                    .parse::<EventId>()
                    .expect("same UUID version");
                let digest =
                    squad_store::commit::digest(&(work_id, attempt, fence, epoch, &output))?;
                let producer = serde_json::to_string(&owner).map_err(StoreError::from)?;
                self.store.commit(id, producer, digest, move |tx, group| {
                    let work = squad_store::work::check_fence(
                        tx,
                        work_id,
                        &owner,
                        attempt,
                        fence,
                        epoch,
                        group.accepted_at,
                    )?;
                    authorization::output(&work, &owner, &output)?;
                    squad_store::cases::validate_invocation(tx,&work,&output)?;
                    if let EnvelopePayload::AgentReply(reply) = output {
                        output = EnvelopePayload::MessageFinal(reply);
                    }
                    if let EnvelopePayload::DebateRoundScore(score) = &mut output {
                        squad_store::round_scores::canonicalize(tx, score)?;
                    }
                    if let EnvelopePayload::Verdict(verdict) = &mut output {
                        verdict::validate(tx, verdict)?;
                    }
                    let sender = match &owner {
                        ExecutorKey::Member(nick) => Actor::Member(nick.clone()),
                        ExecutorKey::Case(_) => Actor::Judge,
                    };
                    let mut event = envelope(
                        work.trigger_event_id,
                        sender,
                        vec![Recipient::Me],
                        output.clone(),
                    );
                    event.base.id = id;
                    let seq = group.push(tx, event)?;
                    let event = squad_store::events::at(tx, seq.get())?;
                    match &output {
                        EnvelopePayload::MessageFinal(reply) => {
                            squad_store::history::project(
                                tx,
                                &squad_store::history::attributed(
                                    &event,
                                    work.context_id.clone(),
                                    "ordinary.completion",
                                    HistoryRole::Assistant,
                                    Some(work.target.to_string()),
                                    reply.content.clone(),
                                ),
                            )?;
                            squad_store::work::finish(tx, work_id, WorkStatus::Completed, seq)?;
                            if let Some(step)=work.step_id {squad_store::contest_steps::settle(tx,step,&work.target,None)?;}
                            picks::refresh(tx,group,id)?;
                        }
                        EnvelopePayload::DebateTurn(turn) => {
                            let state=squad_store::contests::debate(tx,turn.debate_id)?;
                            if state.state!=DebateStatus::RoundRunning || state.round!=turn.round {return Err(debates::invalid());}
                            squad_store::contests::associate(
                                tx,
                                turn.debate_id,
                                ContestKind::Debate,
                                seq,
                            )?;
                            squad_store::work::finish(tx, work_id, WorkStatus::Completed, seq)?;
                        }
                        EnvelopePayload::DebateRoundScore(score) => {
                            squad_store::round_scores::insert(tx, &work, score, seq)?;
                            squad_store::history::project(
                                tx,
                                &squad_store::history::attributed(
                                    &event,
                                    work.context_id.clone(),
                                    "round.score",
                                    HistoryRole::Assistant,
                                    Some("judge".into()),
                                    serde_json::to_string(score)?,
                                ),
                            )?;
                        }
                        EnvelopePayload::Verdict(verdict) => {
                            verdict::seal(tx, verdict, seq)?;
                            verdict::announce(tx,group,verdict,&event)?;
                            squad_store::work::finish(tx, work_id, WorkStatus::Completed, seq)?;
                        }
                        EnvelopePayload::CaseRuling(ruling) => {
                            squad_store::cases::accept_ruling(tx, &work, seq)?;
                            squad_store::history::project(
                                tx,
                                &squad_store::history::attributed(
                                    &event,
                                    work.context_id.clone(),
                                    "case.ruling",
                                    HistoryRole::Assistant,
                                    Some("judge".into()),
                                    ruling.content.to_string(),
                                ),
                            )?;
                            squad_store::work::finish(tx, work_id, WorkStatus::Completed, seq)?;
                        }
                        EnvelopePayload::ContestArtifact(artifact) if work.kind.census_composition().is_some()=>squad_store::census::accept(tx,&work,artifact,seq)?,
                        EnvelopePayload::ContestArtifact(artifact)=>squad_store::contest_steps::artifact(tx,artifact,seq)?,
                        _ => return Err(authorization::forbidden()),
                    }
                    let status=if matches!(&output,EnvelopePayload::DebateRoundScore(score) if score.scores.is_none()){WorkStatus::Failed}else{WorkStatus::Completed};
                    group.push(tx,envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::WorkState(WorkStateEvent {work_id,trigger_event_id:work.trigger_event_id,target:work.target.clone(),context_id:work.context_id.clone(),attempt,status,error:None,debate_id:work.debate_id,case_id:work.case_id,contest_id:work.contest_id})))?;
                    let usage=executor::stored_usage(tx,settle_usage(tx,&work,&output)?)?;
                    group.push(tx,envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::UsageRecord(usage)))?;
                    let day: String = tx.query_row(
                        "SELECT day FROM attempts WHERE work_id=? AND attempt=?",
                        params![work_id.to_string(), attempt],
                        |r| r.get(0),
                    )?;
                    group.push(
                        tx,
                        envelope(
                            id,
                            Actor::System,
                            vec![Recipient::Me],
                            EnvelopePayload::BudgetState(squad_store::accounting::budget(
                                tx, &day, max,
                            )?),
                        ),
                    )?;
                    Ok(group.accepted(id))
                })
            }
            ExecutorRequest::Delta {
                fence,
                epoch,
                delta,
            } => {
                let work_id = delta.generation.work_id;
                let attempt = delta.generation.attempt;
                let owned = owner.clone();
                let generation = delta.generation.clone();
                let scope = (
                    delta.debate_id,
                    delta.round,
                    delta.case_id,
                    delta.contest_id,
                    delta.step_id,
                );
                self.store.call(move |db| {
                    let work = squad_store::work::check_fence(
                        db,
                        work_id,
                        &owned,
                        attempt,
                        fence,
                        epoch,
                        chrono::Utc::now(),
                    )?;
                    if work.message_id != generation.message_id
                        || work.trigger_event_id != generation.trigger_event_id
                        || work.debate_id != scope.0
                        || work.round != scope.1
                        || work.case_id != scope.2
                        || work.contest_id != scope.3
                        || work.step_id != scope.4
                    {
                        return Err(authorization::forbidden());
                    }
                    Ok(())
                })?;
                if !self.ephemeral.chunk(&delta) {
                    return Ok(empty());
                }
                Ok(CommitOutcome {
                    result: RequestResult::Accepted {
                        request_id: delta.generation.trigger_event_id,
                        event_ids: Vec::new(),
                        cursor: None,
                    },
                    events: Vec::new(),
                    duplicate: false,
                })
            }
            ExecutorRequest::Presence(presence) => {
                if owner != ExecutorKey::Member(presence.nick.clone()) {
                    return Err(authorization::forbidden());
                }
                if let Some(work_id) = presence.work_id {
                    let key = owner.clone();
                    self.store.call(move |db| {
                        let (work, _) = squad_store::work::template(db, work_id)?
                            .ok_or_else(authorization::forbidden)?;
                        if !key.owns(&work) {
                            return Err(authorization::forbidden());
                        }
                        Ok(())
                    })?;
                }
                let id = EventId::new();
                let sender = Actor::Member(presence.nick.clone());
                envelope(
                    id,
                    sender,
                    vec![Recipient::Me],
                    EnvelopePayload::Presence(presence.clone()),
                )
                .validate()
                .map_err(|_| authorization::forbidden())?;
                self.ephemeral
                    .presence
                    .insert(presence.nick.to_string(), presence);
                Ok(CommitOutcome {
                    result: RequestResult::Accepted {
                        request_id: id,
                        event_ids: Vec::new(),
                        cursor: None,
                    },
                    events: Vec::new(),
                    duplicate: false,
                })
            }
        }
    }
}
fn empty() -> CommitOutcome {
    CommitOutcome {
        result: RequestResult::Internal(InternalResult::NoWork),
        events: Vec::new(),
        duplicate: false,
    }
}
fn settle_usage(
    tx: &rusqlite::Connection,
    work: &Work,
    output: &EnvelopePayload,
) -> Result<UsageRecord> {
    let usage = match output {
        EnvelopePayload::MessageFinal(reply) => reply.completed.usage.clone(),
        EnvelopePayload::Verdict(verdict) => verdict.completed.usage.clone(),
        EnvelopePayload::CaseRuling(ruling) => ruling.completed.usage.clone(),
        EnvelopePayload::DebateTurn(DebateTurn {
            outcome: DebateTurnOutcome::Completed { usage, .. },
            ..
        }) => usage.clone(),
        _ => None,
    };
    let row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT model,day,started_at FROM attempts WHERE work_id=? AND attempt=?",
            params![work.work_id.to_string(), work.attempt],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((model, day, started)) = row else {
        return Err(StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Result requires a durable provider start",
        ));
    };
    let record = UsageRecord {
        work_id: work.work_id,
        attempt: work.attempt,
        target: work.target.clone(),
        context_id: work.context_id.clone(),
        provider_request_id: None,
        model: Text::new(model).map_err(|_| StoreError::Unavailable)?,
        started_at: started.parse().map_err(|_| StoreError::Unavailable)?,
        ended_at: Some(chrono::Utc::now()),
        outcome: UsageOutcome::Completed,
        usage,
        estimated_cost_usd: None,
        debate_id: work.debate_id,
        case_id: work.case_id,
        contest_id: work.contest_id,
        response_model: None,
        day,
    };
    squad_store::accounting::reconcile(tx, &record)?;
    Ok(record)
}
/// Log canonical identities after commit, without conversational bodies or metadata.
fn trace_outcome(request_id: EventId, outcome: &CommitOutcome) {
    for event in &outcome.events {
        let work_id = match &event.content {
            EnvelopePayload::WorkState(w) => Some(w.work_id),
            EnvelopePayload::UsageRecord(w) => Some(w.work_id),
            EnvelopePayload::MessageFinal(w) | EnvelopePayload::AgentReply(w) => {
                Some(w.completed.generation.work_id)
            }
            _ => None,
        };
        tracing::debug!(%request_id,event_id=%event.base.id,correlation_id=%event.base.correlation_id,causation_id=?event.base.causation_id,?work_id,cursor=%event.commit.seq.get(),"Router commit accepted");
    }
    if let RequestResult::Internal(
        InternalResult::Claimed(grant) | InternalResult::Started(grant),
    ) = &outcome.result
    {
        tracing::debug!(%request_id,work_id=%grant.work.work_id,correlation_id=%grant.work.trigger_event_id,causation_id=%grant.work.trigger_event_id,attempt=grant.work.attempt,fence=grant.fence,epoch=grant.epoch,"Executor grant acknowledged");
    }
}

pub fn request_id(request: &Request) -> EventId {
    match request {
        Request::UserMessage { input, .. } => input
            .message_id
            .to_string()
            .parse()
            .expect("same UUID version"),
        Request::Stop { request_id, .. }
        | Request::Regen { request_id, .. }
        | Request::DebateConfirm { request_id, .. }
        | Request::DebateCancel { request_id, .. }
        | Request::Judge { request_id, .. }
        | Request::SetVars { request_id, .. }
        | Request::Clear { request_id }
        | Request::Retry { request_id, .. }
        | Request::ContestPick { request_id, .. }
        | Request::ContestConfirm { request_id, .. }
        | Request::ContestCancel { request_id, .. }
        | Request::CensusContinue { request_id, .. }
        | Request::CrosscheckAccept { request_id, .. } => *request_id,
        Request::Internal(InternalRequest::Executor(
            ExecutorRequest::Claim { work_id }
            | ExecutorRequest::StartAttempt { work_id, .. }
            | ExecutorRequest::RenewLease { work_id, .. }
            | ExecutorRequest::Result { work_id, .. },
        )) => work_id.to_string().parse().expect("same UUID version"),
        Request::Internal(InternalRequest::Executor(ExecutorRequest::Delta { delta, .. })) => {
            delta.generation.trigger_event_id
        }
        _ => EventId::new(),
    }
}
fn snapshot(db: &rusqlite::Connection, roster: Vec<MemberNick>) -> Result<parser::ParseContext> {
    let mut ctx = parser::ParseContext {
        roster,
        ..Default::default()
    };
    let mut query = db.prepare("SELECT name,value FROM vars")?;
    for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u8>(1)?)))? {
        let (name, value) = row?;
        if let Some(name) = VariableName::from_name(&name) {
            ctx.vars.insert(name, value);
        }
    }
    for event in squad_store::events::page(db, None, usize::MAX)? {
        let kind = serde_json::to_value(&event.content)?["kind"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let (message_id, content, reply) = match &event.content {
            EnvelopePayload::UserMessage(m) => (Some(m.input.message_id), m.body.clone(), None),
            EnvelopePayload::MessageFinal(m) => (
                Some(m.completed.generation.message_id),
                m.content.clone(),
                Some(m.in_reply_to_message_id),
            ),
            _ => (None, String::new(), None),
        };
        let superseded: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM superseded WHERE superseded_event_id=?)",
            [event.base.id.to_string()],
            |r| r.get(0),
        )?;
        ctx.records.push(parser::RecordSummary {
            event_id: event.base.id,
            message_id,
            seq: event.commit.seq,
            kind: Text::new(kind).map_err(|_| StoreError::Unavailable)?,
            author: event.base.sender,
            recipients: event.base.recipients,
            content,
            in_reply_to_message_id: reply,
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: Vec::new(),
            state_version: None,
            superseded,
            eligible: !superseded,
        });
    }
    if let Some(raw) = db
        .query_row("SELECT state_json FROM contests WHERE active=1", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
    {
        if let Ok(state) = serde_json::from_str::<ContestSelectionState>(&raw) {
            if state.state == ContestSelectionStatus::AwaitingPick {
                ctx.pending_pick = Some(parser::PendingPick {
                    contest_id: state.contest_id,
                    mode: parser::QuestionFirstMode::Debate,
                    eligible: state.answers.iter().map(|a| a.nick.clone()).collect(),
                });
            }
            ctx.active_contest = Some(ContestState::Selection(state));
        } else if let Ok(state) = serde_json::from_str::<CensusState>(&raw) {
            ctx.active_contest = Some(ContestState::Census(state));
        } else {
            ctx.active_contest = Some(ContestState::Debate(serde_json::from_str(&raw)?));
        }
    }
    Ok(ctx)
}
