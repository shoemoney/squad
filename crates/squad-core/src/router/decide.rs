//! Transactional Decide admission, frozen projections and step barriers.
use crate::{orchestrator::decide as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};
use squad_protocol::{decide::*, *};
use squad_store::{Result, StoreError, commit::CommitGroup, decide as storage};
use std::collections::BTreeMap;
pub fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid decide transition",
    )
}
fn push(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    c: ContestId,
    p: EnvelopePayload,
) -> Result<Cursor> {
    let seq = g.push(
        tx,
        super::envelope(id, Actor::System, vec![Recipient::Me], p),
    )?;
    squad_store::contests::associate(tx, c, ContestKind::Decide, seq)?;
    Ok(seq)
}
fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &DecideState,
    old: Option<u64>,
) -> Result<()> {
    let seq = g.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::DecideState(s.clone()),
        ),
    )?;
    storage::save(tx, s, old, seq)?;
    if s.state.is_terminal() {
        squad_store::history::release(tx, s.contest_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'",[s.contest_id.to_string()])?;
        let mut q = tx.prepare(
            "SELECT work_id FROM work WHERE contest_id=? AND status IN ('queued','running')",
        )?;
        let ids = q
            .query_map([s.contest_id.to_string()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for w in ids {
            squad_store::accounting::release_unstarted_work(tx, w.parse().map_err(|_| invalid())?)?;
        }
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[s.contest_id.to_string()])?;
        let mut q=tx.prepare("SELECT reservation_id,day,amount_micros FROM reservations WHERE contest_id=? AND work_id IS NULL AND unresolved=0")?;
        let rows = q
            .query_map([s.contest_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (rid, day, amount) in rows {
            tx.execute(
                "UPDATE spend_daily SET reserved_micros=reserved_micros-? WHERE day=?",
                params![amount, day],
            )?;
            tx.execute("DELETE FROM reservations WHERE reservation_id=?", [rid])?;
        }
    }
    Ok(())
}
pub fn steps(
    participants: &[MemberNick],
    decision: &str,
    generated: bool,
    templates: &[u64],
) -> Vec<super::bounds::Step> {
    use super::bounds::*;
    let m = participants.len() as u64;
    let mut out = vec![];
    for (i, (kind, judge, data, visible)) in [
        (WorkKind::DecideOptions, false, json_text(decision), 2048),
        (
            WorkKind::DecideShortlist,
            true,
            blinded(decision) + m * project(limited(2048, 100)),
            4096,
        ),
        (
            WorkKind::DecideCriteria,
            false,
            json_text(decision) + 4 * limited(2048, 50),
            2048,
        ),
        (
            WorkKind::DecideFreeze,
            true,
            blinded(decision) + m * project(limited(2048, 300)),
            4096,
        ),
        (
            WorkKind::DecideScore,
            false,
            json_text(decision) + 4 * limited(2048, 50) + 7 * limited(2048, 60),
            4096,
        ),
        (
            WorkKind::DecideExplain,
            true,
            blinded(decision) + m * project(limited(4096, 700)) + 4 * ARTIFACT_FRAME,
            4096,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if i < 2 && !generated {
            continue;
        }
        let targets = if judge {
            vec!["judge".into()]
        } else {
            participants.iter().map(ToString::to_string).collect()
        };
        for target in targets {
            out.push(Step {
                kind: kind.clone(),
                target,
                alternatives: vec![],
                template: templates[i],
                schema: schema_bytes(output_schema(&kind).expect("mode")),
                data,
                visible,
                calls: 1,
                attempts: if judge { 3 } else { 2 },
            });
        }
    }
    out
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &super::NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::Decide {
        participants,
        decision,
        options,
        input: conversation,
    } = parsed
    else {
        return Err(invalid());
    };
    let busy: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get(0),
    )?;
    if busy {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let config = a.config.ok_or_else(invalid)?;
    let pack = PromptPack::from_override_dir(config.prompts.directory.as_deref())
        .map_err(|_| invalid())?;
    let kinds = [
        WorkKind::DecideOptions,
        WorkKind::DecideShortlist,
        WorkKind::DecideCriteria,
        WorkKind::DecideFreeze,
        WorkKind::DecideScore,
        WorkKind::DecideExplain,
    ];
    let mut prompts = json!({});
    let mut templates = vec![];
    for k in &kinds {
        let name = k.new_mode_composition().expect("mode");
        let (sys, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        prompts[name] = json!({"system":sys,"user":user,"reference":PromptRef{pack_version:Text::new(pack.version()).map_err(|_|invalid())?,template_id:Text::new(name).map_err(|_|invalid())?,template_hash:Text::new(squad_store::commit::digest(&(sys,user))?).map_err(|_|invalid())?}});
        templates.push(super::bounds::template(&pack, name)?);
    }
    let mut models = a.bindings.clone();
    if let Some(j) = models.get_mut("judge") {
        j.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
    }
    let setup = DecideSetup {
        decision: decision.clone(),
        options: options
            .iter()
            .enumerate()
            .map(|(i, s)| DecideOption {
                option_id: format!("o{}", i + 1),
                name: s.clone(),
                description: String::new(),
            })
            .collect(),
        participants: participants.clone(),
        models,
        prompts,
        redactions: config
            .agents
            .iter()
            .flat_map(|x| [x.nick.to_string(), x.model.clone(), x.persona.clone()])
            .chain(std::iter::once(config.judge.model.clone()))
            .collect(),
    };
    let mut step_bounds = steps(participants, decision, options.is_empty(), &templates);
    // Supplied options are literal user input, so account for long lines exactly.
    let actual = serde_json::to_string(&setup.options)?.len() as u64;
    let extra = actual.saturating_sub(4 * super::bounds::limited(2048, 50));
    for step in &mut step_bounds {
        if matches!(
            step.kind,
            WorkKind::DecideCriteria | WorkKind::DecideScore | WorkKind::DecideExplain
        ) {
            step.data += extra;
        }
    }
    let priced = super::bounds::price(tx, &setup.models, &step_bounds)?;
    let (calls, attempts) = script::counts(participants.len() as u32, options.is_empty());
    let mut settings = priced.settings();
    settings["reservedAttempts"] = attempts.into();
    settings["memberCalls"] =
        ((2 + u32::from(options.is_empty())) * participants.len() as u32).into();
    settings["judgeCalls"] = (2 + u32::from(options.is_empty())).into();
    let c = ContestId::new();
    let s = DecideState {
        contest_id: c,
        state_version: 1,
        state: DecideStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    save(tx, g, id, &s, None)?;
    storage::put(tx, c, "setup", &setup)?;
    storage::put(tx, c, "options", &setup.options)?;
    storage::put(tx, c, "skipped", &Vec::<String>::new())?;
    let mut labels = participants
        .iter()
        .map(|n| (CellId::new().to_string(), n.to_string()))
        .collect::<Vec<_>>();
    labels.sort();
    let labels = labels
        .into_iter()
        .enumerate()
        .map(|(i, (_, n))| (n, format!("Member{}", i + 1)))
        .collect::<BTreeMap<_, _>>();
    storage::put(tx, c, "labels", &labels)?;
    squad_store::contest_steps::private_map(tx, c, "decide.labels", &json!({"labels":labels}))?;
    push(
        tx,
        g,
        id,
        c,
        EnvelopePayload::DecideSetup(ContestSetup {
            contest_id: c,
            kind: ContestKind::Decide,
            script_version: Text::new("decide-v1").expect("literal"),
            participants: participants.clone(),
            source_event_ids: vec![],
            settings: json!({"decision":decision,"options":setup.options}),
        }),
    )?;
    super::variables::apply(tx, g, id, &conversation.variable_updates)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let daily = squad_store::accounting::daily(tx, &day)?;
    let plan = ContestPreflight {
        contest_id: c,
        kind: ContestKind::Decide,
        plan_id: PlanId::new(),
        expires_at: g.accepted_at
            + chrono::Duration::milliseconds(constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: priced.estimated_usd,
            reserved_usd: priced.reserved_usd(),
            remaining_usd: a
                .daily_max
                .saturating_sub(daily.spent_micros + daily.reserved_micros)
                as f64
                / 1e6,
            model_calls: calls,
        },
        participants: participants.clone(),
        confirmation_required: priced.estimated_usd > a.confirmation_threshold,
        warning: None,
        settings,
    };
    let seq = push(
        tx,
        g,
        id,
        c,
        EnvelopePayload::ContestPreflight(plan.clone()),
    )?;
    tx.execute("UPDATE contests SET plan_id=?,plan_json=?,expires_at=?,pending_request_json=? WHERE contest_id=?",params![plan.plan_id.to_string(),serde_json::to_string(&plan)?,plan.expires_at.to_rfc3339(),json!({"input":input,"max":a.daily_max,"preconfirmed":a.preconfirmed}).to_string(),c.to_string()])?;
    if priced.reserved_micros
        > a.daily_max
            .saturating_sub(daily.spent_micros + daily.reserved_micros)
    {
        let mut s = s;
        s.state = DecideStatus::Failed;
        s.state_version += 1;
        s.terminal_reason = Some(Text::new("daily_max_reached").expect("literal"));
        let mut error = super::daily_cap_error(tx, &day, priced.reserved_micros, a.daily_max);
        error.contest_id = Some(c);
        push(
            tx,
            g,
            id,
            c,
            EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
                contest_id: c,
                plan_id: plan.plan_id,
                status: PreflightStatus::Rejected,
                confirmation: None,
                error: Some(error.clone()),
            }),
        )?;
        save(tx, g, id, &s, Some(1))?;
        return Ok(RequestResult::Rejected {
            request_id: id,
            error,
        });
    }
    if !plan.confirmation_required || a.preconfirmed {
        resolve(tx, g, id, c, plan.plan_id, false)
    } else {
        Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(plan),
            cursor: seq,
        })
    }
}
pub fn resolve(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    c: ContestId,
    p: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let mut s = load_state(tx, c)?;
    let (raw, pending): (String, String) = tx.query_row(
        "SELECT plan_json,pending_request_json FROM contests WHERE contest_id=?",
        [c.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let plan: ContestPreflight = serde_json::from_str(&raw)?;
    let pending: Value = serde_json::from_str(&pending)?;
    if s.state != DecideStatus::Preflight || plan.plan_id != p {
        return Err(invalid());
    }
    let expired = g.accepted_at >= plan.expires_at;
    let old = s.state_version;
    s.state_version += 1;
    if cancel || expired {
        s.state = DecideStatus::Stopped;
        s.terminal_reason =
            Some(Text::new(if expired { "expired" } else { "cancelled" }).expect("literal"));
    } else {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        squad_store::accounting::reserve(
            tx,
            &format!("plan:{p}"),
            None,
            &day,
            squad_store::accounting::reservation_micros(plan.estimate.reserved_usd)
                .map_err(|_| invalid())?,
            pending["max"].as_i64().ok_or_else(invalid)?,
        )?;
        tx.execute(
            "UPDATE reservations SET plan_id=?,contest_id=? WHERE reservation_id=?",
            params![p.to_string(), c.to_string(), format!("plan:{p}")],
        )?;
        s.state = DecideStatus::WaitingForParticipants;
    }
    push(
        tx,
        g,
        id,
        c,
        EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
            contest_id: c,
            plan_id: p,
            status: if expired {
                PreflightStatus::Expired
            } else if cancel {
                PreflightStatus::Cancelled
            } else {
                PreflightStatus::Confirmed
            },
            confirmation: (!cancel && !expired).then_some(if pending["preconfirmed"] == true {
                Confirmation::Preconfirmed
            } else if plan.confirmation_required {
                Confirmation::Explicit
            } else {
                Confirmation::Automatic
            }),
            error: None,
        }),
    )?;
    save(tx, g, id, &s, Some(old))?;
    Ok(g.accepted(id))
}
pub fn load_state(db: &Connection, c: ContestId) -> Result<DecideState> {
    storage::state(db, c)
}
pub fn recover(_db: &Connection, _c: ContestId) -> Result<()> {
    Ok(())
}
pub fn human_answer(_db: &Connection, _p: &parser::PendingHuman, _complete: bool) -> Result<()> {
    Err(invalid())
}
pub fn stop(tx: &Transaction<'_>, g: &mut CommitGroup, id: EventId, c: ContestId) -> Result<()> {
    let mut s = load_state(tx, c)?;
    let old = s.state_version;
    s.state_version += 1;
    s.state = DecideStatus::Stopped;
    s.terminal_reason = Some(Text::new("stopped").expect("literal"));
    save(tx, g, id, &s, Some(old))
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let s = load_state(tx, submission.contest_id)?;
    if submission.kind != ContestKind::Decide
        || s.state_version != submission.expected_state_version
        || !matches!(
            submission.transition,
            ContestTransition::Decide(DecideTransition::Advance)
        )
    {
        return Err(invalid());
    }
    advance(tx, g, id, s.contest_id)
}
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
pub fn barrier(_tx: &Transaction<'_>, _g: &mut CommitGroup, _b: BarrierCommit) -> Result<()> {
    Err(invalid())
}
pub fn accepted_work(db: &Connection, w: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    let s = load_state(db, a.contest_id)?;
    if s.state.is_terminal()
        || s.step_id != w.step_id
        || a.artifact_kind.as_str() != w.kind.new_mode_composition().ok_or_else(invalid)?
        || a.audience != Some(ArtifactAudience::Human)
    {
        return Err(invalid());
    }
    let frozen: Value = storage::get(db, a.contest_id, &format!("input:{}", w.work_id))?;
    let body = a.body.as_ref().ok_or_else(invalid)?;
    validate_output(&w.kind, &frozen["data"], body).map_err(|_| invalid())?;
    if prose_words(body) > word_limit(&w.kind) {
        return Err(invalid());
    }
    squad_store::contest_steps::artifact(db, a, seq)
}
fn word_limit(k: &WorkKind) -> usize {
    match k {
        WorkKind::DecideOptions => 100,
        WorkKind::DecideShortlist | WorkKind::DecideCriteria => 300,
        WorkKind::DecideFreeze => 500,
        WorkKind::DecideScore => 700,
        _ => 1200,
    }
}
fn step_kind(s: DecideStatus) -> Option<WorkKind> {
    Some(match s {
        DecideStatus::ProposingOptions => WorkKind::DecideOptions,
        DecideStatus::Shortlisting => WorkKind::DecideShortlist,
        DecideStatus::ProposingCriteria => WorkKind::DecideCriteria,
        DecideStatus::Freezing => WorkKind::DecideFreeze,
        DecideStatus::Scoring => WorkKind::DecideScore,
        DecideStatus::Explaining => WorkKind::DecideExplain,
        _ => return None,
    })
}
fn blind(mut v: Value, setup: &DecideSetup) -> Value {
    let mut tokens = setup.redactions.clone();
    tokens.extend(setup.participants.iter().map(ToString::to_string));
    tokens.extend(setup.models.values().map(|b| b.model.to_string()));
    for slug in tokens.clone() {
        if slug.contains('/') {
            tokens.extend(
                slug.split(['/', '-', '_', '.', '~'])
                    .filter(|p| p.chars().any(char::is_alphabetic))
                    .map(str::to_owned),
            );
        }
    }
    tokens.retain(|s| !s.trim().is_empty());
    tokens.sort_by_key(|s| std::cmp::Reverse(s.len()));
    tokens.dedup();
    if tokens.is_empty() {
        return v;
    }
    let pattern = format!(
        r"(?i)\b(?:{})\b",
        tokens
            .iter()
            .map(|t| regex::escape(t.trim_start_matches('~')))
            .collect::<Vec<_>>()
            .join("|")
    );
    let re = regex::Regex::new(&pattern).expect("escaped tokens");
    fn walk(v: &mut Value, re: &regex::Regex) {
        match v {
            Value::String(s) => *s = re.replace_all(s, "[redacted]").into_owned(),
            Value::Array(a) => {
                for v in a {
                    walk(v, re)
                }
            }
            Value::Object(o) => {
                for (k, v) in o {
                    if !matches!(
                        k.as_str(),
                        "memberLabel" | "localId" | "optionId" | "criterionId" | "rivalOptionId"
                    ) {
                        walk(v, re)
                    }
                }
            }
            _ => {}
        }
    }
    walk(&mut v, &re);
    v
}

fn projection(db: &Connection, c: ContestId, k: &WorkKind, setup: &DecideSetup) -> Result<Value> {
    let decision = blind(json!(setup.decision), setup);
    let options: Vec<DecideOption> = storage::get(db, c, "options")?;
    Ok(match k {
        WorkKind::DecideOptions => json!({"decision":decision}),
        WorkKind::DecideShortlist => {
            json!({"decision":decision,"proposals":storage::get::<Value>(db,c,"option_proposals")?})
        }
        WorkKind::DecideCriteria => json!({"decision":decision,"options":options}),
        WorkKind::DecideFreeze => {
            json!({"decision":decision,"proposals":storage::get::<Value>(db,c,"criteria_proposals")?})
        }
        WorkKind::DecideScore => {
            let r: DecideRubric = storage::get(db, c, "rubric")?;
            json!({"decision":decision,"options":options,"criteria":r.kept.iter().map(|x|json!({"criterionId":x.criterion.criterion_id,"name":x.criterion.name,"definition":x.criterion.definition})).collect::<Vec<_>>()})
        }
        WorkKind::DecideExplain => {
            json!({"decision":decision,"options":options,"rubric":storage::get::<Value>(db,c,"rubric")?,"matrix":storage::get::<Value>(db,c,"matrix")?,"evidence":storage::get::<Value>(db,c,"score_proposals")?})
        }
        _ => return Err(invalid()),
    })
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut DecideState,
    setup: &DecideSetup,
) -> Result<()> {
    let k = step_kind(s.state).ok_or_else(invalid)?;
    let judge = k.new_mode_judge();
    let name = k.new_mode_composition().expect("mode");
    let step = StepId::new();
    let c = s.contest_id;
    let case = if judge {
        let saved: Option<CaseId> = storage::optional(tx, c, "case")?;
        Some(match saved {
            Some(v) => v,
            None => {
                let v = CaseId::new();
                storage::put(tx, c, "case", &v)?;
                v
            }
        })
    } else {
        None
    };
    let targets = if judge {
        vec![Target::Judge]
    } else {
        setup
            .participants
            .iter()
            .cloned()
            .map(Target::Member)
            .collect()
    };
    let mut data = projection(tx, c, &k, setup)?;
    let mut items = vec![];
    if k == WorkKind::DecideScore {
        let mut order = data["options"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .map(|x| (CellId::new().to_string(), x.clone()))
            .collect::<Vec<_>>();
        order.sort_by(|a, b| a.0.cmp(&b.0));
        data["options"] = json!(order.into_iter().map(|(_, v)| v).collect::<Vec<_>>());
    }
    let labels: BTreeMap<String, String> = storage::get(tx, c, "labels")?;
    for target in targets {
        let context = ContextId(if let Some(case) = case {
            format!("judge:{case}")
        } else {
            format!("contest:{c}:{step}:{target}")
        });
        let binding = setup
            .models
            .get(&target.to_string())
            .ok_or_else(invalid)?
            .clone();
        let mut data = data.clone();
        if k == WorkKind::DecideScore {
            let rank = labels[&target.to_string()]
                .trim_start_matches("Member")
                .parse::<usize>()
                .map_err(|_| invalid())?;
            let order = data["options"].as_array_mut().ok_or_else(invalid)?;
            let len = order.len();
            order.rotate_left((rank - 1) % len);
            squad_store::contest_steps::private_map(
                tx,
                c,
                &format!("decide.order:{target}"),
                &json!({"options":data["options"]}),
            )?;
        }
        data = blind(data, setup);
        let schema = output_schema(&k).ok_or_else(invalid)?;
        let source = &setup.prompts[name];
        let mut values = BTreeMap::from([
            ("mode_data_json".into(), data.to_string()),
            ("mode_schema_json".into(), schema.to_string()),
            (
                "nick".into(),
                if judge {
                    "judge".into()
                } else {
                    labels[&target.to_string()].clone()
                },
            ),
            ("model_slug".into(), binding.model.to_string()),
            ("judge_model_slug".into(), binding.model.to_string()),
            ("member_nicks_json".into(), "[]".into()),
            ("user_name_json".into(), "\"the user\"".into()),
            ("persona_json".into(), "\"\"".into()),
            ("room_word_limit".into(), word_limit(&k).to_string()),
        ]);
        // Frozen substitutions are single pass, so user text cannot inject placeholders.
        let system = crate::prompts::inputs_census::render_source(
            source["system"].as_str().ok_or_else(invalid)?,
            &values,
        )?;
        let user = crate::prompts::inputs_census::render_source(
            source["user"].as_str().ok_or_else(invalid)?,
            &values,
        )?;
        values.clear();
        let (length, max_output, params_json): (i64, Option<u32>, String) = tx.query_row(
            "SELECT context_length,max_output,supported_params_json FROM catalog WHERE slug=?",
            [binding.model.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let supported: Value = serde_json::from_str(&params_json)?;
        let frozen = json!({"data":data,"system":system,"user":user,"contextLength":length,"maxOutput":max_output.unwrap_or(8192),"reasoning":supported.as_array().is_some_and(|a|a.iter().any(|x|x=="reasoning")),"wordLimit":word_limit(&k)});
        let w = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: k.clone(),
            target,
            context_id: context,
            trigger_event_id: id,
            history_through: HistoryPosition::new(1).expect("positive"),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: case,
            prompt_ref: Some(serde_json::from_value(source["reference"].clone())?),
            max_output_tokens: if k == WorkKind::DecideScore {
                None
            } else {
                Some(if judge { 4096 } else { 2048 })
            },
            contest_id: Some(c),
            step_id: Some(step),
        };
        items.push((w, binding, frozen));
    }
    let ordinal = match k {
        WorkKind::DecideOptions => 1,
        WorkKind::DecideShortlist => 2,
        WorkKind::DecideCriteria => 3,
        WorkKind::DecideFreeze => 4,
        WorkKind::DecideScore => 5,
        _ => 6,
    };
    let assignment = ContestStep {
        contest_id: c,
        step_id: step,
        ordinal,
        script_version: Text::new("decide-v1").expect("literal"),
        move_name: if judge {
            ContestMove::Judge
        } else {
            ContestMove::Fanout
        },
        inputs: vec![],
        assignments: items
            .iter()
            .map(|(w, _, _)| ContestAssignment {
                target: w.target.clone(),
                context_id: w.context_id.clone(),
                payload_refs: vec![],
                work_id: Some(w.work_id),
            })
            .collect(),
    };
    let mut event = super::envelope(
        id,
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestStep(assignment.clone()),
    );
    event.base.id = id;
    let seq = g.push(tx, event)?;
    squad_store::contests::associate(tx, c, ContestKind::Decide, seq)?;
    squad_store::contest_steps::insert(tx, &assignment, seq)?;
    for (mut w, binding, frozen) in items {
        if let Some(case) = case {
            tx.execute("INSERT OR IGNORE INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?,?,(SELECT epoch FROM room_meta),?,?,?,?,'{}')",params![case.to_string(),w.context_id.0,c.to_string(),binding.model.as_str(),frozen["data"].to_string(),seq.get()])?;
        }
        let projection = HistoryProjection {
            context_id: w.context_id.clone(),
            origin_event_id: id,
            projection_key: Text::new("decide.step").expect("literal"),
            role: HistoryRole::User,
            name: None,
            content: frozen.to_string(),
            meta: HistoryMetadata::Anonymous { anonymous: True },
        };
        w.history_through = squad_store::history::project(tx, &projection)?.ok_or_else(invalid)?;
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step, &w)?;
        storage::put(tx, c, &format!("input:{}", w.work_id), &frozen)?;
        g.push(
            tx,
            super::envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::WorkState(WorkStateEvent {
                    work_id: w.work_id,
                    trigger_event_id: id,
                    target: w.target,
                    context_id: w.context_id,
                    attempt: 0,
                    status: WorkStatus::Queued,
                    error: None,
                    debate_id: None,
                    case_id: case,
                    contest_id: Some(c),
                }),
            ),
        )?;
    }
    s.step_id = Some(step);
    Ok(())
}
fn output_artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut DecideState,
    kind: &str,
    body: Value,
) -> Result<()> {
    let step = s.step_id.ok_or_else(invalid)?;
    let a = ContestArtifact {
        kind: Some(ContestKind::Decide),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body.clone()),
        contest_id: s.contest_id,
        step_id: step,
        artifact_kind: Text::new(kind).map_err(|_| invalid())?,
        work_id: None,
        source_refs: vec![],
        data: body,
    };
    let event = super::envelope(
        id,
        Actor::System,
        vec![Recipient::Me],
        EnvelopePayload::ContestArtifact(a.clone()),
    );
    let event_id = event.base.id;
    let seq = g.push(tx, event)?;
    squad_store::contest_steps::artifact(tx, &a, seq)?;
    s.artifact_event_ids.push(event_id);
    Ok(())
}
pub fn advance(tx: &Transaction<'_>, g: &mut CommitGroup, id: EventId, c: ContestId) -> Result<()> {
    let mut s = load_state(tx, c)?;
    if s.state.is_terminal() {
        return Ok(());
    }
    let setup: DecideSetup = storage::get(tx, c, "setup")?;
    let old = s.state_version;
    if s.state == DecideStatus::Preflight {
        let raw: String = tx.query_row(
            "SELECT plan_json FROM contests WHERE contest_id=?",
            [c.to_string()],
            |r| r.get(0),
        )?;
        let p: ContestPreflight = serde_json::from_str(&raw)?;
        if g.accepted_at >= p.expires_at {
            resolve(tx, g, id, c, p.plan_id, true)?;
        }
        return Ok(());
    }
    if s.state == DecideStatus::WaitingForParticipants {
        for n in &setup.participants {
            let busy:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE target=? AND kind='reply' AND status IN ('queued','running') AND held_by_contest_id IS NULL)",[n.as_str()],|r|r.get(0))?;
            if busy {
                return Ok(());
            }
        }
        s.state = if setup.options.is_empty() {
            DecideStatus::ProposingOptions
        } else {
            DecideStatus::ProposingCriteria
        };
    } else if s.state == DecideStatus::Computing {
        let r: DecideRubric = storage::get(tx, c, "rubric")?;
        let os: Vec<DecideOption> = storage::get(tx, c, "options")?;
        let scores: Vec<DecideScores> = storage::get::<Vec<Value>>(tx, c, "score_proposals")?
            .into_iter()
            .map(|mut v| {
                v.as_object_mut().expect("proposal").remove("memberLabel");
                serde_json::from_value(v)
            })
            .collect::<std::result::Result<_, _>>()?;
        let m = script::matrix(&os, &r, &scores).map_err(|_| invalid())?;
        storage::put(tx, c, "matrix", &m)?;
        output_artifact(
            tx,
            g,
            id,
            &mut s,
            "decide.matrix",
            serde_json::to_value(&m)?,
        )?;
        s.state = DecideStatus::Explaining;
        s.step_id = None;
    } else if s.state == DecideStatus::Reporting {
        let report = DecideReport {
            proposers: storage::get::<BTreeMap<String, String>>(tx, c, "labels")?
                .into_iter()
                .map(|(n, l)| Ok((l, n.parse().map_err(|_| invalid())?)))
                .collect::<Result<_>>()?,
            options: storage::get(tx, c, "options")?,
            rubric: storage::get(tx, c, "rubric")?,
            matrix: storage::get(tx, c, "matrix")?,
            explanation: storage::optional(tx, c, "explanation")?,
            skipped: storage::get(tx, c, "skipped")?,
        };
        output_artifact(
            tx,
            g,
            id,
            &mut s,
            "decide.report",
            serde_json::to_value(report)?,
        )?;
        s.state = DecideStatus::Complete;
        s.terminal_reason = Some(Text::new("complete").expect("literal"));
    } else if s.step_id.is_none() {
        allocate(tx, g, id, &mut s, &setup)?;
    } else {
        let step = s.step_id.expect("checked");
        let mut q =
            tx.prepare("SELECT work_id FROM contest_assignments WHERE step_id=? ORDER BY target")?;
        let ids = q
            .query_map([step.to_string()], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut artifacts = vec![];
        let mut missing = vec![];
        let mut failed_judge = false;
        for raw in ids {
            let wid: WorkId = raw.parse().map_err(|_| invalid())?;
            let record = squad_store::work::record(tx, wid)?.ok_or_else(invalid)?;
            let w = squad_store::work::template(tx, wid)?.ok_or_else(invalid)?.0;
            if matches!(record.status, WorkStatus::Queued | WorkStatus::Running) {
                return Ok(());
            }
            if record.status == WorkStatus::Completed {
                let seq: i64 = tx.query_row(
                    "SELECT completed_event_seq FROM work WHERE work_id=?",
                    [raw],
                    |r| r.get(0),
                )?;
                let e = squad_store::events::at(tx, seq)?;
                let EnvelopePayload::ContestArtifact(a) = e.content else {
                    return Err(invalid());
                };
                artifacts.push((w, a, e.base.id));
            } else {
                failed_judge |= w.kind.new_mode_judge();
                missing.push(wid);
                let err = super::error(
                    ErrorCode::MODE_INVALID_OUTPUT,
                    "Skipped after exhausted attempts",
                );
                squad_store::contest_steps::settle_if_pending(tx, step, &w.target, Some(&err))?;
                let mut skipped: Vec<String> = storage::get(tx, c, "skipped")?;
                let line = format!(
                    "{}: {}",
                    w.target,
                    w.kind.new_mode_composition().unwrap_or("decide")
                );
                if !skipped.contains(&line) {
                    skipped.push(line);
                    storage::put(tx, c, "skipped", &skipped)?;
                }
            }
        }
        let seq = push(
            tx,
            g,
            id,
            c,
            EnvelopePayload::ContestReport(ContestReport {
                contest_id: c,
                step_id: step,
                artifact_event_ids: artifacts.iter().map(|x| x.2).collect(),
                skipped_work_ids: missing,
                data: json!({"step":s.state}),
            }),
        )?;
        squad_store::contest_steps::barrier(tx, step, seq)?;
        s.artifact_event_ids.extend(artifacts.iter().map(|x| x.2));
        if failed_judge {
            s.state = DecideStatus::Failed;
            s.terminal_reason = Some(Text::new("judge_failed").expect("literal"));
        } else {
            let k = step_kind(s.state).ok_or_else(invalid)?;
            if !k.new_mode_judge() && artifacts.len() < 2 {
                let reason = match k {
                    WorkKind::DecideOptions => "insufficient_options",
                    WorkKind::DecideCriteria => "insufficient_criteria",
                    _ => "insufficient_scores",
                };
                s.state = DecideStatus::Failed;
                s.terminal_reason = Some(Text::new(reason).expect("literal"));
            } else {
                let labels: BTreeMap<String, String> = storage::get(tx, c, "labels")?;
                let proposals = || {
                    artifacts
                        .iter()
                        .map(|(w, a, _)| {
                            let mut v = a.body.clone().expect("validated");
                            v["memberLabel"] = json!(labels[&w.target.to_string()]);
                            blind(v, &setup)
                        })
                        .collect::<Vec<_>>()
                };
                match k {
                    WorkKind::DecideOptions => {
                        storage::put(tx, c, "option_proposals", &proposals())?;
                        s.state = DecideStatus::Shortlisting;
                    }
                    WorkKind::DecideShortlist => {
                        let out: DecideShortlist =
                            serde_json::from_value(artifacts[0].1.body.clone().expect("body"))?;
                        storage::put(tx, c, "shortlist", &out)?;
                        storage::put(tx, c, "options", &script::shortlist(&out))?;
                        s.state = DecideStatus::ProposingCriteria;
                    }
                    WorkKind::DecideCriteria => {
                        storage::put(tx, c, "criteria_proposals", &proposals())?;
                        s.state = DecideStatus::Freezing;
                    }
                    WorkKind::DecideFreeze => {
                        let merged: DecideFreeze =
                            serde_json::from_value(artifacts[0].1.body.clone().expect("body"))?;
                        let lists: Vec<Value> = storage::get(tx, c, "criteria_proposals")?;
                        let mut input = BTreeMap::new();
                        for mut v in lists {
                            let label = v["memberLabel"].as_str().ok_or_else(invalid)?.to_owned();
                            v.as_object_mut().expect("object").remove("memberLabel");
                            input.insert(label, serde_json::from_value(v)?);
                        }
                        match script::rubric(&input, &merged) {
                            Ok(r) => {
                                storage::put(tx, c, "rubric", &r)?;
                                output_artifact(
                                    tx,
                                    g,
                                    id,
                                    &mut s,
                                    "decide.rubric",
                                    serde_json::to_value(&r)?,
                                )?;
                                s.state = DecideStatus::Scoring;
                            }
                            Err(_) => {
                                s.state = DecideStatus::Failed;
                                s.terminal_reason =
                                    Some(Text::new("no_shared_rubric").expect("literal"));
                            }
                        }
                    }
                    WorkKind::DecideScore => {
                        storage::put(tx, c, "score_proposals", &proposals())?;
                        s.state = DecideStatus::Computing;
                    }
                    WorkKind::DecideExplain => {
                        storage::put(tx, c, "explanation", &artifacts[0].1.body)?;
                        s.state = DecideStatus::Reporting;
                    }
                    _ => return Err(invalid()),
                }
            }
        }
        // Reporting keeps the final step so its app-owned artifact is associated with it.
        if s.state != DecideStatus::Reporting && s.state != DecideStatus::Computing {
            s.step_id = None;
        }
    }
    s.state_version += 1;
    save(tx, g, id, &s, Some(old))
}
