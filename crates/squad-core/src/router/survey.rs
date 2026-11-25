//! Router authority for frozen survey sources, stage boundaries and results.
use super::{envelope, error};
use crate::orchestrator::survey as script;
use rusqlite::{Connection, Transaction, params};
use squad_protocol::*;
use squad_store::{
    Result, StoreError,
    commit::{CommitGroup, CommitOutcome},
};
use std::collections::BTreeMap;
fn invalid() -> StoreError {
    StoreError::Domain(ErrorCode::INVALID_STATE_TRANSITION, "Invalid survey move")
}
pub fn save(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &SurveyState,
    version: u64,
) -> Result<()> {
    let seq = group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::SurveyState(state.clone()),
        ),
    )?;
    if tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=? WHERE contest_id=? AND kind='survey' AND state_version=?",params![state.state_version as i64,serde_json::to_string(state)?,!state.state.is_terminal(),seq.get(),state.contest_id.to_string(),version as i64])?!=1 {return Err(invalid());}
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Survey, seq)?;
    if state.state.is_terminal() {
        tx.execute_batch("UPDATE spend_daily SET reserved_micros=reserved_micros-COALESCE((SELECT SUM(amount_micros) FROM reservations WHERE reservations.day=spend_daily.day AND attempt IS NULL AND unresolved=0 AND contest_id IN (SELECT contest_id FROM contests WHERE active=0 AND kind='survey')),0); DELETE FROM reservations WHERE attempt IS NULL AND unresolved=0 AND contest_id IN (SELECT contest_id FROM contests WHERE active=0 AND kind='survey');")?;
        squad_store::history::release(tx, state.contest_id)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn admit(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    parsed: ParsedInput,
    bindings: &BTreeMap<String, ExecutorBinding>,
    max: i64,
    threshold: f64,
    preconfirmed: bool,
    config: Option<&SquadConfig>,
) -> Result<RequestResult> {
    let ParsedInput::Survey {
        input: conversation,
        participants,
        source,
    } = parsed
    else {
        return Err(invalid());
    };
    let contest = ContestId::new();
    let (question_message_id, question, answers, collect) = match source {
        parser::SurveySource::Question { text } => (input.message_id, text, vec![], true),
        parser::SurveySource::LatestParallelAnswers => {
            let raw:String=tx.query_row("SELECT u.payload FROM events u WHERE u.kind='user.message' AND (SELECT COUNT(*) FROM json_each(u.recipients) WHERE value<>'me')>1 AND EXISTS(SELECT 1 FROM events a WHERE a.kind='message.final' AND json_extract(a.payload,'$.inReplyToMessageId')=json_extract(u.payload,'$.messageId') AND NOT EXISTS(SELECT 1 FROM superseded s WHERE s.superseded_event_id=a.id)) ORDER BY u.seq DESC LIMIT 1",[],|r|r.get(0))?;
            let question: UserMessage = serde_json::from_str(&raw)?;
            let mut q=tx.prepare("SELECT a.seq FROM events a WHERE a.kind='message.final' AND json_extract(a.payload,'$.inReplyToMessageId')=? AND NOT EXISTS(SELECT 1 FROM superseded s WHERE s.superseded_event_id=a.id) ORDER BY a.seq")?;
            let rows = q
                .query_map([question.input.message_id.to_string()], |r| {
                    r.get::<_, i64>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut answers = vec![];
            for seq in rows {
                let e = squad_store::events::at(tx, seq)?;
                if let (Actor::Member(nick), EnvelopePayload::MessageFinal(a)) =
                    (e.base.sender, e.content)
                    && participants.contains(&nick)
                    && !answers.iter().any(|a: &ContestAnswer| a.nick == nick)
                {
                    answers.push(ContestAnswer {
                        nick,
                        message_id: a.completed.generation.message_id,
                        text: a.content,
                    });
                }
            }
            (question.input.message_id, question.body, answers, false)
        }
    };
    let setup = SurveySetup {
        contest_id: contest,
        question_message_id,
        question: Text::new(question).map_err(|_| invalid())?,
        participants: participants.clone(),
        answers,
        collect,
    };
    setup.validate().map_err(|_| invalid())?;
    super::variables::apply(tx, group, id, &conversation.variable_updates)?;
    let tokens = config
        .map(|c| crate::orchestrator::blind::tokens(c, bindings))
        .unwrap_or_else(|| setup.participants.iter().map(ToString::to_string).collect());
    let (calls, attempts) = SurveyPreflight::counts(participants.len(), collect);
    // Reserve conservative frozen input/output bounds for every invocation.
    // Actual rendered requests are independently checked against that context.
    let mut max_call = 0.0_f64;
    for target in participants
        .iter()
        .map(|n| n.as_str())
        .chain(std::iter::once("judge"))
    {
        let b = bindings.get(target).ok_or(StoreError::Domain(
            ErrorCode::MODEL_UNAVAILABLE,
            "Survey executor is unavailable",
        ))?;
        let (length, raw): (i64, String) = tx.query_row(
            "SELECT context_length,pricing_json FROM catalog WHERE slug=?",
            [b.model.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let prices: serde_json::Value = serde_json::from_str(&raw)?;
        let price = |key: &str| {
            prices[key]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| prices[key].as_f64())
                .unwrap_or(0.0)
        };
        let input_bound =
            (serde_json::to_vec(&setup)?.len() + setup.participants.len() * 32768 + 65536) as f64;
        let cost = input_bound.min(length as f64) * price("prompt") + 12288.0 * price("completion");
        if !cost.is_finite() || cost < 0.0 {
            return Err(invalid());
        }
        max_call = max_call.max(cost);
    }
    let reserved = squad_store::accounting::reservation_micros(max_call)
        .map_err(|_| invalid())?
        .checked_mul(i64::from(attempts))
        .ok_or_else(invalid)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    match squad_store::accounting::admit(tx, &day, reserved, max) {
        Ok(()) => {}
        Err(StoreError::Domain(ErrorCode::DAILY_MAX_REACHED, _)) => {
            return Ok(RequestResult::Rejected {
                request_id: id,
                error: super::daily_cap_error(tx, &day, reserved, max),
            });
        }
        Err(error) => return Err(error),
    }
    let plan = ContestPreflight {
        contest_id: contest,
        kind: ContestKind::Survey,
        plan_id: PlanId::new(),
        expires_at: group.accepted_at
            + chrono::Duration::milliseconds(squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: max_call * f64::from(calls),
            reserved_usd: reserved as f64 / 1e6,
            remaining_usd: (max
                - squad_store::accounting::daily(tx, &day)?.spent_micros
                - squad_store::accounting::daily(tx, &day)?.reserved_micros)
                as f64
                / 1e6,
            model_calls: calls,
        },
        participants,
        confirmation_required: max_call * f64::from(calls) > threshold,
        warning: None,
        settings: serde_json::json!({"collect":collect,"reservedAttempts":attempts,"admissionMaxMicros":max}),
    };
    let state = SurveyState {
        contest_id: contest,
        state: SurveyStatus::Preflight,
        state_version: 1,
        step_id: None,
        artifact_event_ids: vec![],
        terminal_reason: None,
        completeness: None,
    };
    tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at) VALUES (?,'survey',(SELECT epoch FROM room_meta),1,?,1,?,?,?)",params![contest.to_string(),serde_json::to_string(&state)?,plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339()])?;
    tx.execute(
        "INSERT INTO survey_sources VALUES (?,?)",
        params![contest.to_string(), serde_json::to_string(&setup)?],
    )?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "survey.redaction",
        &serde_json::json!({"tokens":tokens}),
    )?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "survey.bindings",
        &serde_json::to_value(bindings)?,
    )?;
    let mut user = envelope(
        id,
        Actor::Me,
        setup
            .participants
            .iter()
            .cloned()
            .map(Recipient::Member)
            .collect(),
        EnvelopePayload::UserMessage(UserMessage {
            input,
            body: setup.question.to_string(),
            resolved_quotes: vec![],
            debate_id: None,
            case_id: None,
            contest_id: Some(contest),
        }),
    );
    user.base.id = id;
    let seq = group.push(tx, user)?;
    squad_store::contests::associate(tx, contest, ContestKind::Survey, seq)?;
    let seq = group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPreflight(plan.clone()),
        ),
    )?;
    squad_store::contests::associate(tx, contest, ContestKind::Survey, seq)?;
    save(tx, group, id, &state, 1)?;
    if plan.confirmation_required && !preconfirmed {
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(plan),
            cursor: seq,
        });
    }
    resolve(tx, group, id, contest, plan.plan_id, false)
}
pub fn resolve(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let mut state = squad_store::survey::state(tx, contest)?;
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=? AND plan_id=?",
        params![contest.to_string(), plan.to_string()],
        |r| r.get(0),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    if state.state != SurveyStatus::Preflight {
        return Err(invalid());
    }
    let expired = group.accepted_at >= plan.expires_at;
    let status = if expired {
        PreflightStatus::Expired
    } else if cancel {
        PreflightStatus::Cancelled
    } else {
        PreflightStatus::Confirmed
    };
    if cancel || expired {
        state.state = SurveyStatus::Stopped;
        state.terminal_reason = Some(
            Text::new(if expired {
                "Survey preflight expired"
            } else {
                "Survey preflight cancelled"
            })
            .expect("literal"),
        );
    } else {
        let amount = squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
            .map_err(|_| invalid())?;
        squad_store::accounting::reserve(
            tx,
            &format!("plan:{}", plan.plan_id),
            None,
            &chrono::Local::now().format("%Y-%m-%d").to_string(),
            amount,
            plan.settings["admissionMaxMicros"]
                .as_i64()
                .ok_or_else(invalid)?,
        )?;
        tx.execute(
            "UPDATE reservations SET plan_id=?,contest_id=? WHERE reservation_id=?",
            params![
                plan.plan_id.to_string(),
                contest.to_string(),
                format!("plan:{}", plan.plan_id)
            ],
        )?;
        let setup = squad_store::survey::setup(tx, contest)?;
        state.state = SurveyStatus::WaitingForParticipants;
        let seq = group.push(
            tx,
            envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::SurveySetup(setup),
            ),
        )?;
        squad_store::contests::associate(tx, contest, ContestKind::Survey, seq)?;
    }
    group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: contest,
                plan_id: plan.plan_id,
                confirmation: (status == PreflightStatus::Confirmed)
                    .then_some(Confirmation::Explicit),
                status,
                error: None,
            }),
        ),
    )?;
    let version = state.state_version;
    state.state_version += 1;
    save(tx, group, id, &state, version)?;
    Ok(group.accepted(id))
}
pub fn is_survey(db: &Connection, id: ContestId) -> Result<bool> {
    Ok(squad_store::schema::validate_contest_kind(
        db,
        id,
        ContestKind::Survey,
    )?)
}
/// Router recomputes the authoritative map before accepting judge prose.
pub fn validate_output(db: &Connection, work: &Work, output: &EnvelopePayload) -> Result<()> {
    squad_store::survey::validate_result(db, work, output)?;
    if let EnvelopePayload::SurveyReport(r) = output {
        let setup = squad_store::survey::setup(db, r.contest_id)?;
        let claims = squad_store::survey::claims(db, r.contest_id)?;
        let selected = squad_store::survey::selected(db, r.contest_id)?
            .claims
            .into_iter()
            .map(|c| c.claim_id)
            .collect::<Vec<_>>();
        let map = script::map(
            &setup,
            &claims,
            &selected,
            &squad_store::survey::ballots(db, r.contest_id, false)?,
            &squad_store::survey::ballots(db, r.contest_id, true)?,
        );
        let completeness = script::completeness(
            &setup,
            &claims,
            &selected,
            &squad_store::survey::ballots(db, r.contest_id, false)?,
            &squad_store::survey::ballots(db, r.contest_id, true)?,
        );
        if r.map != map || r.completeness.as_ref() != Some(&completeness) {
            return Err(invalid());
        }
        r.report.validate(&map).map_err(|_| invalid())?;
    }
    Ok(())
}

/// Survey uses the same allocate_step hook as other scripts. Every assignment
/// gets a fresh context and an anonymous projection of only its frozen inputs.
pub fn allocate(
    router: &mut super::Router,
    request_id: EventId,
    step: ContestStep,
    projections: Vec<HistoryProjection>,
    work: Vec<Work>,
    maps: BTreeMap<String, serde_json::Value>,
) -> Result<CommitOutcome> {
    let digest = squad_store::commit::digest(&(&step, &projections, &work, &maps))?;
    router.store.commit(request_id,"contest-engine".into(),digest,move |tx,group| {
        let mut state=squad_store::survey::state(tx,step.contest_id)?;
        if state.state.is_terminal() || state.step_id.is_some() || state.state==SurveyStatus::Preflight || !projections.is_empty() {return Err(invalid());}
        let expected=match state.state {SurveyStatus::Collecting=>WorkKind::Reply,SurveyStatus::Extracting=>WorkKind::SurveyExtract,SurveyStatus::Voting=>WorkKind::SurveyVote,SurveyStatus::Revoting=>WorkKind::SurveyRevote,SurveyStatus::Reporting=>WorkKind::SurveyReport,_=>return Err(invalid())};
        let setup=squad_store::survey::setup(tx,step.contest_id)?;
        let targets=if matches!(expected,WorkKind::SurveyExtract|WorkKind::SurveyReport) {vec![Target::Judge]}else{setup.participants.iter().cloned().map(Target::Member).collect()};
        if work.len()!=targets.len() || work.iter().map(|w|&w.target).ne(targets.iter()) {return Err(invalid());}
        let raw:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='survey.bindings'",[step.contest_id.to_string()],|r|r.get(0))?;
        let bindings:BTreeMap<String,ExecutorBinding>=serde_json::from_str(&raw)?;
        let seq=group.push(tx,envelope(request_id,Actor::System,vec![Recipient::Me],EnvelopePayload::ContestStep(step.clone())))?;
        squad_store::contest_steps::insert(tx,&step,seq)?;
        let source=if state.state==SurveyStatus::Revoting {
            let origin=*step.inputs.last().ok_or_else(invalid)?;squad_store::events::by_id(tx,origin)?.ok_or_else(invalid)?
        }else{squad_store::events::at(tx,seq.get())?};
        for mut w in work {
            if w.kind!=expected || w.contest_id!=Some(step.contest_id) || w.step_id!=Some(step.step_id) {return Err(invalid());}
            let key=format!("survey.work:{}",w.work_id);
            let values=maps.get(&key).ok_or_else(invalid)?;
            let data=values["mode_data_json"].as_str().ok_or_else(invalid)?.to_owned();
            if let Some(case)=w.case_id {
                let d:serde_json::Value=serde_json::from_str(&data)?;
                let material=if expected==WorkKind::SurveyExtract {SurveyCaseMaterial::Extraction{setup:setup.clone()}}else{SurveyCaseMaterial::Report{setup:setup.clone(),claims:serde_json::from_value(d["claims"].clone())?,first:serde_json::from_value(d["first"].clone())?,second:serde_json::from_value(d["second"].clone())?,map:serde_json::from_value(d["map"].clone())?}};
                let opened=CaseOpened{case_id:case,judge_invocation_id:request_id,work_id:Some(w.work_id),judge_model:bindings.get("judge").ok_or_else(invalid)?.model.clone(),material:CaseMaterial::Survey{material},debate_id:None,contest_id:Some(step.contest_id)};
                let case_seq=group.push(tx,envelope(request_id,Actor::System,vec![Recipient::Me],EnvelopePayload::CaseOpened(opened.clone())))?;
                squad_store::cases::open(tx,&opened,case_seq)?;
                squad_store::contests::associate(tx,step.contest_id,ContestKind::Survey,case_seq)?;
            }
            let p=HistoryProjection{context_id:w.context_id.clone(),origin_event_id:source.base.id,projection_key:Text::new("survey.input").expect("literal"),role:HistoryRole::User,name:Some("squad".into()),content:data,meta:HistoryMetadata::Anonymous{anonymous:True}};
            w.history_through=squad_store::history::project(tx,&p)?.ok_or_else(invalid)?;
            squad_store::contest_steps::private_map(tx,step.contest_id,&key,values)?;
            let mut binding=bindings.get(&w.target.to_string()).ok_or_else(invalid)?.clone();
            if w.target==Target::Judge {binding.effort=constants::NEW_MODE_JUDGE_REASONING_EFFORT;}
            squad_store::work::insert(tx,&w,&binding,None)?;
            squad_store::contest_steps::attach_work(tx,step.step_id,&w)?;
            group.push(tx,envelope(request_id,Actor::System,vec![Recipient::Me],EnvelopePayload::WorkState(WorkStateEvent{work_id:w.work_id,trigger_event_id:w.trigger_event_id,target:w.target.clone(),context_id:w.context_id.clone(),attempt:0,status:WorkStatus::Queued,error:None,debate_id:None,case_id:w.case_id,contest_id:w.contest_id})))?;
        }
        squad_store::contests::associate(tx,step.contest_id,ContestKind::Survey,seq)?;
        let version=state.state_version;state.state_version+=1;state.step_id=Some(step.step_id);save(tx,group,request_id,&state,version)?;
        Ok(group.accepted(request_id))
    })
}
pub fn barrier(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    commit: BarrierCommit,
) -> Result<()> {
    let mut state = squad_store::survey::state(tx, commit.contest_id)?;
    if state.state_version != commit.expected_state_version
        || state.step_id != Some(commit.step_id)
        || state.state.is_terminal()
        || !commit.next_work.is_empty()
        || !commit.projections.is_empty()
    {
        return Err(invalid());
    }
    let mut q = tx.prepare(
        "SELECT work_id,status FROM work WHERE contest_id=? AND step_id=? ORDER BY rowid",
    )?;
    let rows = q
        .query_map(
            params![commit.contest_id.to_string(), commit.step_id.to_string()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() || rows.iter().any(|(_, s)| s == "queued" || s == "running") {
        return Err(invalid());
    }
    let mut skipped = vec![];
    for (wid, status) in &rows {
        if status != "completed" {
            let wid: WorkId = wid.parse().map_err(|_| invalid())?;
            skipped.push(wid);
            let (w, _) = squad_store::work::template(tx, wid)?.ok_or_else(invalid)?;
            squad_store::contest_steps::settle(
                tx,
                commit.step_id,
                &w.target,
                Some(&error(
                    ErrorCode::PROVIDER_ERROR,
                    "Survey assignment missing",
                )),
            )?;
        }
    }
    let EnvelopePayload::ContestReport(mut report) = commit.barrier_event.content else {
        return Err(invalid());
    };
    if report.contest_id != state.contest_id || report.step_id != commit.step_id {
        return Err(invalid());
    }
    report.skipped_work_ids = skipped;
    let seq = group.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestReport(report),
        ),
    )?;
    squad_store::contest_steps::barrier(tx, commit.step_id, seq)?;
    squad_store::contests::associate(tx, state.contest_id, ContestKind::Survey, seq)?;
    let setup = squad_store::survey::setup(tx, state.contest_id)?;
    let failed = rows.iter().any(|(_, s)| s != "completed");
    let previous = state.state;
    state.state = match state.state {
        SurveyStatus::Collecting => {
            let mut setup = setup.clone();
            for (wid, status) in &rows {
                if status == "completed" {
                    let raw:String=tx.query_row("SELECT e.payload FROM events e JOIN work w ON w.completed_event_seq=e.seq WHERE w.work_id=?",[wid],|r|r.get(0))?;
                    let reply: Completion = serde_json::from_str(&raw)?;
                    let w = squad_store::work::template(tx, wid.parse().map_err(|_| invalid())?)?
                        .ok_or_else(invalid)?
                        .0;
                    let Target::Member(nick) = w.target else {
                        return Err(invalid());
                    };
                    setup.answers.push(ContestAnswer {
                        nick,
                        message_id: reply.completed.generation.message_id,
                        text: reply.content,
                    });
                }
            }
            tx.execute(
                "UPDATE survey_sources SET setup_json=? WHERE contest_id=?",
                params![serde_json::to_string(&setup)?, setup.contest_id.to_string()],
            )?;
            let seq = group.push(
                tx,
                envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::SurveySetup(setup.clone()),
                ),
            )?;
            squad_store::contests::associate(tx, setup.contest_id, ContestKind::Survey, seq)?;
            if setup.answers.is_empty() {
                SurveyStatus::Failed
            } else {
                SurveyStatus::Extracting
            }
        }
        SurveyStatus::Extracting => {
            if failed {
                SurveyStatus::Failed
            } else if squad_store::survey::claims(tx, state.contest_id)?
                .claims
                .is_empty()
            {
                let selection = SurveySelection {
                    contest_id: state.contest_id,
                    step_id: commit.step_id,
                    claims: vec![],
                    tallies: vec![],
                };
                let seq = group.push(
                    tx,
                    envelope(
                        id,
                        Actor::System,
                        vec![Recipient::Me],
                        EnvelopePayload::SurveySelection(selection),
                    ),
                )?;
                squad_store::contests::associate(tx, state.contest_id, ContestKind::Survey, seq)?;
                SurveyStatus::Reporting
            } else {
                SurveyStatus::Voting
            }
        }
        SurveyStatus::Voting => {
            let selection = script::selection(
                &setup,
                commit.step_id,
                &squad_store::survey::claims(tx, state.contest_id)?,
                &squad_store::survey::ballots(tx, state.contest_id, false)?,
            );
            let seq = group.push(
                tx,
                envelope(
                    id,
                    Actor::System,
                    vec![Recipient::Me],
                    EnvelopePayload::SurveySelection(selection),
                ),
            )?;
            squad_store::contests::associate(tx, state.contest_id, ContestKind::Survey, seq)?;
            SurveyStatus::Revoting
        }
        SurveyStatus::Revoting => SurveyStatus::Reporting,
        SurveyStatus::Reporting => {
            if failed {
                SurveyStatus::Failed
            } else {
                SurveyStatus::Complete
            }
        }
        _ => return Err(invalid()),
    };
    if state.state == SurveyStatus::Failed {
        state.terminal_reason = Some(
            Text::new(if previous == SurveyStatus::Collecting {
                "No usable survey answers"
            } else {
                "Survey judge exhausted its attempts"
            })
            .expect("literal"),
        );
    }
    if state.state == SurveyStatus::Complete {
        let selected = squad_store::survey::selected(tx, state.contest_id)?
            .claims
            .into_iter()
            .map(|c| c.claim_id)
            .collect::<Vec<_>>();
        let completeness = script::completeness(
            &setup,
            &squad_store::survey::claims(tx, state.contest_id)?,
            &selected,
            &squad_store::survey::ballots(tx, state.contest_id, false)?,
            &squad_store::survey::ballots(tx, state.contest_id, true)?,
        );
        if !completeness.is_complete() {
            state.terminal_reason = Some(Text::new(completeness.summary()).map_err(|_| invalid())?);
        }
        state.completeness = Some(completeness);
    }
    let version = state.state_version;
    state.state_version += 1;
    state.step_id = None;
    save(tx, group, id, &state, version)
}

pub fn transition(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let ContestTransition::Survey(signal) = submission.transition else {
        return Err(invalid());
    };
    let mut state = squad_store::survey::state(tx, submission.contest_id)?;
    if state.state_version != submission.expected_state_version
        || state.state != SurveyStatus::WaitingForParticipants
        || signal != SurveyTransition::ParticipantsReady
    {
        return Err(invalid());
    }
    let setup = squad_store::survey::setup(tx, state.contest_id)?;
    let waiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work w JOIN survey_sources s ON s.contest_id=? WHERE w.kind='reply' AND w.step_id IS NULL AND w.held_by_contest_id IS NULL AND w.status IN ('queued','running') AND json_type(w.scheduling_meta,'$.openModeDormant') IS NULL AND w.target IN (SELECT value FROM json_each(s.setup_json,'$.participants')))",[state.contest_id.to_string()],|r|r.get(0))?;
    if waiting {
        return Err(invalid());
    }
    state.state = if setup.collect {
        SurveyStatus::Collecting
    } else {
        SurveyStatus::Extracting
    };
    let version = state.state_version;
    state.state_version += 1;
    save(tx, group, id, &state, version)
}
