//! Serialized authority for all room mutations. Executors receive scoped ingress only.
pub mod accounting;
pub mod authorization;
pub mod bounds;
pub mod cases;
pub mod census;
pub mod clear;
pub mod contests;
pub(crate) mod crosscheck;
pub mod debates;
pub mod ephemeral;
pub mod executor;
pub mod grill;
pub mod picks;
pub mod preflight;
pub mod probe;
pub mod recovery;
pub mod references;
pub mod regen;
pub mod retry;
pub mod stop;
pub mod survey;
pub mod tournaments;
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
    pub(crate) daily_max: i64,
    pub variable_defaults: squad_store::clear::Defaults,
    confirmation_threshold: f64,
    pub ephemeral: ephemeral::Ephemeral,
    pub(crate) grill_active: bool,
    storage_capacity: u64,
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
        provider_status: None,
        provider_message: None,
    }
}
/// Text for a daily-cap refusal that names what was spent, the cap and the
/// reservation the request would have needed (all in micro-dollars).
pub fn daily_cap_text(spent_micros: i64, max_micros: i64, required_micros: i64) -> String {
    squad_store::accounting::daily_max_text(max_micros, Some((spent_micros, required_micros)))
}
/// Build the admission rejection for `required_micros`, reading today's spend.
pub fn daily_cap_error(
    db: &rusqlite::Connection,
    day: &str,
    required_micros: i64,
    max_micros: i64,
) -> ErrorPayload {
    let spent = squad_store::accounting::daily(db, day)
        .map(|spend| spend.spent_micros)
        .unwrap_or_default();
    error(
        ErrorCode::DAILY_MAX_REACHED,
        daily_cap_text(spent, max_micros, required_micros),
    )
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
        let grill_active = store.call(|db| {
            Ok(db.query_row(
                "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1 AND kind='grill')",
                [],
                |r| r.get(0),
            )?)
        })?;
        Ok(Self {
            grill_active,
            store,
            engine_config: None,
            engine_scope: None,
            roster,
            bindings,
            daily_max,
            variable_defaults: Default::default(),
            confirmation_threshold: constants::DEFAULT_DEBATE_CONFIRM_USD,
            ephemeral: Default::default(),
            storage_capacity: squad_store::storage::DEFAULT_CAPACITY,
        })
    }
    /// Override the durable-storage ceiling (tests use a small value).
    pub fn with_storage_capacity(mut self, bytes: u64) -> Self {
        self.storage_capacity = bytes;
        self
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
        let grill_tick = self.grill_active
            && matches!(
                &request,
                Request::Internal(InternalRequest::Executor(ExecutorRequest::Presence(_)))
            );
        let human = owner.is_none();
        let original_id = request_id(&request);
        let mut outcome = self.process_request(request, owner)?;
        if human
            && !outcome.duplicate
            && let Some(scope) = &mut self.engine_scope
        {
            for event in &outcome.events {
                if let EnvelopePayload::ContestHumanAnswered(answer) = &event.content {
                    scope.insert(answer.contest_id);
                }
                if let EnvelopePayload::TuneState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::BassAckwardsPrdState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::ForgeState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::ForecastState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::DecideState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::NegotiateState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::BlowItUpState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::ReadbackState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::DebuggingState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::TraceState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::RankState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::CensusState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::DebateState(s) = &event.content {
                    scope.insert(s.debate_id);
                } else if let EnvelopePayload::TournamentState(s) = &event.content {
                    scope.insert(s.contest_id);
                } else if let EnvelopePayload::CrosscheckState(s) = &event.content {
                    scope.insert(s.contest_id);
                } else if let EnvelopePayload::SurveyState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::GrillState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
                if let EnvelopePayload::ProbeState(s) = &event.content {
                    scope.insert(s.contest_id);
                }
            }
        }
        for event in &outcome.events {
            if let EnvelopePayload::GrillState(s) = &event.content {
                self.grill_active = !s.state.is_terminal();
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
            if grill_tick {
                self.drive_grills()
            } else {
                Ok(Vec::new())
            }
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
        for event in &outcome.events {
            if let EnvelopePayload::GrillState(s) = &event.content {
                self.grill_active = !s.state.is_terminal();
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
        let engine_config = self.engine_config.clone();
        let self_threshold = self.confirmation_threshold;
        let clear = matches!(request, Request::Clear { .. });
        let storage_capacity = self.storage_capacity;
        let generates = admits_generation(&request);
        let defaults = self.variable_defaults;
        let tournament_max = self.daily_max;
        let tournament_threshold = self.confirmation_threshold;
        let tournament_config = self.engine_config.clone();
        let config = self.engine_config.clone();
        let max = self.daily_max;
        let threshold = self.confirmation_threshold;
        let survey_config = self.engine_config.clone();
        let survey_max = self.daily_max;
        let survey_threshold = self.confirmation_threshold;
        let outcome=self.store.commit(id,"me".into(),digest,move |tx,group| {
            if generates && let Some((code,text))=storage_refusal(squad_store::storage::ensure_capacity(tx,storage_capacity)) {
                return Ok(RequestResult::Rejected {request_id:id,error:error(code,text)});
            }
            match request {
                Request::HumanAnswer {request_id,contest_id,expected_state_version,text} => {
                    let pending=squad_store::contest_steps::pending_human(tx)?;
                    if pending.as_ref().is_none_or(|p|p.contest_id!=contest_id || p.state_version!=expected_state_version) {
                        return Ok(RequestResult::Rejected{request_id,error:error(ErrorCode::INVALID_STATE_TRANSITION,"Human step version changed")});
                    }
                    if let Some(answer)=&text {
                        let ctx=parser::ParseContext{pending_human:pending,..Default::default()};
                        if let Err(e)=parse(&format!("/answer {answer}"),&Default::default(),&ctx) {return Ok(RequestResult::Rejected{request_id,error:error(e.code,e.message)});}
                    }
                    human::answer(tx,group,request_id,contest_id,expected_state_version,text)?; return Ok(group.accepted(request_id))
                },
                Request::UserMessage {input,parsed,preconfirmed}=> {
                    let meta=parser::SubmissionMeta {quote:input.quote.clone().unwrap_or_default(),reply_to:input.reply_to.clone().unwrap_or_default()};
                    let ctx=snapshot(tx,roster.clone())?;
                    let authoritative=match parse(input.text.as_str(),&meta,&ctx) {Ok(parsed)=>parsed,Err(e)=>return Ok(RequestResult::Rejected {request_id:id,error:error(e.code,e.message)})};
                    if authoritative!=parsed {return Err(StoreError::Domain(ErrorCode::INVALID_COMMAND,"Parsed input does not match authoritative state"));}
                    if let Some(kind)=new_mode_parsed_kind(&parsed) { return preflight::prepare_new_mode(kind,tx,group,id,&input,&parsed,&NewModeAdmission{bindings:&bindings,config:engine_config.as_ref(),daily_max:max,confirmation_threshold:threshold,preconfirmed}); }
                    if matches!(parsed,ParsedInput::Survey {..}) {return survey::admit(tx,group,id,input,parsed,&bindings,survey_max,survey_threshold,preconfirmed,survey_config.as_ref());}
                    if matches!(parsed,ParsedInput::QuestionFirst {..}) {picks::collect(tx,group,id,input,parsed,&bindings)?;return Ok(group.accepted(id));}
                    if let ParsedInput::Tournament { input: conversation, entrants, prompt } = &parsed {
                        return tournaments::prepare(tx,group,id,input,conversation,entrants,prompt,&bindings,tournament_max,tournament_threshold,tournament_config.as_ref(),preconfirmed);
                    }
                    if matches!(parsed,ParsedInput::Crosscheck{..}) { return crosscheck::prepare(tx,group,id,input,parsed,&bindings,max,self_threshold,preconfirmed,engine_config.as_ref()); }
                    if let ParsedInput::Grill {input:conversation,leader,grillers,rounds,focus,plan_selection}=&parsed {
                        variables::apply(tx,group,id,&conversation.variable_updates)?;
                        let source=if let Some(source)=plan_selection{squad_store::events::by_id(tx,*source)?}else{
                            let seq:Option<i64>=tx.query_row("SELECT seq FROM events WHERE kind='message.final' AND sender=? AND NOT EXISTS(SELECT 1 FROM superseded s WHERE s.superseded_event_id=events.id) ORDER BY seq DESC LIMIT 1",[leader.as_str()],|r|r.get(0)).optional()?;
                            seq.map(|s|squad_store::events::at(tx,s)).transpose()?
                        }.ok_or(StoreError::Domain(ErrorCode::INVALID_REFERENCE,"No plan to grill"))?;
                        let EnvelopePayload::MessageFinal(reply)=source.content else{return Err(authorization::forbidden());};
                        let setup=GrillSetup{contest_id:ContestId::new(),leader:leader.clone(),grillers:grillers.clone(),source_message_id:reply.completed.generation.message_id,initial_plan:reply.content,focus:focus.clone(),total_rounds:u32::from(*rounds)};
                        return grill::prepare(tx,group,id,input,setup,&bindings,max,threshold,preconfirmed,engine_config.as_ref().ok_or(StoreError::Unavailable)?,false);
                    }
                    if let ParsedInput::Probe {input:conversation,participants,brief}=&parsed {
                        variables::apply(tx,group,id,&conversation.variable_updates)?;
                        return probe::prepare(tx,group,id,input,participants.clone(),brief.clone(),&bindings,max,threshold,preconfirmed,engine_config.as_ref());
                    }
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
                        let held:Option<String>=tx.query_row("SELECT contest_id FROM contests WHERE active=1 AND json_extract(state_json,'$.state') IN ('WAITING_FOR_DEBATERS','ROUND_RUNNING','ROUND_COMMITTING','JUDGING','WAITING_FOR_ENTRANTS','ANSWERING','ELIMINATING') AND EXISTS(SELECT 1 FROM json_each(COALESCE(json_extract(state_json,'$.debaters'),json_extract(state_json,'$.survivors'))) WHERE value=?)",[nick.as_str()],|r|r.get(0)).optional()?;
                        let held=if held.is_some(){held}else{tx.query_row("SELECT c.contest_id FROM contests c JOIN census_runs r ON r.contest_id=c.contest_id WHERE c.active=1 AND json_extract(c.state_json,'$.status') NOT IN ('PREFLIGHT','WAITING_FOR_PLAN') AND EXISTS(SELECT 1 FROM json_each(json_extract(r.setup_json,'$.participants')) WHERE value=?)",[nick.as_str()],|r|r.get(0)).optional()?};
                        let held=if held.is_some(){held}else{tx.query_row("SELECT contest_id FROM contests WHERE active=1 AND kind='survey' AND json_extract(state_json,'$.state')<>'PREFLIGHT' AND EXISTS(SELECT 1 FROM survey_sources s,json_each(s.setup_json,'$.participants') p WHERE s.contest_id=contests.contest_id AND p.value=?)",[nick.as_str()],|r|r.get(0)).optional()?};
                        let held=if held.is_some(){held}else{tx.query_row("SELECT contest_id FROM contests WHERE active=1 AND kind='probe' AND json_extract(state_json,'$.state')<>'PREFLIGHT' AND EXISTS(SELECT 1 FROM probe_roles WHERE probe_roles.contest_id=contests.contest_id AND nick=? AND role<>'sitting_out')",[nick.as_str()],|r|r.get(0)).optional()?};
                        let held=if held.is_some(){held}else{tx.query_row("SELECT c.contest_id FROM contests c JOIN contest_private_maps p ON p.contest_id=c.contest_id AND p.scope_key='grill.settings' WHERE c.active=1 AND c.kind='grill' AND json_extract(c.state_json,'$.state')<>'PREFLIGHT' AND (json_extract(p.map_json,'$.setup.leader')=? OR EXISTS(SELECT 1 FROM json_each(p.map_json,'$.setup.grillers') WHERE value=?))",params![nick.as_str(),nick.as_str()],|r|r.get(0)).optional()?};
                        let held=if held.is_some(){held}else{human::held_contest(tx,&nick)?.map(|id|id.to_string())};
                        let held=held.map(|id|id.parse().map_err(|_|StoreError::Unavailable)).transpose()?;
                        let projection=squad_store::history::attributed(&event,ContextId(nick.to_string()),"ordinary.input",HistoryRole::User,None,content.clone());
                        if let Some(contest)=held {squad_store::history::defer(tx,contest,&projection)?;}else{squad_store::history::project(tx,&projection)?;}
                        let work=Work {work_id:WorkId::new(),message_id:MessageId::new(),in_reply_to_message_id:Some(input.message_id),kind:WorkKind::Reply,target:Target::Member(nick.clone()),context_id:ContextId(nick.to_string()),trigger_event_id:id,history_through:HistoryPosition::new(1).expect("positive"),attempt:1,debate_id:None,round:None,replay:None,case_id:None,prompt_ref:None,max_output_tokens:None,contest_id:None,step_id:None};
                        squad_store::work::insert(tx,&work,&bindings[nick.as_str()],held)?;
                    }
                }
                Request::ContestConfirm {contest_id,plan_id,..} if squad_store::schema::validate_contest_kind(tx,contest_id,ContestKind::Grill)? => return grill::resolve(tx,group,id,contest_id,plan_id,false,max),
                Request::ContestCancel {contest_id,plan_id,..} if squad_store::schema::validate_contest_kind(tx,contest_id,ContestKind::Grill)? => return grill::resolve(tx,group,id,contest_id,plan_id,true,max),
                Request::ContestConfirm {contest_id,plan_id,..} if squad_store::schema::validate_contest_kind(tx,contest_id,ContestKind::Probe)? => return probe::resolve(tx,group,id,contest_id,plan_id,false,max),
                Request::ContestCancel {contest_id,plan_id,..} if squad_store::schema::validate_contest_kind(tx,contest_id,ContestKind::Probe)? => return probe::resolve(tx,group,id,contest_id,plan_id,true,max),
                Request::ContestExpire {contest_id,plan_id,..} => return tournaments::expire(tx,group,id,contest_id,plan_id),
                Request::ContestStop {contest_id,..}=>stop::tournament(tx,group,id,contest_id)?,
                Request::CrosscheckAccept {contest_id,expected_state_version,plan_event_id,grid_event_id,cell_ids,reason,..}=>crosscheck::accept(tx,group,id,contest_id,expected_state_version,plan_event_id,grid_event_id,cell_ids,reason)?,
                Request::DebateConfirm {debate_id,plan_id,..}|Request::ContestConfirm {contest_id:debate_id,plan_id,..}=>{
                    if squad_store::schema::validate_contest_kind(tx,debate_id,ContestKind::Tournament)? { return tournaments::resolve(tx,group,id,debate_id,plan_id,false,max); }
                    if survey::is_survey(tx,debate_id)? { return survey::resolve(tx,group,id,debate_id,plan_id,false); }
                    if squad_store::schema::validate_contest_kind(tx,debate_id,ContestKind::Census)? { return census::resolve(tx,group,id,debate_id,plan_id,false); }
                    return preflight::resolve(tx,group,id,debate_id,plan_id,false);
                },
                Request::DebateCancel {debate_id,plan_id,..}|Request::ContestCancel {contest_id:debate_id,plan_id,..}=>{
                    if squad_store::schema::validate_contest_kind(tx,debate_id,ContestKind::Tournament)? { return tournaments::resolve(tx,group,id,debate_id,plan_id,true,max); }
                    if survey::is_survey(tx,debate_id)? { return survey::resolve(tx,group,id,debate_id,plan_id,true); }
                    if squad_store::schema::validate_contest_kind(tx,debate_id,ContestKind::Census)? { return census::resolve(tx,group,id,debate_id,plan_id,true); }
                    return preflight::resolve(tx,group,id,debate_id,plan_id,true);
                },
                Request::ContestPick {contest_id,nicks,..}=>{
                    let ctx=snapshot(tx,roster.clone())?;
                    if ctx.pending_pick.as_ref().is_none_or(|p|p.contest_id!=contest_id) {return Err(StoreError::Domain(ErrorCode::CONTEST_NOT_FOUND,"Contest not found"));}
                    let raw=nicks.iter().map(|n|format!("@{n}")).collect::<Vec<_>>().join(" ");
                    if let Err(e)=parse(&raw,&parser::SubmissionMeta::default(),&ctx) {return Ok(RequestResult::Rejected {request_id:id,error:error(e.code,e.message)});}
                    if squad_store::schema::validate_contest_kind(tx,contest_id,ContestKind::Grill)? {return grill::pick(tx,group,id,contest_id,nicks,&bindings,max,threshold,engine_config.as_ref().ok_or(StoreError::Unavailable)?);}
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
                Request::CensusContinue {contest_id,expected_state_version,plan_event_id,replacement_text,..}=>return census::continue_plan(tx,group,id,contest_id,expected_state_version,plan_event_id,replacement_text),
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
                input_tokens,
            } => executor::start(
                &self.store,
                owner,
                work_id,
                attempt,
                fence,
                input_tokens,
                max,
            ),
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
                let tournament_pack = crate::prompts::PromptPack::from_override_dir(
                    self.engine_config
                        .as_ref()
                        .and_then(|c| c.prompts.directory.as_deref()),
                )
                .map_err(|_| StoreError::Unavailable)?;
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
                    if work.kind == WorkKind::JudgeTournament || (work.kind==WorkKind::Reply && work.round.is_some()) { squad_store::tournaments::validate_execution(tx,&work)?; } else { squad_store::cases::validate_invocation(tx,&work,&output)?; }
                    if let EnvelopePayload::TournamentJudgment(j) = &output {
                        tournaments::accept(tx,group,id,&work,j,&tournament_pack)?;
                        group.push(tx,envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::WorkState(WorkStateEvent{work_id,trigger_event_id:work.trigger_event_id,target:work.target.clone(),context_id:work.context_id.clone(),attempt,status:WorkStatus::Completed,error:None,debate_id:None,case_id:work.case_id,contest_id:work.contest_id})))?;
                        let usage=executor::stored_usage(tx,settle_usage(tx,&work,&output)?)?;
                        group.push(tx,envelope(id,Actor::System,vec![Recipient::Me],EnvelopePayload::UsageRecord(usage)))?;
                        return Ok(group.accepted(id));
                    }
                    if let EnvelopePayload::SurveyClaims(claims)=&mut output {
                        let sources=squad_store::survey::setup(tx,claims.contest_id)?;
                        claims.extraction.validate(&sources.answers).map_err(|_|authorization::forbidden())?;
                        for claim in &mut claims.extraction.claims {claim.id=uuid::Uuid::new_v4().to_string();}
                    }
                    if matches!(work.kind,WorkKind::SurveyExtract|WorkKind::SurveyVote|WorkKind::SurveyRevote|WorkKind::SurveyReport) {survey::validate_output(tx,&work,&output)?;}
                    if work.kind.is_probe() {squad_store::probe::validate_output(tx,&work,&output)?;}
                    if work.kind.grill_composition().is_some(){output=grill::canonical(tx,&work,&output)?;}
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
                    if work.kind.grill_composition().is_some(){event.base.recipients=grill::recipients(&squad_store::grill::setup(tx,work.contest_id.ok_or(StoreError::Unavailable)?)?.setup);}
                    let seq = group.push(tx, event)?;
                    let event = squad_store::events::at(tx, seq.get())?;
                    match &output {
                        EnvelopePayload::GrillQuestion(_)|EnvelopePayload::GrillAnswer(_)|EnvelopePayload::GrillPass(_)|EnvelopePayload::GrillPlan(_) => {
                            squad_store::grill::retain(tx,&event)?;
                            squad_store::work::finish(tx,work_id,WorkStatus::Completed,seq)?;
                            squad_store::contest_steps::settle(tx,work.step_id.ok_or(StoreError::Unavailable)?,&work.target,None)?;
                        }
                        EnvelopePayload::TournamentEntry(entry) => {
                            squad_store::contests::associate(tx,entry.contest_id,ContestKind::Tournament,seq)?;
                            let DebateTurnOutcome::Completed{content,..}=&entry.outcome else{return Err(authorization::forbidden());};
                            squad_store::history::project(tx,&squad_store::history::attributed(&event,work.context_id.clone(),"tournament.entry",HistoryRole::Assistant,Some(entry.entrant.to_string()),content.clone()))?;
                            squad_store::work::finish(tx,work_id,WorkStatus::Completed,seq)?;
                        }
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
                        EnvelopePayload::ContestArtifact(artifact) if work.kind.new_mode_kind().is_some()=>new_mode_accepted_work(work.kind.new_mode_kind().expect("checked"),tx,&work,artifact,seq)?,
                        EnvelopePayload::ContestArtifact(artifact) if work.kind.census_composition().is_some()=>squad_store::census::accept(tx,&work,artifact,seq)?,
                        EnvelopePayload::ContestArtifact(artifact)=>{ if matches!(work.kind,WorkKind::CrosscheckInventory | WorkKind::CrosscheckCell | WorkKind::CrosscheckAudit|WorkKind::CrosscheckGoal | WorkKind::CrosscheckNormalize | WorkKind::CrosscheckRule) {crosscheck::validate_artifact(tx,&work,artifact)?;} squad_store::contest_steps::artifact(tx,artifact,seq)?; },
                        EnvelopePayload::SurveyClaims(_) | EnvelopePayload::SurveyVote(_) | EnvelopePayload::SurveyRevote(_) | EnvelopePayload::SurveyReport(_) => {
                            squad_store::survey::accept(tx,&work,&output,seq)?;
                            let mut state=squad_store::survey::state(tx,work.contest_id.ok_or(StoreError::Unavailable)?)?;
                            let version=state.state_version;state.state_version+=1;state.artifact_event_ids.push(event.base.id);survey::save(tx,group,id,&state,version)?;
                        },
                        EnvelopePayload::ProbeExplanation(_) | EnvelopePayload::ProbeCheck(_) | EnvelopePayload::ProbePrediction(_) | EnvelopePayload::ProbeReport(_) => squad_store::probe::accept(tx,&work,&output,seq)?,
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
        EnvelopePayload::SurveyClaims(v) => v.completed.usage.clone(),
        EnvelopePayload::SurveyVote(v) | EnvelopePayload::SurveyRevote(v) => {
            v.completed.usage.clone()
        }
        EnvelopePayload::SurveyReport(v) => v.completed.usage.clone(),
        EnvelopePayload::MessageFinal(reply) => reply.completed.usage.clone(),
        EnvelopePayload::Verdict(verdict) => verdict.completed.usage.clone(),
        EnvelopePayload::CaseRuling(ruling) => ruling.completed.usage.clone(),
        EnvelopePayload::TournamentEntry(TournamentEntry {
            outcome: DebateTurnOutcome::Completed { usage, .. },
            ..
        }) => usage.clone(),
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
        let (contest_id, step_id) = match &event.content {
            EnvelopePayload::ContestArtifact(v) => (Some(v.contest_id), Some(v.step_id)),
            EnvelopePayload::ContestReport(v) => (Some(v.contest_id), Some(v.step_id)),
            EnvelopePayload::WorkState(v) => (v.contest_id, None),
            EnvelopePayload::UsageRecord(v) => (v.contest_id, None),
            EnvelopePayload::CensusReport(v) => (Some(v.contest_id), None),
            EnvelopePayload::CrosscheckReport(v) => (Some(v.contest_id), None),
            EnvelopePayload::SurveyReport(v) => (Some(v.contest_id), None),
            EnvelopePayload::ProbeReport(v) => (Some(v.contest_id), Some(v.step_id)),
            _ => (None, None),
        };
        tracing::debug!(%request_id,?contest_id,?step_id,event_id=%event.base.id,correlation_id=%event.base.correlation_id,causation_id=?event.base.causation_id,?work_id,cursor=%event.commit.seq.get(),"Router commit accepted");
    }
    if let RequestResult::Internal(
        InternalResult::Claimed(grant) | InternalResult::Started(grant),
    ) = &outcome.result
    {
        tracing::debug!(%request_id,work_id=%grant.work.work_id,correlation_id=%grant.work.trigger_event_id,causation_id=%grant.work.trigger_event_id,attempt=grant.work.attempt,contest_id=?grant.work.contest_id,step_id=?grant.work.step_id,work_kind=?grant.work.kind,fence=grant.fence,epoch=grant.epoch,"Executor grant acknowledged");
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
        | Request::ContestExpire { request_id, .. }
        | Request::ContestStop { request_id, .. }
        | Request::HumanAnswer { request_id, .. }
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
pub(crate) fn load_new_mode_state(
    db: &rusqlite::Connection,
    id: ContestId,
    kind: ContestKind,
) -> Result<ContestState> {
    match kind {
        ContestKind::Forge => Ok(ContestState::Forge(forge::load_state(db, id)?)),
        ContestKind::Tune => Ok(ContestState::Tune(tune::load_state(db, id)?)),
        ContestKind::BassAckwardsPrd => Ok(ContestState::BassAckwardsPrd(
            bass_ackwards_prd::load_state(db, id)?,
        )),
        ContestKind::Forecast => Ok(ContestState::Forecast(forecast::load_state(db, id)?)),
        ContestKind::Decide => Ok(ContestState::Decide(decide::load_state(db, id)?)),
        ContestKind::Negotiate => Ok(ContestState::Negotiate(negotiate::load_state(db, id)?)),
        ContestKind::BlowItUp => Ok(ContestState::BlowItUp(blow_it_up::load_state(db, id)?)),
        ContestKind::Readback => Ok(ContestState::Readback(readback::load_state(db, id)?)),
        ContestKind::Debugging => Ok(ContestState::Debugging(debugging::load_state(db, id)?)),
        ContestKind::Trace => Ok(ContestState::Trace(trace::load_state(db, id)?)),
        ContestKind::Rank => Ok(ContestState::Rank(rank::load_state(db, id)?)),
        _ => Err(StoreError::Unavailable),
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
            EnvelopePayload::CensusPlan(p) => (None, p.text.to_string(), None),
            EnvelopePayload::CensusReport(r) => (None, crate::export::census::report_text(r), None),
            EnvelopePayload::MessageFinal(m) => (
                Some(m.completed.generation.message_id),
                m.content.clone(),
                Some(m.in_reply_to_message_id),
            ),
            _ if kind == "grill.plan" => (
                None,
                crate::orchestrator::census::grill_plan_text(&serde_json::to_value(
                    &event.content,
                )?)
                .unwrap_or_default(),
                None,
            ),
            _ => (None, String::new(), None),
        };
        let crosscheck_state = if let EnvelopePayload::CrosscheckState(state) = &event.content {
            Some(state.clone())
        } else {
            None
        };
        let crosscheck_contest = match &event.content {
            EnvelopePayload::CrosscheckSetup(s) => Some(s.contest_id),
            EnvelopePayload::CrosscheckGrid(g) => Some(g.contest_id),
            _ => None,
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
            contest_id: crosscheck_contest,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: Vec::new(),
            state_version: None,
            superseded,
            eligible: !superseded,
        });
        if let Some(state) = crosscheck_state {
            let record = ctx.records.last_mut().expect("just pushed");
            record.contest_id = Some(state.contest_id);
            record.state_version = Some(state.state_version);
            record.content = serde_json::to_string(&state)?;
            for (kind, slot) in [
                ("crosscheck.setup", &mut record.plan_event_id),
                ("crosscheck.grid", &mut record.grid_event_id),
            ] {
                let raw:Option<String>=db.query_row("SELECT e.id FROM events e JOIN contest_events c ON c.event_seq=e.seq WHERE c.contest_id=? AND e.kind=? ORDER BY e.seq DESC LIMIT 1",params![state.contest_id.to_string(),kind],|r|r.get(0)).optional()?;
                *slot = raw
                    .map(|id| id.parse().map_err(|_| StoreError::Unavailable))
                    .transpose()?;
            }
            let mut q = db.prepare("SELECT cell_id FROM crosscheck_cells WHERE contest_id=?")?;
            record.cell_ids = q
                .query_map([state.contest_id.to_string()], |r| r.get::<_, String>(0))?
                .map(|r| r?.parse().map_err(|_| rusqlite::Error::InvalidQuery))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if state.status == CrosscheckStatus::Complete {
                let report: CrosscheckReport =
                    crosscheck::map(db, state.contest_id, "crosscheck.report")?;
                record.cell_ids.extend(
                    report
                        .residual_risks
                        .iter()
                        .filter_map(|r| r.id.as_str().parse::<CellId>().ok()),
                );
            }
        }
    }
    if let Some(raw) = db
        .query_row("SELECT state_json FROM contests WHERE active=1", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()?
    {
        let (id, kind): (String, String) = db.query_row(
            "SELECT contest_id,kind FROM contests WHERE active=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let kind: ContestKind = serde_json::from_value(serde_json::Value::String(kind))?;
        if matches!(
            kind,
            ContestKind::Tune
                | ContestKind::BassAckwardsPrd
                | ContestKind::Forge
                | ContestKind::Forecast
                | ContestKind::Decide
                | ContestKind::Negotiate
                | ContestKind::BlowItUp
                | ContestKind::Readback
                | ContestKind::Debugging
                | ContestKind::Trace
                | ContestKind::Rank
        ) {
            ctx.active_contest = Some(load_new_mode_state(
                db,
                id.parse().map_err(|_| StoreError::Unavailable)?,
                kind,
            )?);
        } else if let Ok(state) = serde_json::from_str::<ContestSelectionState>(&raw) {
            if state.state == ContestSelectionStatus::AwaitingPick {
                ctx.pending_pick = Some(parser::PendingPick {
                    contest_id: state.contest_id,
                    mode: if state.kind == ContestKind::Grill {
                        parser::QuestionFirstMode::Grill
                    } else {
                        parser::QuestionFirstMode::Debate
                    },
                    eligible: state.answers.iter().map(|a| a.nick.clone()).collect(),
                });
            }
            ctx.active_contest = Some(ContestState::Selection(state));
        } else if let Ok(state) = serde_json::from_str::<ForgeState>(&raw) {
            ctx.active_contest = Some(ContestState::Forge(state));
        } else if let Ok(state) = serde_json::from_str::<ForecastState>(&raw) {
            ctx.active_contest = Some(ContestState::Forecast(state));
        } else if let Ok(state) = serde_json::from_str::<DecideState>(&raw) {
            ctx.active_contest = Some(ContestState::Decide(state));
        } else if let Ok(state) = serde_json::from_str::<NegotiateState>(&raw) {
            ctx.active_contest = Some(ContestState::Negotiate(state));
        } else if let Ok(state) = serde_json::from_str::<BlowItUpState>(&raw) {
            ctx.active_contest = Some(ContestState::BlowItUp(state));
        } else if let Ok(state) = serde_json::from_str::<ReadbackState>(&raw) {
            ctx.active_contest = Some(ContestState::Readback(state));
        } else if let Ok(state) = serde_json::from_str::<DebuggingState>(&raw) {
            ctx.active_contest = Some(ContestState::Debugging(state));
        } else if let Ok(state) = serde_json::from_str::<TraceState>(&raw) {
            ctx.active_contest = Some(ContestState::Trace(state));
        } else if let Ok(state) = serde_json::from_str::<RankState>(&raw) {
            ctx.active_contest = Some(ContestState::Rank(state));
        } else if let Ok(state) = serde_json::from_str::<GrillState>(&raw) {
            ctx.active_contest = Some(ContestState::Grill(state));
        } else if let Ok(state) = serde_json::from_str::<TournamentState>(&raw) {
            ctx.active_contest = Some(ContestState::Tournament(state));
        } else if let Ok(state) = serde_json::from_str::<CensusState>(&raw) {
            ctx.active_contest = Some(ContestState::Census(state));
        } else if let Ok(state) = serde_json::from_str::<SurveyState>(&raw) {
            ctx.active_contest = Some(ContestState::Survey(state));
        } else {
            ctx.active_contest = if let Ok(state) = serde_json::from_str::<CrosscheckState>(&raw) {
                Some(ContestState::Generic(state.generic()))
            } else if let Ok(state) = serde_json::from_str::<ProbeState>(&raw) {
                Some(ContestState::Probe(state))
            } else {
                Some(ContestState::Debate(serde_json::from_str(&raw)?))
            };
        }
    }
    ctx.pending_human = squad_store::contest_steps::pending_human(db)?;
    Ok(ctx)
}

/// Requests that start or restart model generation; refused at the storage ceiling.
/// The storage guard fails closed: a ceiling hit and an unreadable size both
/// refuse new paid work. History, export, `/clear`, stop and cancel never reach it.
pub(crate) fn storage_refusal(check: squad_store::Result<()>) -> Option<(ErrorCode, String)> {
    match check {
        Ok(()) => None,
        Err(StoreError::Domain(code, text)) => Some((code, text.to_owned())),
        Err(_) => Some((
            ErrorCode::STORAGE_LIMIT,
            "Storage could not be checked, so new work is refused. History is preserved: export it with `squad export`, run `squad doctor`, or free space with /clear.".to_owned(),
        )),
    }
}
fn admits_generation(request: &Request) -> bool {
    match request {
        Request::UserMessage { parsed, .. } => !matches!(
            parsed,
            ParsedInput::Assignment { .. } | ParsedInput::Slash { .. }
        ),
        Request::ContestConfirm { .. }
        | Request::DebateConfirm { .. }
        | Request::ContestPick { .. }
        | Request::Judge { .. }
        | Request::CensusContinue { .. }
        | Request::Retry { .. }
        | Request::Regen { .. } => true,
        _ => false,
    }
}

#[cfg(test)]
mod storage_guard_tests {
    use super::*;

    #[test]
    fn an_unreadable_store_size_refuses_new_work() {
        assert!(storage_refusal(Ok(())).is_none());
        let (code, text) = storage_refusal(Err(StoreError::Unavailable)).unwrap();
        assert_eq!(code, ErrorCode::STORAGE_LIMIT);
        assert!(text.contains("could not be checked"));
        let (code, _) =
            storage_refusal(Err(StoreError::Domain(ErrorCode::STORAGE_LIMIT, "full"))).unwrap();
        assert_eq!(code, ErrorCode::STORAGE_LIMIT);
    }
}

pub mod forge;

pub mod forecast;

pub mod decide;

pub mod negotiate;

pub mod blow_it_up;

pub mod readback;

pub mod debugging;

pub mod trace;

pub mod rank;

pub(crate) fn new_mode_prepare(
    kind: ContestKind,
    tx: &rusqlite::Transaction<'_>,
    group: &mut squad_store::commit::CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    admission: &NewModeAdmission<'_>,
) -> squad_store::Result<RequestResult> {
    match kind {
        ContestKind::Forge => forge::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Tune => tune::prepare(tx, group, id, input, parsed, admission),
        ContestKind::BassAckwardsPrd => {
            bass_ackwards_prd::prepare(tx, group, id, input, parsed, admission)
        }
        ContestKind::Forecast => forecast::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Decide => decide::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Negotiate => negotiate::prepare(tx, group, id, input, parsed, admission),
        ContestKind::BlowItUp => blow_it_up::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Readback => readback::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Debugging => debugging::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Trace => trace::prepare(tx, group, id, input, parsed, admission),
        ContestKind::Rank => rank::prepare(tx, group, id, input, parsed, admission),
        _ => Err(squad_store::StoreError::Unavailable),
    }
}

pub(crate) fn new_mode_accepted_work(
    kind: ContestKind,
    tx: &rusqlite::Connection,
    work: &Work,
    artifact: &ContestArtifact,
    seq: Cursor,
) -> squad_store::Result<()> {
    match kind {
        ContestKind::Forge => forge::accepted_work(tx, work, artifact, seq),
        ContestKind::Tune => tune::accepted_work(tx, work, artifact, seq),
        ContestKind::BassAckwardsPrd => bass_ackwards_prd::accepted_work(tx, work, artifact, seq),
        ContestKind::Forecast => forecast::accepted_work(tx, work, artifact, seq),
        ContestKind::Decide => decide::accepted_work(tx, work, artifact, seq),
        ContestKind::Negotiate => negotiate::accepted_work(tx, work, artifact, seq),
        ContestKind::BlowItUp => blow_it_up::accepted_work(tx, work, artifact, seq),
        ContestKind::Readback => readback::accepted_work(tx, work, artifact, seq),
        ContestKind::Debugging => debugging::accepted_work(tx, work, artifact, seq),
        ContestKind::Trace => trace::accepted_work(tx, work, artifact, seq),
        ContestKind::Rank => rank::accepted_work(tx, work, artifact, seq),
        _ => Err(squad_store::StoreError::Unavailable),
    }
}

pub(crate) fn new_mode_human_answer(
    kind: ContestKind,
    tx: &rusqlite::Connection,
    pending: &parser::PendingHuman,
    complete: bool,
) -> squad_store::Result<()> {
    match kind {
        ContestKind::Forge => forge::human_answer(tx, pending, complete),
        ContestKind::Tune => tune::human_answer(tx, pending, complete),
        ContestKind::BassAckwardsPrd => bass_ackwards_prd::human_answer(tx, pending, complete),
        ContestKind::Forecast => forecast::human_answer(tx, pending, complete),
        ContestKind::Decide => decide::human_answer(tx, pending, complete),
        ContestKind::Negotiate => negotiate::human_answer(tx, pending, complete),
        ContestKind::BlowItUp => blow_it_up::human_answer(tx, pending, complete),
        ContestKind::Readback => readback::human_answer(tx, pending, complete),
        ContestKind::Debugging => debugging::human_answer(tx, pending, complete),
        ContestKind::Trace => trace::human_answer(tx, pending, complete),
        ContestKind::Rank => rank::human_answer(tx, pending, complete),
        _ => Err(squad_store::StoreError::Unavailable),
    }
}

pub(crate) fn new_mode_stop(
    kind: ContestKind,
    tx: &rusqlite::Transaction<'_>,
    group: &mut squad_store::commit::CommitGroup,
    id: EventId,
    contest: ContestId,
) -> squad_store::Result<()> {
    match kind {
        ContestKind::Forge => forge::stop(tx, group, id, contest),
        ContestKind::Tune => tune::stop(tx, group, id, contest),
        ContestKind::BassAckwardsPrd => bass_ackwards_prd::stop(tx, group, id, contest),
        ContestKind::Forecast => forecast::stop(tx, group, id, contest),
        ContestKind::Decide => decide::stop(tx, group, id, contest),
        ContestKind::Negotiate => negotiate::stop(tx, group, id, contest),
        ContestKind::BlowItUp => blow_it_up::stop(tx, group, id, contest),
        ContestKind::Readback => readback::stop(tx, group, id, contest),
        ContestKind::Debugging => debugging::stop(tx, group, id, contest),
        ContestKind::Trace => trace::stop(tx, group, id, contest),
        ContestKind::Rank => rank::stop(tx, group, id, contest),
        _ => Err(squad_store::StoreError::Unavailable),
    }
}

pub(crate) fn new_mode_recover(
    kind: ContestKind,
    tx: &rusqlite::Connection,
    contest: ContestId,
) -> squad_store::Result<()> {
    match kind {
        ContestKind::Forge => forge::recover(tx, contest),
        ContestKind::Tune => tune::recover(tx, contest),
        ContestKind::BassAckwardsPrd => bass_ackwards_prd::recover(tx, contest),
        ContestKind::Forecast => forecast::recover(tx, contest),
        ContestKind::Decide => decide::recover(tx, contest),
        ContestKind::Negotiate => negotiate::recover(tx, contest),
        ContestKind::BlowItUp => blow_it_up::recover(tx, contest),
        ContestKind::Readback => readback::recover(tx, contest),
        ContestKind::Debugging => debugging::recover(tx, contest),
        ContestKind::Trace => trace::recover(tx, contest),
        ContestKind::Rank => rank::recover(tx, contest),
        _ => Err(squad_store::StoreError::Unavailable),
    }
}

pub(crate) fn new_mode_parsed_kind(parsed: &ParsedInput) -> Option<ContestKind> {
    match parsed {
        ParsedInput::Forge { .. } => Some(ContestKind::Forge),
        ParsedInput::Tune { .. } => Some(ContestKind::Tune),
        ParsedInput::BassAckwardsPrd { .. } => Some(ContestKind::BassAckwardsPrd),
        ParsedInput::Forecast { .. } => Some(ContestKind::Forecast),
        ParsedInput::Decide { .. } => Some(ContestKind::Decide),
        ParsedInput::Negotiate { .. } => Some(ContestKind::Negotiate),
        ParsedInput::BlowItUp { .. } => Some(ContestKind::BlowItUp),
        ParsedInput::Readback { .. } => Some(ContestKind::Readback),
        ParsedInput::Debugging { .. } => Some(ContestKind::Debugging),
        ParsedInput::Trace { .. } => Some(ContestKind::Trace),
        ParsedInput::Rank { .. } => Some(ContestKind::Rank),
        _ => None,
    }
}

pub mod human;

pub struct NewModeAdmission<'a> {
    pub bindings: &'a BTreeMap<String, ExecutorBinding>,
    pub config: Option<&'a SquadConfig>,
    pub daily_max: i64,
    pub confirmation_threshold: f64,
    pub preconfirmed: bool,
}

pub mod tune;

pub mod bass_ackwards_prd;
#[cfg(test)]
mod observability_tests;
