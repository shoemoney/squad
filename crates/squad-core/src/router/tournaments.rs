//! Transactional tournament admission, versioned state and accepted outcomes.
use super::{envelope, error};
use crate::prompts::PromptPack;
use rusqlite::{Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid tournament transition",
    )
}
pub fn emit(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    payload: EnvelopePayload,
) -> Result<CommittedEnvelope> {
    let recipients = if let EnvelopePayload::TournamentResult(r) = &payload {
        vec![Recipient::Me, Recipient::Member(r.entrant.clone())]
    } else {
        vec![Recipient::Me]
    };
    let seq = group.push(tx, envelope(id, Actor::System, recipients, payload))?;
    squad_store::contests::associate(tx, contest, ContestKind::Tournament, seq)?;
    squad_store::events::at(tx, seq.get())
}
pub fn save(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    state: &TournamentState,
    expected: Option<u64>,
) -> Result<()> {
    state.validate().map_err(|_| invalid())?;
    if let Some(v) = expected {
        if tx.execute("UPDATE contests SET state_json=?,state_version=?,active=? WHERE contest_id=? AND kind='tournament' AND state_version=?",params![serde_json::to_string(state)?,state.state_version as i64,!state.state.is_terminal(),state.contest_id.to_string(),v as i64])?!=1 {return Err(invalid());}
    } else {
        tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active) VALUES (?,'tournament',(SELECT epoch FROM room_meta),?,?,1)",params![state.contest_id.to_string(),state.state_version as i64,serde_json::to_string(state)?])?;
    }
    let e = emit(
        tx,
        group,
        id,
        state.contest_id,
        EnvelopePayload::TournamentState(state.clone()),
    )?;
    tx.execute(
        "UPDATE contests SET updated_seq=? WHERE contest_id=?",
        params![e.commit.seq.get(), state.contest_id.to_string()],
    )?;
    if state.state.is_terminal() {
        let mut q=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
        let rows = q
            .query_map([state.contest_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (r, d, a) in rows {
            tx.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![a, d],
            )?;
            tx.execute("DELETE FROM reservations WHERE reservation_id=?", [r])?;
        }
        squad_store::history::release(tx, state.contest_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'",[state.contest_id.to_string()])?;
    }
    Ok(())
}

/// Frozen tournament steps. Round `r` has `k - r + 1` entrants answering from
/// their own history, the prompt and their earlier answers and results, then one
/// blind judge call over every answer; each logical round may be replayed once.
pub fn steps(
    pack: &PromptPack,
    entrants: &[MemberNick],
    prompt: &str,
    history: u64,
) -> Result<Vec<super::bounds::Step>> {
    use super::bounds::{
        Step, artifact, json_text, project, raw_templates, schema_bytes, template,
    };
    let k = entrants.len() as u32;
    let reply = constants::REPLY_VISIBLE_TOKENS;
    let judge = constants::FINAL_JUDGE_VISIBLE_TOKENS;
    let prompt = json_text(prompt) + 512;
    let entry = template(pack, "tournament.entry")?;
    let judge_template = raw_templates(pack, &["judge.system", "tournament.judge"])?;
    let judge_schema = schema_bytes(validation::schema::<TournamentJudgeResponse>());
    let mut out = vec![];
    for round in 1..k {
        let remaining = k - round + 1;
        out.push(Step {
            kind: WorkKind::Reply,
            target: entrants[0].to_string(),
            alternatives: entrants[1..].iter().map(ToString::to_string).collect(),
            template: entry,
            schema: 0,
            data: history + prompt + u64::from(round - 1) * (artifact(reply) + 1024),
            visible: reply,
            calls: remaining,
            attempts: 4,
        });
        out.push(Step {
            kind: WorkKind::JudgeTournament,
            target: "judge".into(),
            alternatives: vec![],
            template: judge_template,
            schema: judge_schema,
            data: prompt + u64::from(remaining) * project(artifact(reply)),
            visible: judge,
            calls: 1,
            attempts: 6,
        });
    }
    Ok(out)
}
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    conversation: &parser::ConversationalInput,
    entrants: &[MemberNick],
    prompt: &str,
    bindings: &BTreeMap<String, ExecutorBinding>,
    max: i64,
    threshold: f64,
    config: Option<&SquadConfig>,
    preconfirmed: bool,
) -> Result<RequestResult> {
    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get(0),
    )?;
    if active {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running",
        ));
    }
    if entrants.len() < 3 || !validation::unique(entrants) {
        return Err(StoreError::Domain(
            ErrorCode::TOURNAMENT_ENTRANTS_REQUIRED,
            "$tournament needs at least three entrants.",
        ));
    }
    super::variables::apply(tx, group, id, &conversation.variable_updates)?;
    let judge = bindings.get("judge").ok_or(StoreError::Domain(
        ErrorCode::MODEL_UNAVAILABLE,
        "Judge unavailable",
    ))?;
    if entrants.iter().any(|n| !bindings.contains_key(n.as_str())) {
        return Err(StoreError::Domain(
            ErrorCode::MODEL_UNAVAILABLE,
            "Entrant unavailable",
        ));
    }
    let contest = ContestId::new();
    let setup = TournamentSetup {
        contest_id: contest,
        prompt_message_id: input.message_id,
        prompt: prompt.into(),
        entrants: entrants.to_vec(),
        judge_model: judge.model.to_string(),
        total_rounds: entrants.len() as u32 - 1,
    };
    let mut state = TournamentState {
        contest_id: contest,
        state: TournamentStatus::Preflight,
        state_version: 0,
        round: 1,
        replay: 0,
        survivors: entrants.to_vec(),
        eliminated: vec![],
        terminal_reason: None,
    };
    save(tx, group, id, &state, None)?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "tournament.setup",
        &serde_json::to_value(&setup)?,
    )?;
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "tournament.bindings",
        &serde_json::to_value(bindings)?,
    )?;
    if let Some(config) = config {
        let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
            .map_err(|_| invalid())?;
        squad_store::contest_steps::private_map(
            tx,
            contest,
            "tournament.prompts",
            &serde_json::to_value(&pack)?,
        )?;
        let roster = serde_json::json!(
            config
                .agents
                .iter()
                .map(|a| a.nick.as_str())
                .collect::<Vec<_>>()
        );
        let mut systems = serde_json::Map::new();
        for n in entrants {
            let agent = config
                .agents
                .iter()
                .find(|a| &a.nick == n)
                .ok_or_else(invalid)?;
            let inputs = crate::prompts::Inputs::from([
                ("nick".into(), serde_json::json!(n)),
                (
                    "model_slug".into(),
                    serde_json::json!(bindings[n.as_str()].model),
                ),
                ("member_nicks_json".into(), roster.clone()),
                ("user_name_json".into(), serde_json::json!(config.user.name)),
                ("persona_json".into(), serde_json::json!(agent.persona)),
                (
                    "room_word_limit".into(),
                    serde_json::json!(config.prompts.limits.room_words),
                ),
            ]);
            systems.insert(
                n.to_string(),
                serde_json::json!(
                    pack.render_template("room.system", &inputs)
                        .map_err(|_| invalid())?
                ),
            );
        }
        let mut instructions = serde_json::Map::new();
        for round in 1..=setup.total_rounds {
            let mut s = state.clone();
            s.round = round;
            s.survivors.truncate(entrants.len() + 1 - round as usize);
            let inputs = pack.tournament_entry_values(&s, config.prompts.limits.room_words as u16);
            instructions.insert(
                round.to_string(),
                serde_json::json!(
                    pack.render_template("tournament.entrant", &inputs)
                        .map_err(|_| invalid())?
                ),
            );
        }
        squad_store::contest_steps::private_map(
            tx,
            contest,
            "tournament.member_prompts",
            &serde_json::json!({"systems":systems,"instructions":instructions}),
        )?;
    }
    let mut tokens = Vec::new();
    if let Some(c) = config {
        for a in &c.agents {
            tokens.extend([a.nick.to_string(), a.model.clone(), a.persona.clone()]);
        }
    }
    for binding in bindings.values() {
        tokens.push(binding.model.to_string());
    }
    for token in tokens.clone() {
        tokens.extend(
            token
                .split(['/', '-', '_', '.', ':'])
                .filter(|p| !p.is_empty())
                .map(str::to_owned),
        );
    }
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "tournament.identities",
        &serde_json::json!({"tokens":tokens}),
    )?;
    let (normal, reserved) = crate::orchestrator::tournament::call_counts(entrants.len() as u32);
    let pack = PromptPack::from_override_dir(config.and_then(|c| c.prompts.directory.as_deref()))
        .map_err(|_| invalid())?;
    let mut history = 0_u64;
    for n in entrants {
        let len: i64 = tx.query_row(
            "SELECT COALESCE(SUM(length(content)),0) FROM history WHERE context_id=?",
            [n.as_str()],
            |r| r.get(0),
        )?;
        history = history.max(len as u64);
    }
    let priced = super::bounds::price(
        tx,
        bindings,
        &steps(&pack, entrants, setup.prompt.as_str(), history)?,
    )?;
    let estimated = priced.estimated_usd;
    let reserve = priced.reserved_usd();
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let used = squad_store::accounting::daily(tx, &day)?;
    let plan = TournamentPreflight {
        base: ContestPreflight {
            contest_id: contest,
            kind: ContestKind::Tournament,
            plan_id: PlanId::new(),
            expires_at: group.accepted_at
                + chrono::Duration::milliseconds(
                    squad_protocol::constants::PREFLIGHT_EXPIRY_MS as i64,
                ),
            estimate: DebateEstimate {
                estimated_usd: estimated,
                reserved_usd: reserve,
                remaining_usd: (max - used.spent_micros - used.reserved_micros).max(0) as f64 / 1e6,
                model_calls: normal,
            },
            participants: entrants.to_vec(),
            confirmation_required: estimated > threshold,
            warning: None,
            settings: {
                let mut settings = priced.settings();
                settings["reservedCalls"] = reserved.into();
                settings["replays"] = 1.into();
                settings["entryAttempts"] = 2.into();
                settings["judgeAttempts"] = 3.into();
                settings
            },
        },
        total_rounds: setup.total_rounds,
        entry_attempts: 2,
        judge_attempts: 3,
        replays: 1,
    };
    tx.execute("UPDATE contests SET plan_id=?,plan_json=?,pending_request_json=?,expires_at=? WHERE contest_id=?",params![plan.base.plan_id.to_string(),serde_json::to_string(&plan)?,serde_json::json!({"input":input,"admissionMaxMicros":max,"confirmation":if preconfirmed {Confirmation::Preconfirmed}else if estimated>threshold {Confirmation::Explicit}else{Confirmation::Automatic}}).to_string(),plan.base.expires_at.to_rfc3339(),contest.to_string()])?;
    let e = emit(
        tx,
        group,
        id,
        contest,
        EnvelopePayload::TournamentPreflight(plan.clone()),
    )?;
    if let Err(StoreError::Domain(code, message)) =
        squad_store::accounting::admit(tx, &day, priced.reserved_micros, max)
    {
        let rejection = if code == ErrorCode::DAILY_MAX_REACHED {
            super::daily_cap_error(tx, &day, priced.reserved_micros, max)
        } else {
            error(code, message)
        };
        emit(
            tx,
            group,
            id,
            contest,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: contest,
                plan_id: plan.base.plan_id,
                status: PreflightStatus::Rejected,
                confirmation: None,
                error: Some(rejection.clone()),
            }),
        )?;
        state.state = TournamentStatus::Failed;
        state.state_version += 1;
        state.terminal_reason = Some(message.into());
        save(tx, group, id, &state, Some(0))?;
        return Ok(RequestResult::Rejected {
            request_id: id,
            error: rejection,
        });
    }
    if plan.base.confirmation_required && !preconfirmed {
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(plan.base),
            cursor: e.commit.seq,
        });
    }
    resolve(tx, group, id, contest, plan.base.plan_id, false, max)
}

enum PreflightDecision {
    Confirm,
    Cancel,
    Expire,
}

pub(crate) fn expire(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan_id: PlanId,
) -> Result<RequestResult> {
    resolve_preflight(
        tx,
        group,
        id,
        contest,
        plan_id,
        PreflightDecision::Expire,
        0,
    )
}

pub fn resolve(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan_id: PlanId,
    cancel: bool,
    current_max: i64,
) -> Result<RequestResult> {
    let decision = if cancel {
        PreflightDecision::Cancel
    } else {
        PreflightDecision::Confirm
    };
    resolve_preflight(tx, group, id, contest, plan_id, decision, current_max)
}

fn resolve_preflight(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    plan_id: PlanId,
    decision: PreflightDecision,
    current_max: i64,
) -> Result<RequestResult> {
    let mut state = squad_store::tournaments::state(tx, contest)?;
    let raw: String = tx.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let plan: TournamentPreflight = serde_json::from_str(&raw)?;
    if state.state != TournamentStatus::Preflight || plan_id != plan.base.plan_id {
        return Err(invalid());
    }
    let status = if plan.base.expires_at <= group.accepted_at {
        PreflightStatus::Expired
    } else {
        match decision {
            PreflightDecision::Confirm => PreflightStatus::Confirmed,
            PreflightDecision::Cancel => PreflightStatus::Cancelled,
            PreflightDecision::Expire => return Err(invalid()),
        }
    };
    let raw: String = tx.query_row(
        "SELECT pending_request_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let pending: serde_json::Value = serde_json::from_str(&raw)?;
    let confirmation: Confirmation = serde_json::from_value(pending["confirmation"].clone())?;
    if status == PreflightStatus::Confirmed {
        let max = pending["admissionMaxMicros"]
            .as_i64()
            .ok_or_else(invalid)?
            .min(current_max);
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        if let Err(failure) = squad_store::accounting::reserve(
            tx,
            &format!("plan:{plan_id}"),
            None,
            &day,
            (plan.base.estimate.reserved_usd * 1e6).ceil() as i64,
            max,
        ) {
            let StoreError::Domain(code, message) = failure else {
                return Err(failure);
            };
            let old = state.state_version;
            state.state_version += 1;
            state.state = TournamentStatus::Failed;
            state.terminal_reason = Some(message.into());
            save(tx, group, id, &state, Some(old))?;
            let mut failure = error(code, message);
            failure.contest_id = Some(contest);
            emit(
                tx,
                group,
                id,
                contest,
                EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                    contest_id: contest,
                    plan_id,
                    status: PreflightStatus::Rejected,
                    confirmation: None,
                    error: Some(failure.clone()),
                }),
            )?;
            return Ok(RequestResult::Rejected {
                request_id: id,
                error: failure,
            });
        }
    }
    let confirmed = status == PreflightStatus::Confirmed;
    let terminal_reason = match status {
        PreflightStatus::Expired => Some("expired"),
        PreflightStatus::Cancelled => Some("cancelled"),
        _ => None,
    };
    emit(
        tx,
        group,
        id,
        contest,
        EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
            contest_id: contest,
            plan_id,
            confirmation: confirmed.then_some(confirmation),
            status,
            error: None,
        }),
    )?;
    let old = state.state_version;
    state.state_version += 1;
    if let Some(reason) = terminal_reason {
        state.state = TournamentStatus::Stopped;
        state.terminal_reason = Some(reason.into());
    } else {
        let v = pending;
        tx.execute(
            "UPDATE reservations SET contest_id=?,plan_id=? WHERE reservation_id=?",
            params![
                contest.to_string(),
                plan_id.to_string(),
                format!("plan:{plan_id}")
            ],
        )?;
        state.state = TournamentStatus::WaitingForEntrants;
        let setup = squad_store::tournaments::setup(tx, contest)?;
        let mut input: UserMessageInput = serde_json::from_value(v["input"].clone())?;
        input.quote = None;
        let mut user = envelope(
            id,
            Actor::Me,
            vec![Recipient::Me],
            EnvelopePayload::UserMessage(UserMessage {
                input: input.clone(),
                body: setup.prompt.clone(),
                resolved_quotes: vec![],
                debate_id: None,
                case_id: None,
                contest_id: Some(contest),
            }),
        );
        user.base.id = EventId::from_str(&input.message_id.to_string()).map_err(|_| invalid())?;
        let seq = group.push(tx, user)?;
        squad_store::contests::associate(tx, contest, ContestKind::Tournament, seq)?;
        let e = emit(
            tx,
            group,
            id,
            contest,
            EnvelopePayload::TournamentSetup(setup.clone()),
        )?;
        for n in setup.entrants {
            squad_store::history::project(
                tx,
                &squad_store::history::attributed(
                    &e,
                    ContextId(n.to_string()),
                    "tournament.setup",
                    HistoryRole::User,
                    Some("squad".into()),
                    setup.prompt.clone(),
                ),
            )?;
        }
    }
    save(tx, group, id, &state, Some(old))?;
    Ok(group.accepted(id))
}
use std::str::FromStr;
/// Accepting a ranking, own results, one elimination and a possible champion
/// is one writer transaction. The unique round row is the duplicate gate.
pub fn accept(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    work: &Work,
    judgment: &TournamentJudgment,
    _pack: &PromptPack,
) -> Result<()> {
    let frozen:String=tx.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='tournament.prompts'",[judgment.contest_id.to_string()],|r|r.get(0))?;
    let frozen: PromptPack = serde_json::from_str(&frozen)?;
    let pack = &frozen;
    let mut state = squad_store::tournaments::validate_execution(tx, work)?;
    if judgment.contest_id != state.contest_id
        || judgment.round != state.round
        || judgment.replay != state.replay
        || Some(judgment.case_id) != work.case_id
    {
        return Err(invalid());
    }
    let ranking = squad_store::tournaments::decode(tx, work, &judgment.response)?;
    crate::orchestrator::tournament::step(tx, group, id, &state, ContestMove::Tally)?;
    let e = emit(
        tx,
        group,
        id,
        state.contest_id,
        EnvelopePayload::TournamentRanking(ranking.clone()),
    )?;
    if tx.execute("UPDATE tournament_rounds SET ranking_event_seq=? WHERE contest_id=? AND round=? AND replay=? AND ranking_event_seq IS NULL AND judge_work_id=?",params![e.commit.seq.get(),state.contest_id.to_string(),state.round,state.replay,work.work_id.to_string()])?!=1{return Err(invalid());}
    squad_store::work::finish(tx, work.work_id, WorkStatus::Completed, e.commit.seq)?;
    squad_store::cases::seal(tx, judgment.case_id, e.commit.seq)?;
    let old = state.state_version;
    state.state = TournamentStatus::Eliminating;
    state.state_version += 1;
    save(tx, group, id, &state, Some(old))?;
    let entries =
        squad_store::tournaments::entries(tx, state.contest_id, state.round, state.replay)?;
    for row in &ranking.ranking {
        let result = TournamentResult {
            contest_id: state.contest_id,
            round: state.round,
            entrant: row.nick.clone(),
            rank: row.rank,
            remaining_before: ranking.ranking.len() as u32,
            score: row.score,
            critique: row.critique.clone(),
            survived: row.rank < ranking.ranking.len() as u32,
        };
        let e = emit(
            tx,
            group,
            id,
            state.contest_id,
            EnvelopePayload::TournamentResult(result.clone()),
        )?;
        let entry = entries
            .iter()
            .find(|(_, e)| e.entrant == row.nick)
            .ok_or_else(invalid)?;
        tx.execute(
            "INSERT INTO tournament_results VALUES (?,?,?,?,?,?,?,?,?)",
            params![
                state.contest_id.to_string(),
                state.round,
                row.nick.as_str(),
                entry.0.get(),
                e.commit.seq.get(),
                row.rank,
                row.score,
                row.critique,
                !result.survived
            ],
        )?;
        // Only this entrant's result notice enters its capability.
        let content = pack.tournament_result(&result).map_err(|_| invalid())?;
        squad_store::history::project(
            tx,
            &squad_store::history::attributed(
                &e,
                ContextId(row.nick.to_string()),
                "tournament.result",
                HistoryRole::User,
                Some("squad".into()),
                content,
            ),
        )?;
    }
    let barrier =
        crate::orchestrator::tournament::step(tx, group, id, &state, ContestMove::Barrier)?;
    let event = emit(
        tx,
        group,
        id,
        state.contest_id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: state.contest_id,
            step_id: barrier,
            artifact_event_ids: vec![],
            skipped_work_ids: vec![],
            data: serde_json::json!({"round":state.round,"replay":state.replay}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, barrier, event.commit.seq)?;
    let loser = ranking.ranking.last().ok_or_else(invalid)?.nick.clone();
    state.survivors.retain(|n| n != &loser);
    state.eliminated.push(loser);
    if state.survivors.len() == 1 {
        crate::orchestrator::tournament::step(tx, group, id, &state, ContestMove::Report)?;
        let champion = state.survivors[0].clone();
        tx.execute("INSERT INTO scores(nick,wins,losses,trophies) VALUES (?,0,0,1) ON CONFLICT(nick) DO UPDATE SET trophies=trophies+1",[champion.as_str()])?;
        let trophies: i64 = tx.query_row(
            "SELECT trophies FROM scores WHERE nick=?",
            [champion.as_str()],
            |r| r.get(0),
        )?;
        let mut placements = vec![TournamentPlacement {
            nick: champion.clone(),
            placement: 1,
        }];
        let total = state.eliminated.len() as u32 + 1;
        placements.extend(
            state
                .eliminated
                .iter()
                .enumerate()
                .map(|(i, n)| TournamentPlacement {
                    nick: n.clone(),
                    placement: total - i as u32,
                }),
        );
        placements.sort_by_key(|p| p.placement);
        let champion = TournamentChampion {
            contest_id: state.contest_id,
            champion,
            placements,
            trophies: trophies as u64,
        };
        let e = emit(
            tx,
            group,
            id,
            state.contest_id,
            EnvelopePayload::TournamentChampion(champion.clone()),
        )?;
        let content = pack.tournament_final(&champion).map_err(|_| invalid())?;
        for p in &champion.placements {
            squad_store::history::project(
                tx,
                &squad_store::history::attributed(
                    &e,
                    ContextId(p.nick.to_string()),
                    "tournament.final",
                    HistoryRole::User,
                    Some("squad".into()),
                    content.clone(),
                ),
            )?;
        }
        state.state = TournamentStatus::Complete;
    } else {
        state.state = TournamentStatus::Answering;
        state.round += 1;
        state.replay = 0;
    }
    let old = state.state_version;
    state.state_version += 1;
    save(tx, group, id, &state, Some(old))
}
