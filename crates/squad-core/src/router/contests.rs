//! Router commit hooks for the trusted contest engine (Protocol section 02,
//! "Router commit pipeline" and "Shared contest execution"). These methods
//! retain the normal receipt gate and return only committed canonical events.
use super::Router;
use squad_protocol::*;
use squad_store::{Result, commit::CommitOutcome};

impl Router {
    /// Submit a router-normalized lifecycle carrier. The router derives recipients,
    /// persists typed associations and validates it against admitted state.
    pub fn produce(&mut self, envelope: ProducerEnvelope) -> Result<CommitOutcome> {
        self.contest_request(InternalRequest::Produce { envelope })
    }
    /// Compare the stored state version, validate a lifecycle signal and commit
    /// its resulting state snapshot. No provider work is dispatched by this hook.
    pub fn transition(
        &mut self,
        envelope_id: EventId,
        submission: ContestTransitionSubmission,
    ) -> Result<CommitOutcome> {
        if submission.kind == ContestKind::Census {
            return self.census_transition(envelope_id, submission);
        }
        self.contest_request(InternalRequest::Transition {
            envelope_id,
            submission,
        })
    }
    /// Atomically persist a resolved step, authorized history deliveries and all
    /// next-work allocations. Repeating the same request returns its receipt.
    pub fn barrier(&mut self, request_id: EventId, commit: BarrierCommit) -> Result<CommitOutcome> {
        self.contest_request(InternalRequest::Barrier { request_id, commit })
    }
    /// Read the durable admitted debate, including its optimistic state_version.
    /// Selection/preflight state is retained in the canonical contests row.
    pub fn debate_state(&self, id: DebateId) -> Result<DebateState> {
        self.store
            .call(move |db| squad_store::contests::debate(db, id))
    }
}

impl Router {
    fn contest_request(&mut self, request: InternalRequest) -> Result<CommitOutcome> {
        let id = match &request {
            InternalRequest::Produce { envelope } => envelope.base.id,
            InternalRequest::Transition { envelope_id, .. } => *envelope_id,
            InternalRequest::Barrier { request_id, .. } => *request_id,
            _ => return Err(super::authorization::forbidden()),
        };
        let digest = squad_store::commit::digest(&request)?;
        let bindings = self.bindings.clone();
        let max = self.daily_max;
        let threshold = self.confirmation_threshold;
        self.store
            .commit(id, "contest-engine".into(), digest, move |tx, group| {
                match request {
                    InternalRequest::Transition {
                        envelope_id,
                        submission,
                    } => {
                        if let ContestTransition::Crosscheck(signal) = &submission.transition {
                            if submission.kind != ContestKind::Crosscheck {
                                return Err(super::debates::invalid());
                            }
                            let state = squad_store::crosscheck::state(tx, submission.contest_id)?;
                            if state.state_version != submission.expected_state_version {
                                return Err(super::debates::invalid());
                            }
                            let setup: CrosscheckSetup =
                                super::crosscheck::map(tx, state.contest_id, "crosscheck.setup")?;
                            let next = crate::orchestrator::crosscheck::advance(
                                &state,
                                signal,
                                setup.goal.is_none(),
                            )
                            .map_err(|_| super::debates::invalid())?;
                            super::crosscheck::save(tx, group, &next, Some(state.state_version))?;
                        } else if submission.kind == ContestKind::Survey {
                            super::survey::transition(tx, group, envelope_id, submission)?;
                        } else if submission.kind == ContestKind::Probe {
                            super::probe::transition(tx, group, envelope_id, submission)?;
                        } else {
                            super::debates::transition(tx, group, envelope_id, submission)?;
                        }
                    }
                    InternalRequest::Produce { envelope } => {
                        if let EnvelopePayload::DebatePreflight(plan) = envelope.content {
                            return super::preflight::propose(tx, group, id, plan, max, threshold);
                        }
                        produce(tx, group, envelope)?;
                    }
                    InternalRequest::Barrier { commit, .. } => {
                        barrier(tx, group, commit, &bindings)?
                    }
                    _ => return Err(super::authorization::forbidden()),
                }
                Ok(group.accepted(id))
            })
    }
}
use rusqlite::{OptionalExtension, Transaction, params};
use squad_store::{StoreError, commit::CommitGroup};
fn produce(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    mut event: ProducerEnvelope,
) -> Result<()> {
    event.base.recipients = vec![Recipient::Me];
    event.base.sender = Actor::System;
    match &event.content {
        EnvelopePayload::DebateTurn(turn) => {
            if !matches!(turn.outcome, DebateTurnOutcome::Forfeit { .. }) {
                return Err(super::authorization::forbidden());
            }
            let record = squad_store::work::record(tx, turn.generation.work_id)?
                .ok_or_else(super::debates::invalid)?;
            let (work, _) = squad_store::work::template(tx, turn.generation.work_id)?
                .ok_or_else(super::debates::invalid)?;
            if record.attempt < 2
                || turn.generation.attempt != record.attempt
                || turn.generation.trigger_event_id != work.trigger_event_id
                || record.status != WorkStatus::Failed
                || work.debate_id != Some(turn.debate_id)
                || work.round != Some(turn.round)
                || work.message_id != turn.generation.message_id
                || work.target != Target::Member(turn.speaker.clone())
            {
                return Err(super::debates::invalid());
            }
            event.base.sender = Actor::System;
            let turn = turn.clone();
            let seq = group.push(tx, event)?;
            tx.execute(
                "UPDATE work SET completed_event_seq=? WHERE work_id=?",
                params![seq.get(), work.work_id.to_string()],
            )?;
            squad_store::contests::associate(tx, turn.debate_id, ContestKind::Debate, seq)?;
        }
        EnvelopePayload::DebateRoundScore(score) => {
            if score.scores.is_some() {
                return Err(super::authorization::forbidden());
            }
            let id:Option<String>=tx.query_row("SELECT work_id FROM work WHERE kind='judge.round' AND debate_id=? AND round=? AND attempt BETWEEN 1 AND 2 AND status='failed'",params![score.debate_id.to_string(),score.round],|r|r.get(0)).optional()?;
            let id: WorkId = id
                .ok_or_else(super::debates::invalid)?
                .parse()
                .map_err(|_| StoreError::Unavailable)?;
            let mut work = squad_store::work::template(tx, id)?
                .ok_or_else(super::debates::invalid)?
                .0;
            let record = squad_store::work::record(tx, id)?.ok_or_else(super::debates::invalid)?;
            work.attempt = record.attempt;
            let mut score = score.clone();
            squad_store::round_scores::canonicalize(tx, &mut score)?;
            event.content = EnvelopePayload::DebateRoundScore(score.clone());
            event.base.sender = Actor::System;
            let seq = group.push(tx, event)?;
            squad_store::round_scores::insert(tx, &work, &score, seq)?;
            let source = squad_store::events::at(tx, seq.get())?;
            squad_store::history::project(
                tx,
                &squad_store::history::attributed(
                    &source,
                    work.context_id,
                    "round.score",
                    HistoryRole::Assistant,
                    Some("judge".into()),
                    serde_json::to_string(&score)?,
                ),
            )?;
        }
        EnvelopePayload::ContestPreflight(plan) => {
            let plan = plan.clone();
            let rejected = GenericContestState {
                contest_id: plan.contest_id,
                kind: plan.kind,
                state_version: 1,
                status: Text::new("REJECTED").expect("literal"),
                step_id: None,
                artifact_event_ids: vec![],
                terminal_reason: Some(
                    Text::new("Contest mode is not implemented").expect("literal"),
                ),
            };
            let seq = group.push(tx, event)?;
            tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at,updated_seq) VALUES (?,?,(SELECT epoch FROM room_meta),1,?,0,?,?,?,?)",params![plan.contest_id.to_string(),plan.kind.as_str(),serde_json::to_string(&rejected)?,plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),seq.get()])?;
            squad_store::contests::associate(tx, plan.contest_id, plan.kind, seq)?;
            group.push(
                tx,
                super::envelope(
                    plan.plan_id.to_string().parse().expect("UUID"),
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                        contest_id: plan.contest_id,
                        plan_id: plan.plan_id,
                        status: PreflightStatus::Rejected,
                        confirmation: None,
                        error: Some(super::error(
                            ErrorCode::COMMAND_NOT_IMPLEMENTED,
                            "Contest mode is not implemented",
                        )),
                    }),
                ),
            )?;
        }
        EnvelopePayload::ContestStep(step) => {
            let state = squad_store::contests::debate(tx, step.contest_id)?;
            if state.state.is_terminal() {
                return Err(super::debates::invalid());
            }
            let step = step.clone();
            let seq = group.push(tx, event)?;
            squad_store::contest_steps::insert(tx, &step, seq)?;
            squad_store::contests::associate(tx, step.contest_id, ContestKind::Debate, seq)?;
        }
        EnvelopePayload::DebateOpening(opening) => {
            let state = squad_store::contests::debate(tx, opening.debate_id)?;
            if opening.debaters != state.debaters
                || opening.rounds != state.total_rounds
                || opening.round != 0
                || state.state != DebateStatus::WaitingForDebaters
            {
                return Err(super::debates::invalid());
            }
            let contest = opening.debate_id;
            let members = state.debaters;
            let text = serde_json::to_string(opening)?;
            let seq = group.push(tx, event)?;
            let source = squad_store::events::at(tx, seq.get())?;
            squad_store::contests::associate(tx, contest, ContestKind::Debate, seq)?;
            for nick in members {
                squad_store::history::project(
                    tx,
                    &squad_store::history::attributed(
                        &source,
                        ContextId(nick.to_string()),
                        "debate.opening",
                        HistoryRole::User,
                        None,
                        text.clone(),
                    ),
                )?;
            }
        }
        EnvelopePayload::DebateSetup(setup) => {
            let state = squad_store::contests::debate(tx, setup.debate_id)?;
            if setup.rounds != state.total_rounds
                || setup
                    .solutions
                    .iter()
                    .map(|s| &s.nick)
                    .ne(state.debaters.iter())
                || !matches!(
                    state.state,
                    DebateStatus::WaitingForDebaters | DebateStatus::RoundCommitting
                )
            {
                return Err(super::debates::invalid());
            }
            let contest = setup.debate_id;
            let members = state.debaters;
            let text = serde_json::to_string(setup)?;
            let seq = group.push(tx, event)?;
            let source = squad_store::events::at(tx, seq.get())?;
            squad_store::contests::associate(tx, contest, ContestKind::Debate, seq)?;
            for nick in members {
                squad_store::history::project(
                    tx,
                    &squad_store::history::attributed(
                        &source,
                        ContextId(nick.to_string()),
                        "debate.setup",
                        HistoryRole::User,
                        None,
                        text.clone(),
                    ),
                )?;
            }
        }
        EnvelopePayload::CaseOpened(opened) => {
            let opened = opened.clone();
            let seq = group.push(tx, event)?;
            squad_store::cases::open(tx, &opened, seq)?;
            if let Some(contest) = opened.debate_id {
                squad_store::contests::associate(tx, contest, ContestKind::Debate, seq)?;
                let mut state = squad_store::contests::debate(tx, contest)?;
                if state.state.is_terminal() {
                    return Err(super::debates::invalid());
                }
                let version = state.state_version;
                state.case_ids.push(opened.case_id);
                state.case_id = Some(opened.case_id);
                state.state_version += 1;
                super::debates::save(tx, group, id_for(&opened), &state, Some(version))?;
            }
            let source = squad_store::events::at(tx, seq.get())?;
            squad_store::history::project(
                tx,
                &squad_store::history::attributed(
                    &source,
                    ContextId(format!("judge:{}", opened.case_id)),
                    "case.material",
                    HistoryRole::User,
                    None,
                    serde_json::to_string(&opened.material)?,
                ),
            )?;
        }
        EnvelopePayload::ContestArtifact(artifact) => {
            let artifact = artifact.clone();
            if artifact.work_id.is_some() {
                return Err(super::authorization::forbidden());
            }
            squad_store::contest_steps::validate_artifact(tx, &artifact)?;
            let seq = group.push(tx, event)?;
            squad_store::contest_steps::artifact(tx, &artifact, seq)?;
        }
        EnvelopePayload::ContestReport(report) => {
            let report = report.clone();
            squad_store::contest_steps::validate_report(tx, &report)?;
            let seq = group.push(tx, event)?;
            squad_store::contests::associate(tx, report.contest_id, ContestKind::Debate, seq)?;
        }
        _ => return Err(super::authorization::forbidden()),
    }
    Ok(())
}
fn barrier(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    commit: BarrierCommit,
    bindings: &std::collections::BTreeMap<String, ExecutorBinding>,
) -> Result<()> {
    if squad_store::schema::validate_contest_kind(tx, commit.contest_id, ContestKind::Crosscheck)? {
        return super::crosscheck::barrier(tx, group, commit);
    }
    if super::survey::is_survey(tx, commit.contest_id)? {
        return super::survey::barrier(tx, group, commit.barrier_event.base.correlation_id, commit);
    }
    if squad_store::schema::validate_contest_kind(tx, commit.contest_id, ContestKind::Probe)? {
        return super::probe::barrier(tx, group, commit.barrier_event.base.correlation_id, commit);
    }
    let state = squad_store::contests::debate(tx, commit.contest_id)?;
    if state.state_version != commit.expected_state_version
        || state.state.is_terminal()
        || (!commit.next_work.is_empty()
            && matches!(
                state.state,
                DebateStatus::Preparing | DebateStatus::AwaitingConfirmation
            ))
    {
        return Err(super::debates::invalid());
    }
    squad_store::contest_steps::validate_step(tx, commit.contest_id, commit.step_id)?;
    for projection in &commit.projections {
        super::authorization::projection(tx, &state, projection)?;
    }
    let mut event = commit.barrier_event;
    event.base.recipients = vec![Recipient::Me];
    if !matches!(&event.content,EnvelopePayload::ContestReport(r) if r.contest_id==commit.contest_id && r.step_id==commit.step_id)
    {
        return Err(super::authorization::forbidden());
    }
    if let EnvelopePayload::ContestReport(report) = &event.content {
        squad_store::contest_steps::validate_report(tx, report)?;
        for id in &report.skipped_work_ids {
            let record = squad_store::work::record(tx, *id)?.ok_or_else(super::debates::invalid)?;
            let work = squad_store::work::template(tx, *id)?
                .ok_or_else(super::debates::invalid)?
                .0;
            if work.contest_id != Some(commit.contest_id)
                || work.step_id != Some(commit.step_id)
                || !matches!(record.status, WorkStatus::Failed | WorkStatus::Cancelled)
            {
                return Err(super::debates::invalid());
            }
            let error = record.error.unwrap_or_else(|| {
                super::error(ErrorCode::WORK_CANCELLED, "Assignment was cancelled")
            });
            squad_store::contest_steps::settle(tx, commit.step_id, &work.target, Some(&error))?;
        }
    }
    let seq = group.push(tx, event)?;
    squad_store::contest_steps::barrier(tx, commit.step_id, seq)?;
    squad_store::contests::associate(tx, commit.contest_id, ContestKind::Debate, seq)?;
    for projection in commit.projections {
        squad_store::history::project(tx, &projection)?;
    }
    for mut work in commit.next_work {
        let boundary: i64 = tx.query_row(
            "SELECT COALESCE(MAX(pos),0) FROM history WHERE context_id=?",
            [&work.context_id.0],
            |r| r.get(0),
        )?;
        work.history_through =
            HistoryPosition::new(boundary).map_err(|_| super::debates::invalid())?;
        if work.contest_id != Some(commit.contest_id) {
            return Err(super::authorization::forbidden());
        }
        if work.kind == WorkKind::Judge {
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE debate_id=? AND kind='judge.round' AND status IN ('queued','running'))",[commit.contest_id.to_string()],|r|r.get(0))?;
            if pending {
                return Err(super::debates::invalid());
            }
        }
        let binding = bindings
            .get(&work.target.to_string())
            .ok_or(StoreError::Domain(
                ErrorCode::CONFIG_INVALID,
                "Missing executor binding",
            ))?;
        squad_store::work::insert(tx, &work, binding, None)?;
        if let Some(step) = work.step_id {
            squad_store::contest_steps::attach_work(tx, step, &work)?;
        }
        queued(tx, group, &work)?;
    }
    Ok(())
}

fn id_for(opened: &CaseOpened) -> EventId {
    opened.judge_invocation_id
}

impl Router {
    /// Freeze a model step, private routing maps, its allowed inputs and all work
    /// in one transaction before either assignment becomes runnable. This is the
    /// initial-step equivalent of `barrier`'s next-work allocation.
    pub fn allocate_step(
        &mut self,
        request_id: EventId,
        step: ContestStep,
        projections: Vec<HistoryProjection>,
        work: Vec<Work>,
        private_maps: std::collections::BTreeMap<String, serde_json::Value>,
    ) -> Result<CommitOutcome> {
        if self.store.call({
            let id = step.contest_id;
            move |db| super::survey::is_survey(db, id)
        })? {
            return super::survey::allocate(
                self,
                request_id,
                step,
                projections,
                work,
                private_maps,
            );
        }
        let digest = squad_store::commit::digest(&(&step, &projections, &work, &private_maps))?;
        let bindings = self.bindings.clone();
        self.store.commit(
            request_id,
            "contest-engine".into(),
            digest,
            move |tx, group| {
                if squad_store::schema::validate_contest_kind(
                    tx,
                    step.contest_id,
                    ContestKind::Crosscheck,
                )? {
                    super::crosscheck::allocate(
                        tx,
                        group,
                        step,
                        projections,
                        work,
                        private_maps,
                        &bindings,
                    )?;
                    return Ok(group.accepted(request_id));
                }
                if squad_store::schema::validate_contest_kind(
                    tx,
                    step.contest_id,
                    ContestKind::Probe,
                )? {
                    if !projections.is_empty() || !private_maps.is_empty() {
                        return Err(super::authorization::forbidden());
                    }
                    return super::probe::allocate(tx, group, request_id, step, work, &bindings);
                }
                let state = squad_store::contests::debate(tx, step.contest_id)?;
                if state.state.is_terminal()
                    || (!work.is_empty()
                        && matches!(
                            state.state,
                            DebateStatus::Preparing | DebateStatus::AwaitingConfirmation
                        ))
                {
                    return Err(super::debates::invalid());
                }
                let seq = group.push(
                    tx,
                    super::envelope(
                        request_id,
                        Actor::System,
                        vec![Recipient::Me],
                        EnvelopePayload::ContestStep(step.clone()),
                    ),
                )?;
                squad_store::contest_steps::insert(tx, &step, seq)?;
                for projection in &projections {
                    super::authorization::projection(tx, &state, projection)?;
                }
                for (scope, value) in private_maps {
                    squad_store::contest_steps::private_map(tx, step.contest_id, &scope, &value)?;
                }
                for projection in projections {
                    squad_store::history::project(tx, &projection)?;
                }
                for mut item in work {
                    let boundary: i64 = tx.query_row(
                        "SELECT COALESCE(MAX(pos),0) FROM history WHERE context_id=?",
                        [&item.context_id.0],
                        |r| r.get(0),
                    )?;
                    item.history_through =
                        HistoryPosition::new(boundary).map_err(|_| super::debates::invalid())?;
                    if item.contest_id != Some(step.contest_id)
                        || item.step_id != Some(step.step_id)
                    {
                        return Err(super::authorization::forbidden());
                    }
                    let binding = bindings
                        .get(&item.target.to_string())
                        .ok_or_else(super::authorization::forbidden)?;
                    squad_store::work::insert(tx, &item, binding, None)?;
                    squad_store::contest_steps::attach_work(tx, step.step_id, &item)?;
                    queued(tx, group, &item)?;
                }
                squad_store::contests::associate(tx, step.contest_id, ContestKind::Debate, seq)?;
                Ok(group.accepted(request_id))
            },
        )
    }
}

impl Router {
    /// Allocate the two debate turns (or case work) against one stored version.
    /// DebateTurn deliberately has no generic step_id under the shared Work
    /// contract; its per-round boundaries are frozen together by this hook.
    pub fn allocate_work(
        &mut self,
        request_id: EventId,
        contest: ContestId,
        expected_state_version: u64,
        work: Vec<Work>,
    ) -> Result<CommitOutcome> {
        let digest = squad_store::commit::digest(&(contest, expected_state_version, &work))?;
        let bindings = self.bindings.clone();
        self.store.commit(request_id,"contest-engine".into(),digest,move |tx,group| {
            let state=squad_store::contests::debate(tx,contest)?;
            if state.state_version!=expected_state_version || state.state.is_terminal() {return Err(super::debates::invalid());}
            let frozen:Option<String>=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='engine.plan'",[contest.to_string()],|r|r.get(0)).optional()?;
            let frozen=frozen.map(|raw|serde_json::from_str::<crate::orchestrator::types::DebatePlan>(&raw)).transpose()?;
            for mut item in work {
                let boundary: i64=tx.query_row("SELECT COALESCE(MAX(pos),0) FROM history WHERE context_id=?",[&item.context_id.0],|r|r.get(0))?;
                if item.history_through.get()>boundary {return Err(super::debates::invalid());}
                item.history_through=HistoryPosition::new(boundary).map_err(|_|super::debates::invalid())?;
                if item.contest_id!=Some(contest) || item.debate_id!=Some(contest) || item.step_id.is_some() {return Err(super::authorization::forbidden());}
                if item.kind==WorkKind::DebateTurn && (item.round!=Some(state.round) || state.state!=DebateStatus::RoundRunning) {return Err(super::debates::invalid());}
                if item.kind==WorkKind::JudgeRound && (!matches!(state.state,DebateStatus::RoundRunning|DebateStatus::RoundCommitting|DebateStatus::Judging) || item.round.is_none_or(|r|r>state.round)) {return Err(super::debates::invalid());}
                if item.kind==WorkKind::Judge {
                    let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE debate_id=? AND kind='judge.round' AND status IN ('queued','running'))",[contest.to_string()],|r|r.get(0))?;
                    if pending || state.state!=DebateStatus::Judging {return Err(super::debates::invalid());}
                }
                let mut binding=bindings.get(&item.target.to_string()).cloned().ok_or_else(super::authorization::forbidden)?;
                if let Some(plan)=&frozen {
                    let model=match &item.target {Target::Judge=>plan.judge.as_ref(),Target::Member(nick)=>plan.members.iter().find(|m|m.nick.as_ref()==Some(nick))}.ok_or_else(super::debates::invalid)?;
                    binding.model=Text::new(model.resolved_slug.clone()).map_err(|_|super::debates::invalid())?;
                    if let Some(effort)=model.reasoning_effort {binding.effort=effort;}
                } else if item.target==Target::Judge { binding.model=Text::new(state.judge_model.clone()).map_err(|_|super::debates::invalid())?; }
                squad_store::work::insert(tx,&item,&binding,None)?;
                queued(tx,group,&item)?;
            }
            Ok(group.accepted(request_id))
        })
    }
}

impl Router {
    /// Open a debate case and freeze its authorized seven-item bundle in the
    /// case-only history. Router supplies `case.bundle` source metadata after
    /// allocating the case.opened event sequence; no room/member delivery occurs.
    pub fn open_debate_case(
        &mut self,
        event: ProducerEnvelope,
        bundle: DebateAdjudicationBundle,
    ) -> Result<CommitOutcome> {
        let id = event.base.id;
        let digest = squad_store::commit::digest(&(&event, &bundle))?;
        self.store.commit(id,"contest-engine".into(),digest,move |tx,group| {
            let EnvelopePayload::CaseOpened(opened)=&event.content else{return Err(super::authorization::forbidden());};
            let CaseMaterial::Debate{setup,..}=&opened.material else{return Err(super::authorization::forbidden());};
            let state=squad_store::contests::debate(tx,setup.debate_id)?;
            if opened.debate_id!=Some(setup.debate_id) || bundle.topic.message_id!=setup.topic_message_id || bundle.topic.text!=setup.topic_text || bundle.solutions!=setup.solutions || bundle.instruction!=setup.instruction || bundle.judge.model!=opened.judge_model.as_str() || bundle.round_scoring.round_tally!=squad_store::round_scores::tally(tx,&state)? {return Err(super::debates::invalid());}
            let scores=squad_store::round_scores::retained(tx,state.debate_id)?;
            let mut rounds=tx.prepare("SELECT json_extract(e.payload,'$.data.round') FROM contest_steps s JOIN events e ON e.seq=s.updated_seq WHERE s.contest_id=? AND s.committed=1 AND e.kind='contest.report' AND json_extract(e.payload,'$.data.round')>0")?;
            let committed=rounds.query_map([state.debate_id.to_string()],|r|r.get::<_,u32>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut unscored=scores.iter().filter(|score|score.scores.is_none()).map(|score|score.round).chain(committed.into_iter().filter(|round|!scores.iter().any(|s|s.round==*round && s.scores.is_some()))).collect::<Vec<_>>();unscored.sort_unstable();unscored.dedup();
            if bundle.round_scoring.round_scores!=scores || bundle.round_scoring.unscored_rounds!=unscored {return Err(super::debates::invalid());}
            for turn in &bundle.transcript {
                let raw:Option<String>=tx.query_row("SELECT payload FROM events WHERE kind='debate.turn' AND json_extract(payload,'$.workId')=?",[turn.generation.work_id.to_string()],|r|r.get(0)).optional()?;
                let retained=raw.map(|r|serde_json::from_str::<DebateTurn>(&r)).transpose()?.is_some_and(|t|t==*turn);
                if !retained || turn.debate_id!=state.debate_id {return Err(super::debates::invalid());}
            }
            let case=opened.case_id;produce(tx,group,event)?;
            let source=squad_store::events::by_id(tx,id)?.ok_or(StoreError::Unavailable)?;
            squad_store::history::project(tx,&squad_store::history::attributed(&source,ContextId(format!("judge:{case}")),"case.bundle",HistoryRole::User,None,serde_json::to_string(&bundle)?))?;
            Ok(group.accepted(id))
        })
    }
}

impl Router {
    /// Freeze the configured confirmation threshold before accepting input.
    pub fn with_confirmation_threshold(mut self, usd: f64) -> Result<Self> {
        if !usd.is_finite() || usd < 0.0 {
            return Err(StoreError::Domain(
                ErrorCode::CONFIG_INVALID,
                "Invalid confirmation threshold",
            ));
        }
        self.confirmation_threshold = usd;
        Ok(self)
    }
}

pub(crate) fn queued(tx: &Transaction<'_>, group: &mut CommitGroup, work: &Work) -> Result<()> {
    group.push(
        tx,
        super::envelope(
            work.trigger_event_id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::WorkState(WorkStateEvent {
                work_id: work.work_id,
                trigger_event_id: work.trigger_event_id,
                target: work.target.clone(),
                context_id: work.context_id.clone(),
                attempt: 0,
                status: WorkStatus::Queued,
                error: None,
                debate_id: work.debate_id,
                case_id: work.case_id,
                contest_id: work.contest_id,
            }),
        ),
    )?;
    Ok(())
}
