//! Admission, reference gates and atomic script moves for Blow It Up.
use super::{NewModeAdmission, envelope};
use crate::{orchestrator::blow_it_up as script, prompts::PromptPack};
use rusqlite::{Connection, Transaction, params};
use serde_json::{Value, json};
use squad_protocol::blow_it_up::*;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};

fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid blow_it_up move",
    )
}
fn push(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    c: ContestId,
    p: EnvelopePayload,
) -> Result<Cursor> {
    let seq = g.push(tx, envelope(id, Actor::System, vec![Recipient::Me], p))?;
    squad_store::contests::associate(tx, c, ContestKind::BlowItUp, seq)?;
    Ok(seq)
}
pub fn load_state(db: &Connection, id: ContestId) -> Result<BlowItUpState> {
    squad_store::blow_it_up::state(db, id)
}
fn plan(db: &Connection, id: ContestId) -> Result<ContestPreflight> {
    let raw: String = db.query_row(
        "SELECT plan_json FROM contests WHERE contest_id=?",
        [id.to_string()],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&raw)?)
}
fn save(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut BlowItUpState,
) -> Result<()> {
    let previous = s.state_version;
    s.state_version += 1;
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::BlowItUpState(s.clone()),
    )?;
    if tx.execute("UPDATE contests SET state_version=?,state_json=?,active=?,updated_seq=? WHERE contest_id=? AND state_version=?",params![s.state_version as i64,serde_json::to_string(s)?,!s.state.is_terminal(),seq.get(),s.contest_id.to_string(),previous as i64])?!=1{return Err(invalid());}
    if s.state.is_terminal() {
        tx.execute("UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1 WHERE contest_id=? AND status IN ('queued','running')",[s.contest_id.to_string()])?;
        squad_store::accounting::release_contest(tx, s.contest_id)?;
        squad_store::history::release(tx, s.contest_id)?;
        tx.execute("UPDATE work SET held_by_contest_id=NULL,runnable=1 WHERE held_by_contest_id=? AND status='queued'",[s.contest_id.to_string()])?;
    }
    Ok(())
}
pub fn steps(
    pack: &PromptPack,
    setup: &BlowItUpSetup,
    words: u32,
) -> Result<Vec<super::bounds::Step>> {
    use super::bounds::*;
    let m = setup.participants.len() as u64;
    let attack = json_text(&setup.target) + json_text(&setup.scope) + 512;
    // Additions enlarge only the ruling projection; every bound includes them.
    let pool = project(m * limited(4096, 700));
    let challenge = blinded(&setup.target) + pool + json_text(&setup.scope) + 512;
    let ruling = challenge + m * project(limited(4096, words)) + m * 6 * 256;
    let mut out = vec![];
    for n in &setup.participants {
        for (kind, data, visible) in [
            (WorkKind::BlowItUpAttack, attack, 2048),
            (WorkKind::BlowItUpChallenge, challenge, 4096),
        ] {
            out.push(Step {
                template: template(
                    pack,
                    squad_protocol::blow_it_up::composition(&kind).ok_or_else(invalid)?,
                )?,
                schema: schema_bytes(
                    squad_protocol::blow_it_up::output_schema(&kind).ok_or_else(invalid)?,
                ),
                kind,
                target: n.to_string(),
                alternatives: vec![],
                data,
                visible,
                calls: 1,
                attempts: 2,
            });
        }
    }
    out.push(Step {
        kind: WorkKind::BlowItUpRule,
        target: "judge".into(),
        alternatives: vec![],
        template: template(pack, "blow_it_up.rule")?,
        schema: schema_bytes(validation::schema::<BlowItUpRuling>()),
        data: ruling,
        visible: 4096,
        calls: 1,
        attempts: 3,
    });
    Ok(out)
}
pub fn prepare(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    input: &UserMessageInput,
    parsed: &ParsedInput,
    a: &NewModeAdmission<'_>,
) -> Result<RequestResult> {
    let ParsedInput::BlowItUp {
        participants,
        source,
        scope,
        ..
    } = parsed
    else {
        return Err(invalid());
    };
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(StoreError::Domain(
            ErrorCode::DEBATE_BUSY,
            "A contest is already running. Stop it before starting another.",
        ));
    }
    let (target, source_event_ids) = super::census::source(tx, source)?;
    let contest_id = ContestId::new();
    let mut draw = participants.clone();
    use rand::seq::SliceRandom;
    draw.shuffle(&mut rand::rng());
    let lenses = draw
        .iter()
        .enumerate()
        .map(|(i, n)| {
            (
                n.clone(),
                squad_protocol::blow_it_up::LENSES[i % 6].to_string(),
            )
        })
        .collect();
    let setup = BlowItUpSetup {
        contest_id,
        target: target.clone(),
        scope: scope.clone().unwrap_or_default(),
        participants: participants.clone(),
        lenses,
        source_event_ids,
    };
    setup.validate().map_err(|_| invalid())?;
    let pack = PromptPack::from_override_dir(a.config.and_then(|c| c.prompts.directory.as_deref()))
        .map_err(|_| invalid())?;
    let words = a.config.map_or(1200, |c| c.prompts.limits.plan_words);
    let priced = super::bounds::price(tx, a.bindings, &steps(&pack, &setup, words)?)?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    squad_store::accounting::admit(tx, &day, priced.reserved_micros, a.daily_max)?;
    let spend = squad_store::accounting::daily(tx, &day)?;
    let mut prompts = serde_json::Map::new();
    for name in [
        "blow_it_up.attack",
        "blow_it_up.challenge",
        "blow_it_up.rule",
    ] {
        let (system, user) = pack
            .composition_source(name, false)
            .map_err(|_| invalid())?;
        let (repair_system, repair_user) =
            pack.composition_source(name, true).map_err(|_| invalid())?;
        // Remove the shared judge preamble's hard-coded room nick from blind cases.
        let system = system.replace("@fable", "an unrelated room member");
        let repair_system = repair_system.replace("@fable", "an unrelated room member");
        prompts.insert(name.into(),json!({"system":system,"user":user,"repairSystem":repair_system,"repairUser":repair_user,"hash":squad_store::commit::digest(&(system,user,repair_system,repair_user))?}));
    }
    let mut tokens = a
        .bindings
        .iter()
        .flat_map(|(n, b)| vec![n.clone(), b.model.to_string()])
        .collect::<Vec<_>>();
    if let Some(c) = a.config {
        for m in &c.agents {
            tokens.push(m.persona.clone());
            tokens.push(m.model.clone());
        }
    }
    let p = ContestPreflight {
        contest_id,
        kind: ContestKind::BlowItUp,
        plan_id: PlanId::new(),
        expires_at: g.accepted_at
            + chrono::Duration::milliseconds(constants::PREFLIGHT_EXPIRY_MS as i64),
        estimate: DebateEstimate {
            estimated_usd: priced.estimated_usd,
            reserved_usd: priced.reserved_usd(),
            remaining_usd: (a.daily_max - spend.spent_micros - spend.reserved_micros) as f64 / 1e6,
            model_calls: setup.calls(),
        },
        participants: participants.clone(),
        confirmation_required: priced.estimated_usd > a.confirmation_threshold && !a.preconfirmed,
        warning: None,
        settings: json!({"attemptShares":priced.shares,"stepInputBounds":priced.input_bounds,"reservedAttempts":setup.attempts(),"memberCalls":2*participants.len(),"judgeCalls":1,"bindings":a.bindings,"admissionMaxMicros":a.daily_max,"packVersion":pack.version(),"prompts":prompts,"planWords":words,"tokens":super::probe::blind_tokens(tokens)}),
    };
    let state = BlowItUpState {
        contest_id,
        state_version: 1,
        state: if p.confirmation_required {
            BlowItUpStatus::Preflight
        } else {
            BlowItUpStatus::WaitingForParticipants
        },
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    // Insert before associations, with the first canonical event's cursor.
    let seq = g.push(
        tx,
        envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::BlowItUpPreflight(p.clone()),
        ),
    )?;
    tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,plan_id,plan_json,expires_at,updated_seq) VALUES(?,'blow_it_up',(SELECT epoch FROM room_meta),1,?,1,?,?,?,?)",params![contest_id.to_string(),serde_json::to_string(&state)?,p.plan_id.to_string(),serde_json::to_string(&p)?,p.expires_at.to_rfc3339(),seq.get()])?;
    squad_store::contests::associate(tx, contest_id, ContestKind::BlowItUp, seq)?;
    squad_store::blow_it_up::insert(tx, &setup)?;
    let mut event = envelope(
        id,
        Actor::Me,
        vec![Recipient::Me],
        EnvelopePayload::UserMessage(UserMessage {
            input: input.clone(),
            body: target,
            resolved_quotes: vec![],
            debate_id: None,
            case_id: None,
            contest_id: Some(contest_id),
        }),
    );
    event.base.id = input
        .message_id
        .to_string()
        .parse()
        .map_err(|_| invalid())?;
    let seq = g.push(tx, event)?;
    squad_store::contests::associate(tx, contest_id, ContestKind::BlowItUp, seq)?;
    push(
        tx,
        g,
        id,
        contest_id,
        EnvelopePayload::BlowItUpSetup(ContestSetup {
            contest_id,
            kind: ContestKind::BlowItUp,
            script_version: Text::new("blow_it_up-v1").unwrap(),
            participants: participants.clone(),
            source_event_ids: setup.source_event_ids.clone(),
            settings: serde_json::to_value(&setup)?,
        }),
    )?;
    push(tx, g, id, contest_id, EnvelopePayload::BlowItUpState(state))?;
    let seq = push(
        tx,
        g,
        id,
        contest_id,
        EnvelopePayload::ContestPreflight(p.clone()),
    )?;
    if p.confirmation_required {
        return Ok(RequestResult::PendingConfirmation {
            request_id: id,
            preflight: PendingPreflight::Contest(p),
            cursor: seq,
        });
    }
    reserve(tx, &p)?;
    preflight_result(
        tx,
        g,
        id,
        &p,
        PreflightStatus::Confirmed,
        Some(if a.preconfirmed {
            Confirmation::Preconfirmed
        } else {
            Confirmation::Automatic
        }),
    )?;
    Ok(g.accepted(id))
}
fn reserve(tx: &Connection, p: &ContestPreflight) -> Result<()> {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let key = format!("plan:{}", p.plan_id);
    let amount = squad_store::accounting::reservation_micros(p.estimate.reserved_usd)
        .map_err(|_| invalid())?;
    squad_store::accounting::reserve(
        tx,
        &key,
        None,
        &day,
        amount,
        p.settings["admissionMaxMicros"]
            .as_i64()
            .ok_or_else(invalid)?,
    )?;
    tx.execute(
        "UPDATE reservations SET contest_id=?,plan_id=? WHERE reservation_id=?",
        params![p.contest_id.to_string(), p.plan_id.to_string(), key],
    )?;
    Ok(())
}
fn preflight_result(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    p: &ContestPreflight,
    status: PreflightStatus,
    confirmation: Option<Confirmation>,
) -> Result<()> {
    push(
        tx,
        g,
        id,
        p.contest_id,
        EnvelopePayload::ContestPreflightResult(ContestPreflightResult {
            contest_id: p.contest_id,
            plan_id: p.plan_id,
            status,
            confirmation,
            error: None,
        }),
    )?;
    Ok(())
}
pub fn resolve(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    c: ContestId,
    pid: PlanId,
    cancel: bool,
) -> Result<RequestResult> {
    let mut s = load_state(tx, c)?;
    let p = plan(tx, c)?;
    if s.state != BlowItUpStatus::Preflight || p.plan_id != pid {
        return Err(invalid());
    }
    let expired = g.accepted_at >= p.expires_at;
    if cancel || expired {
        s.state = if cancel {
            BlowItUpStatus::Stopped
        } else {
            BlowItUpStatus::Failed
        };
        s.terminal_reason = Some(
            Text::new(if cancel {
                "stopped"
            } else {
                "preflight_expired"
            })
            .unwrap(),
        );
    } else {
        reserve(tx, &p)?;
        s.state = BlowItUpStatus::WaitingForParticipants;
    }
    preflight_result(
        tx,
        g,
        id,
        &p,
        if cancel {
            PreflightStatus::Cancelled
        } else if expired {
            PreflightStatus::Expired
        } else {
            PreflightStatus::Confirmed
        },
        (!cancel && !expired).then_some(Confirmation::Explicit),
    )?;
    save(tx, g, id, &mut s)?;
    Ok(g.accepted(id))
}
fn tokens(p: &ContestPreflight) -> Vec<String> {
    serde_json::from_value(p.settings["tokens"].clone()).unwrap_or_default()
}
/// The blind projection of attack and challenge prose; public for the isolation fixtures.
#[doc(hidden)]
pub fn blind(mut v: Value, p: &ContestPreflight) -> Value {
    fn visit(v: &mut Value, tokens: &[String], prose: bool) {
        match v {
            Value::String(s) if prose => *s = super::probe::redact(s, tokens),
            Value::Array(a) => {
                for v in a {
                    visit(v, tokens, prose)
                }
            }
            Value::Object(m) => {
                for (k, v) in m {
                    visit(
                        v,
                        tokens,
                        matches!(
                            k.as_str(),
                            "target"
                                | "scope"
                                | "title"
                                | "targetQuote"
                                | "exploit"
                                | "blastRadius"
                                | "reason"
                                | "defeatingQuote"
                                | "mitigation"
                                | "residual"
                                | "challengeRejection"
                                | "note"
                        ),
                    )
                }
            }
            _ => {}
        }
    }
    visit(&mut v, &tokens(p), false);
    v
}
/// Map a quote from the anonymous target back to a verified span of the frozen original.
fn original_quote(quote: &str, source: &str, tokens: &[String]) -> Option<String> {
    if let Some(span) = crate::orchestrator::verbatim::verbatim(quote, source) {
        return Some(source[span].into());
    }
    let mut projected = String::new();
    let mut spans = Vec::new();
    let mut offset = 0;
    while offset < source.len() {
        let rest = &source[offset..];
        let previous = source[..offset].chars().next_back();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let found = tokens
            .iter()
            .filter(|t| !t.is_empty())
            .filter_map(|token| {
                let end = rest
                    .char_indices()
                    .nth(token.chars().count())
                    .map(|(i, _)| i)
                    .unwrap_or(rest.len());
                (rest[..end].to_lowercase() == token.to_lowercase()
                    && boundary(previous)
                    && boundary(rest[end..].chars().next()))
                .then_some(end)
            })
            .max();
        let (piece, end) = if let Some(len) = found {
            ("[redacted]".to_owned(), offset + len)
        } else {
            let c = rest.chars().next()?;
            (c.to_string(), offset + c.len_utf8())
        };
        spans.extend(std::iter::repeat_n((offset, end), piece.len()));
        projected.push_str(&piece);
        offset = end;
    }
    let range = crate::orchestrator::verbatim::verbatim(quote, &projected)?;
    let original = &source[spans.get(range.start)?.0..spans.get(range.end - 1)?.1];
    crate::orchestrator::verbatim::verbatim(original, source)?;
    Some(original.into())
}
pub fn stage_input(db: &Connection, s: &BlowItUpState, target: &Target) -> Result<Value> {
    let setup = squad_store::blow_it_up::setup(db, s.contest_id)?;
    let p = plan(db, s.contest_id)?;
    if s.state == BlowItUpStatus::Attacking {
        let Target::Member(n) = target else {
            return Err(invalid());
        };
        return Ok(
            json!({"target":setup.target,"scope":setup.scope,"lens":setup.lenses.get(n).ok_or_else(invalid)?,"wordLimit":700}),
        );
    }
    let pool = squad_store::blow_it_up::pool(db, s.contest_id)?;
    if s.state == BlowItUpStatus::Challenging {
        let Target::Member(n) = target else {
            return Err(invalid());
        };
        let vectors = pool
            .vectors
            .iter()
            .filter(|v| {
                !pool
                    .provenance
                    .iter()
                    .any(|p| p.vector_id == v.vector_id && &p.attacker == n)
            })
            .collect::<Vec<_>>();
        return Ok(blind(
            json!({"target":setup.target,"scope":setup.scope,"vectors":vectors,"wordLimit":p.settings["planWords"]}),
            &p,
        ));
    }
    if s.state == BlowItUpStatus::Ruling && *target == Target::Judge {
        let ballots =
            squad_store::blow_it_up::ballots::<BlowItUpChallenge>(db, s.contest_id, "challenge")?;
        let material = BlowItUpCaseMaterial {
            target: setup.target,
            scope: setup.scope,
            vectors: pool.vectors,
            tallies: squad_store::blow_it_up::tally(db, s.contest_id)?,
            challenges: ballots.into_iter().flat_map(|(_, b)| b.verdicts).collect(),
        };
        let mut v = blind(serde_json::to_value(material)?, &p);
        v["wordLimit"] = p.settings["planWords"].clone();
        return Ok(v);
    }
    Err(invalid())
}
/// Validate the exact projection used by a call, then repeat at the commit gate.
pub fn validate_output(
    kind: &WorkKind,
    data: &Value,
    output: &Value,
) -> validation::ValidationResult {
    use validation::{decode, ensure};
    let target = data["target"].as_str().unwrap_or_default();
    let quote = |v: &BlowItUpVector| {
        ensure(
            crate::orchestrator::verbatim::verbatim(v.target_quote.as_str(), target).is_some(),
            "targetQuote must be verbatim (at least 12 contiguous characters)",
        )
    };
    match kind {
        WorkKind::BlowItUpAttack => {
            let a = decode::<BlowItUpAttack>(output)?;
            a.validate()?;
            for v in &a.vectors {
                quote(v)?;
            }
            ensure(prose_words(output) <= 700, "attack exceeds 700 prose words")
        }
        WorkKind::BlowItUpChallenge => {
            let b = decode::<BlowItUpChallenge>(output)?;
            b.validate()?;
            let vectors: Vec<PooledVector> = serde_json::from_value(data["vectors"].clone())
                .map_err(|_| validation::ValidationError {
                    message: "invalid frozen pool".into(),
                })?;
            ensure(
                b.verdicts.len() == vectors.len()
                    && vectors
                        .iter()
                        .all(|v| b.verdicts.iter().any(|b| b.vector_id == v.vector_id)),
                "one verdict required per assigned vector",
            )?;
            for v in &b.verdicts {
                if let Some(q) = &v.defeating_quote {
                    ensure(
                        crate::orchestrator::verbatim::verbatim(q.as_str(), target).is_some(),
                        "defeatingQuote must be verbatim",
                    )?;
                }
                if let Some(id) = &v.duplicate_of {
                    ensure(
                        vectors.iter().any(|v| &v.vector_id == id),
                        "duplicateOf must be a supplied vector",
                    )?;
                }
            }
            for v in &b.additions {
                quote(&v.clone().into())?;
            }
            ensure(
                prose_words(output) <= data["wordLimit"].as_u64().unwrap_or(1200) as usize,
                "challenge prose budget exceeded",
            )
        }
        WorkKind::BlowItUpRule => {
            let r = decode::<BlowItUpRuling>(output)?;
            let vectors: Vec<PooledVector> = serde_json::from_value(data["vectors"].clone())
                .map_err(|_| validation::ValidationError {
                    message: "invalid frozen pool".into(),
                })?;
            let tally: BlowItUpTally = decode(&data["tallies"])?;
            r.validate_against(&vectors, &tally)?;
            ensure(
                prose_words(output) <= data["wordLimit"].as_u64().unwrap_or(1200) as usize,
                "ruling prose budget exceeded",
            )
        }
        _ => ensure(false, "unknown work kind"),
    }
}
pub fn prose_words(v: &Value) -> usize {
    match v {
        Value::Object(m) => m
            .iter()
            .filter(|(k, _)| {
                matches!(
                    k.as_str(),
                    "title"
                        | "targetQuote"
                        | "exploit"
                        | "blastRadius"
                        | "reason"
                        | "defeatingQuote"
                        | "mitigation"
                        | "residual"
                        | "challengeRejection"
                        | "note"
                        | "vectors"
                        | "verdicts"
                        | "additions"
                        | "findings"
                        | "killedNotes"
                )
            })
            .map(|(_, v)| prose_words(v))
            .sum(),
        Value::Array(a) => a.iter().map(prose_words).sum(),
        Value::String(s) => s.split_whitespace().count(),
        _ => 0,
    }
}
pub fn accepted_work(db: &Connection, w: &Work, a: &ContestArtifact, seq: Cursor) -> Result<()> {
    let mut s = load_state(db, a.contest_id)?;
    if s.step_id != w.step_id
        || s.state.is_terminal()
        || a.artifact_kind.as_str()
            != squad_protocol::blow_it_up::composition(&w.kind).ok_or_else(invalid)?
        || a.round != Some(0)
        || a.audience != Some(ArtifactAudience::Human)
    {
        return Err(invalid());
    }
    let body = a.body.as_ref().ok_or_else(invalid)?;
    validate_output(&w.kind, &stage_input(db, &s, &w.target)?, body).map_err(|_| invalid())?;
    if w.kind == WorkKind::BlowItUpChallenge {
        let setup = squad_store::blow_it_up::setup(db, s.contest_id)?;
        let p = plan(db, s.contest_id)?;
        let ballot: BlowItUpChallenge = serde_json::from_value(body.clone())?;
        for q in ballot
            .verdicts
            .iter()
            .filter_map(|v| v.defeating_quote.as_ref())
            .chain(ballot.additions.iter().map(|v| &v.target_quote))
        {
            original_quote(q.as_str(), &setup.target, &tokens(&p)).ok_or_else(invalid)?;
        }
    }
    squad_store::contest_steps::artifact(db, a, seq)?;
    let stage = match w.kind {
        WorkKind::BlowItUpAttack => "attack",
        WorkKind::BlowItUpChallenge => "challenge",
        WorkKind::BlowItUpRule => "rule",
        _ => return Err(invalid()),
    };
    squad_store::blow_it_up::ballot(db, s.contest_id, &w.target.to_string(), stage, body, seq)?;
    let event = squad_store::events::at(db, seq.get())?;
    s.artifact_event_ids.push(event.base.id);
    db.execute(
        "UPDATE contests SET state_json=? WHERE contest_id=?",
        params![serde_json::to_string(&s)?, s.contest_id.to_string()],
    )?;
    if let Some(case) = w.case_id {
        squad_store::cases::seal(db, case, seq)?;
    }
    Ok(())
}
pub fn human_answer(_tx: &Connection, _p: &parser::PendingHuman, _complete: bool) -> Result<()> {
    Err(invalid())
}
pub fn stop(tx: &Transaction<'_>, g: &mut CommitGroup, id: EventId, c: ContestId) -> Result<()> {
    let mut s = load_state(tx, c)?;
    s.state = BlowItUpStatus::Stopped;
    s.terminal_reason = Some(Text::new("stopped").unwrap());
    save(tx, g, id, &mut s)
}
pub fn recover(db: &Connection, c: ContestId) -> Result<()> {
    let s = load_state(db, c)?;
    s.validate().map_err(|_| invalid())?;
    squad_store::blow_it_up::setup(db, c)?
        .validate()
        .map_err(|_| invalid())
}
pub fn transition(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    submission: ContestTransitionSubmission,
) -> Result<()> {
    let mut s = load_state(tx, submission.contest_id)?;
    if submission.expected_state_version != s.state_version
        || s.state != BlowItUpStatus::WaitingForParticipants
        || !matches!(
            submission.transition,
            ContestTransition::BlowItUp(BlowItUpTransition::ParticipantsReady)
        )
    {
        return Err(invalid());
    }
    s.state = BlowItUpStatus::Attacking;
    save(tx, g, id, &mut s)
}
pub fn produce(_tx: &Transaction<'_>, _g: &mut CommitGroup, _e: ProducerEnvelope) -> Result<()> {
    Err(invalid())
}
fn system_artifact(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut BlowItUpState,
    name: &str,
    body: Value,
) -> Result<Cursor> {
    let artifact = ContestArtifact {
        contest_id: s.contest_id,
        step_id: s.step_id.ok_or_else(invalid)?,
        artifact_kind: Text::new(name).unwrap(),
        work_id: None,
        source_refs: vec![],
        data: json!({}),
        kind: Some(ContestKind::BlowItUp),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(body),
    };
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestArtifact(artifact.clone()),
    )?;
    squad_store::contest_steps::artifact(tx, &artifact, seq)?;
    s.artifact_event_ids
        .push(squad_store::events::at(tx, seq.get())?.base.id);
    Ok(seq)
}
pub fn barrier(tx: &Transaction<'_>, g: &mut CommitGroup, commit: BarrierCommit) -> Result<()> {
    let mut s = load_state(tx, commit.contest_id)?;
    if s.state_version != commit.expected_state_version
        || s.step_id != Some(commit.step_id)
        || s.state.is_terminal()
        || !commit.next_work.is_empty()
        || !commit.projections.is_empty()
    {
        return Err(invalid());
    }
    complete_stage(tx, g, commit.barrier_event.base.id, &mut s)
}
fn complete_stage(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut BlowItUpState,
) -> Result<()> {
    let step = s.step_id.ok_or_else(invalid)?;
    let mut q=tx.prepare("SELECT a.target,w.status FROM contest_assignments a JOIN work w ON w.work_id=a.work_id WHERE a.step_id=? AND a.outcome='pending'")?;
    let rows = q
        .query_map([step.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (n, status) in rows {
        if !matches!(status.as_str(), "failed" | "cancelled") {
            return Err(invalid());
        }
        let target = if n == "judge" {
            Target::Judge
        } else {
            Target::Member(n.parse().map_err(|_| invalid())?)
        };
        squad_store::contest_steps::settle(
            tx,
            step,
            &target,
            Some(&super::error(
                ErrorCode::PROVIDER_ERROR,
                "Blow It Up slot exhausted",
            )),
        )?;
    }
    match s.state {
        BlowItUpStatus::Attacking => {
            let attacks =
                squad_store::blow_it_up::ballots::<BlowItUpAttack>(tx, s.contest_id, "attack")?;
            if attacks.len() < 2 {
                s.state = BlowItUpStatus::Failed;
                s.terminal_reason = Some(Text::new("insufficient_attacks").unwrap());
            } else {
                let mut pool = BlowItUpPool {
                    vectors: vec![],
                    provenance: vec![],
                };
                for (n, a) in attacks {
                    for v in a.vectors {
                        add_vector(&mut pool, n.parse().map_err(|_| invalid())?, v, false);
                    }
                }
                use rand::seq::SliceRandom;
                pool.vectors.shuffle(&mut rand::rng());
                squad_store::blow_it_up::save_pool(tx, s.contest_id, &pool)?;
                system_artifact(
                    tx,
                    g,
                    id,
                    s,
                    "blow_it_up.pool",
                    serde_json::to_value(&pool)?,
                )?;
                s.state = BlowItUpStatus::Challenging;
            }
        }
        BlowItUpStatus::Challenging => {
            let mut pool = squad_store::blow_it_up::pool(tx, s.contest_id)?;
            let ballots = squad_store::blow_it_up::ballots::<BlowItUpChallenge>(
                tx,
                s.contest_id,
                "challenge",
            )?;
            for (n, b) in &ballots {
                for v in &b.additions {
                    add_vector(
                        &mut pool,
                        n.parse().map_err(|_| invalid())?,
                        v.clone().into(),
                        true,
                    );
                }
            }
            let tally = script::tally(&pool, &ballots);
            squad_store::blow_it_up::save_pool(tx, s.contest_id, &pool)?;
            squad_store::blow_it_up::save_tally(tx, s.contest_id, &tally)?;
            system_artifact(
                tx,
                g,
                id,
                s,
                "blow_it_up.pool",
                serde_json::to_value(&pool)?,
            )?;
            system_artifact(
                tx,
                g,
                id,
                s,
                "blow_it_up.tally",
                serde_json::to_value(tally)?,
            )?;
            s.state = BlowItUpStatus::Ruling;
        }
        BlowItUpStatus::Ruling => {
            let rulings =
                squad_store::blow_it_up::ballots::<BlowItUpRuling>(tx, s.contest_id, "rule")?;
            if let Some((_, r)) = rulings.first() {
                s.state = BlowItUpStatus::Reporting;
                save(tx, g, id, s)?;
                let pool = squad_store::blow_it_up::pool(tx, s.contest_id)?;
                let tally = squad_store::blow_it_up::tally(tx, s.contest_id)?;
                let ballots = squad_store::blow_it_up::ballots::<BlowItUpChallenge>(
                    tx,
                    s.contest_id,
                    "challenge",
                )?;
                let setup = squad_store::blow_it_up::setup(tx, s.contest_id)?;
                let attacks =
                    squad_store::blow_it_up::ballots::<BlowItUpAttack>(tx, s.contest_id, "attack")?;
                let mut skipped = vec![];
                for n in setup.participants {
                    for (stage, missing) in [
                        ("attack", !attacks.iter().any(|(a, _)| a == n.as_str())),
                        ("challenge", !ballots.iter().any(|(a, _)| a == n.as_str())),
                    ] {
                        if missing {
                            skipped.push(SkippedMember {
                                member: n.clone(),
                                step: stage.into(),
                            });
                        }
                    }
                }
                let mut report_ballots = ballots.clone();
                let setup = squad_store::blow_it_up::setup(tx, s.contest_id)?;
                let p = plan(tx, s.contest_id)?;
                for (_, b) in &mut report_ballots {
                    for v in &mut b.verdicts {
                        if let Some(q) = &v.defeating_quote {
                            v.defeating_quote = Some(
                                Text::new(
                                    original_quote(q.as_str(), &setup.target, &tokens(&p))
                                        .ok_or_else(invalid)?,
                                )
                                .map_err(|_| invalid())?,
                            );
                        }
                    }
                }
                let report = script::report(r, &pool, &tally, &report_ballots, skipped);
                system_artifact(
                    tx,
                    g,
                    id,
                    s,
                    "blow_it_up.report",
                    serde_json::to_value(report)?,
                )?;
                s.state = BlowItUpStatus::Complete;
                s.terminal_reason = Some(Text::new("reported").unwrap());
            } else {
                s.state = BlowItUpStatus::Failed;
                s.terminal_reason = Some(Text::new("judge_failed").unwrap());
            }
        }
        _ => return Err(invalid()),
    }
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestReport(ContestReport {
            contest_id: s.contest_id,
            step_id: step,
            artifact_event_ids: s.artifact_event_ids.clone(),
            skipped_work_ids: vec![],
            data: json!({"blowItUpStage":s.state}),
        }),
    )?;
    squad_store::contest_steps::barrier(tx, step, seq)?;
    s.step_id = None;
    save(tx, g, id, s)
}
fn add_vector(pool: &mut BlowItUpPool, attacker: MemberNick, v: BlowItUpVector, addition: bool) {
    let id = Text::new(uuid::Uuid::new_v4().to_string()).unwrap();
    pool.provenance.push(VectorProvenance {
        vector_id: id.clone(),
        attacker,
        local_id: v.local_id,
        addition,
    });
    pool.vectors.push(PooledVector {
        vector_id: id,
        title: v.title,
        target_quote: v.target_quote,
        exploit: v.exploit,
        severity: v.severity,
        likelihood: v.likelihood,
        blast_radius: v.blast_radius,
        addition,
    });
}
pub fn advance(tx: &Transaction<'_>, g: &mut CommitGroup, id: EventId, c: ContestId) -> Result<()> {
    let mut s = load_state(tx, c)?;
    if s.state.is_terminal() {
        return Ok(());
    }
    if s.state == BlowItUpStatus::Preflight {
        let p = plan(tx, c)?;
        if g.accepted_at >= p.expires_at {
            resolve(tx, g, id, c, p.plan_id, false)?;
        }
        return Ok(());
    }
    // Hold newly admitted ordinary work before the runtime wake. Older replies drain.
    let setup = squad_store::blow_it_up::setup(tx, c)?;
    tx.execute("UPDATE work SET held_by_contest_id=?,runnable=0 WHERE kind='reply' AND step_id IS NULL AND status='queued' AND held_by_contest_id IS NULL AND trigger_event_seq>(SELECT MIN(e.seq) FROM events e JOIN contest_events ce ON ce.event_seq=e.seq WHERE ce.contest_id=? AND e.kind='user.message') AND target IN (SELECT value FROM json_each(?))",params![c.to_string(),c.to_string(),serde_json::to_string(&setup.participants)?])?;
    if s.state == BlowItUpStatus::WaitingForParticipants {
        let setup = squad_store::blow_it_up::setup(tx, c)?;
        let waiting:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND step_id IS NULL AND held_by_contest_id IS NULL AND status IN ('queued','running') AND json_type(scheduling_meta,'$.openModeDormant') IS NULL AND target IN (SELECT value FROM json_each(?)))",[serde_json::to_string(&setup.participants)?],|r|r.get(0))?;
        if waiting {
            return Ok(());
        }
        s.state = BlowItUpStatus::Attacking;
        save(tx, g, id, &mut s)?;
    }
    if let Some(step) = s.step_id {
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM work WHERE step_id=? AND status IN ('queued','running'))",
            [step.to_string()],
            |r| r.get(0),
        )?;
        if !pending {
            complete_stage(tx, g, id, &mut s)?;
        }
        return Ok(());
    }
    allocate(tx, g, id, &mut s)
}
fn allocate(
    tx: &Transaction<'_>,
    g: &mut CommitGroup,
    id: EventId,
    s: &mut BlowItUpState,
) -> Result<()> {
    let p = plan(tx, s.contest_id)?;
    let setup = squad_store::blow_it_up::setup(tx, s.contest_id)?;
    let (kind, ordinal, movement) = match s.state {
        BlowItUpStatus::Attacking => (WorkKind::BlowItUpAttack, 1, ContestMove::Fanout),
        BlowItUpStatus::Challenging => (WorkKind::BlowItUpChallenge, 2, ContestMove::Fanout),
        BlowItUpStatus::Ruling => (WorkKind::BlowItUpRule, 3, ContestMove::Judge),
        _ => return Err(invalid()),
    };
    let targets = if ordinal == 3 {
        vec![Target::Judge]
    } else {
        setup
            .participants
            .iter()
            .cloned()
            .map(Target::Member)
            .collect()
    };
    let step = StepId::new();
    let name = squad_protocol::blow_it_up::composition(&kind).ok_or_else(invalid)?;
    let mut works = vec![];
    let mut assignments = vec![];
    let trigger: String=tx.query_row("SELECT e.id FROM contest_events c JOIN events e ON e.seq=c.event_seq WHERE c.contest_id=? ORDER BY e.seq LIMIT 1",[s.contest_id.to_string()],|r|r.get(0))?;
    let trigger: EventId = trigger.parse().map_err(|_| invalid())?;
    for target in targets {
        let case_id = (target == Target::Judge).then(CaseId::new);
        let context = ContextId(if let Some(case) = case_id {
            format!("judge:{case}")
        } else {
            format!("contest:{}:{step}:{target}", s.contest_id)
        });
        let w = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: None,
            kind: kind.clone(),
            target: target.clone(),
            context_id: context.clone(),
            trigger_event_id: trigger,
            history_through: HistoryPosition::new(1).unwrap(),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id,
            prompt_ref: Some(PromptRef {
                template_id: Text::new(name).unwrap(),
                pack_version: Text::new(p.settings["packVersion"].as_str().ok_or_else(invalid)?)
                    .unwrap(),
                template_hash: Text::new(
                    p.settings["prompts"][name]["hash"]
                        .as_str()
                        .ok_or_else(invalid)?,
                )
                .unwrap(),
            }),
            max_output_tokens: Some(if ordinal == 1 { 2048 } else { 4096 }),
            contest_id: Some(s.contest_id),
            step_id: Some(step),
        };
        assignments.push(ContestAssignment {
            target,
            context_id: context,
            payload_refs: vec![],
            work_id: Some(w.work_id),
        });
        works.push(w);
    }
    let step_data = ContestStep {
        contest_id: s.contest_id,
        step_id: step,
        ordinal,
        script_version: Text::new("blow_it_up-v1").unwrap(),
        move_name: movement,
        inputs: vec![],
        assignments,
    };
    let seq = push(
        tx,
        g,
        id,
        s.contest_id,
        EnvelopePayload::ContestStep(step_data.clone()),
    )?;
    squad_store::contest_steps::insert(tx, &step_data, seq)?;
    let source = squad_store::events::at(tx, seq.get())?;
    s.step_id = Some(step);
    for mut w in works {
        let data = stage_input(tx, s, &w.target)?;
        let mut binding: ExecutorBinding =
            serde_json::from_value(p.settings["bindings"][w.target.to_string()].clone())?;
        if w.target == Target::Judge {
            binding.effort = constants::NEW_MODE_JUDGE_REASONING_EFFORT;
            let material: BlowItUpCaseMaterial = {
                let mut d = data.clone();
                d.as_object_mut().ok_or_else(invalid)?.remove("wordLimit");
                serde_json::from_value(d)?
            };
            material.validate().map_err(|_| invalid())?;
            let case = CaseOpened {
                case_id: w.case_id.ok_or_else(invalid)?,
                judge_invocation_id: EventId::new(),
                work_id: Some(w.work_id),
                judge_model: binding.model.clone(),
                material: CaseMaterial::BlowItUp(material),
                debate_id: None,
                contest_id: Some(s.contest_id),
            };
            let case_seq = push(
                tx,
                g,
                id,
                s.contest_id,
                EnvelopePayload::CaseOpened(case.clone()),
            )?;
            squad_store::cases::open(tx, &case, case_seq)?;
        }
        let (length, output, params_json): (u32, Option<u32>, String) = tx.query_row(
            "SELECT context_length,max_output,supported_params_json FROM catalog WHERE slug=?",
            [binding.model.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let parameters: Vec<String> = serde_json::from_str(&params_json)?;
        let frozen = json!({"data":data,"prompt":p.settings["prompts"][name],"contextLength":length,"outputLimit":output.unwrap_or(65536),"visibleTokens":if ordinal==1{2048}else{4096},"supportsReasoning":parameters.iter().any(|p|p=="reasoning")});
        let boundary = squad_store::history::project(
            tx,
            &HistoryProjection {
                context_id: w.context_id.clone(),
                origin_event_id: source.base.id,
                projection_key: Text::new("blow_it_up.input").unwrap(),
                role: HistoryRole::User,
                name: None,
                content: frozen.to_string(),
                meta: HistoryMetadata::Anonymous { anonymous: True },
            },
        )?;
        w.history_through = boundary.ok_or_else(invalid)?;
        squad_store::work::insert(tx, &w, &binding, None)?;
        squad_store::contest_steps::attach_work(tx, step, &w)?;
        super::contests::queued(tx, g, &w)?;
    }
    save(tx, g, id, s)
}

/// Readiness avoids writing an empty engine receipt on every streaming wakeup.
pub fn needs_advance(db: &Connection, c: ContestId) -> Result<bool> {
    let s = load_state(db, c)?;
    if s.state.is_terminal() {
        return Ok(false);
    }
    if s.state == BlowItUpStatus::Preflight {
        return Ok(chrono::Utc::now() >= plan(db, c)?.expires_at);
    }
    let setup = squad_store::blow_it_up::setup(db, c)?;
    let needs_hold:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND step_id IS NULL AND status='queued' AND held_by_contest_id IS NULL AND trigger_event_seq>(SELECT MIN(e.seq) FROM events e JOIN contest_events ce ON ce.event_seq=e.seq WHERE ce.contest_id=? AND e.kind='user.message') AND target IN (SELECT value FROM json_each(?)))",params![c.to_string(),serde_json::to_string(&setup.participants)?],|r|r.get(0))?;
    if needs_hold {
        return Ok(true);
    }
    if s.state == BlowItUpStatus::WaitingForParticipants {
        let waiting:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM work WHERE kind='reply' AND step_id IS NULL AND held_by_contest_id IS NULL AND status IN ('queued','running') AND json_type(scheduling_meta,'$.openModeDormant') IS NULL AND target IN (SELECT value FROM json_each(?)))",[serde_json::to_string(&setup.participants)?],|r|r.get(0))?;
        return Ok(!waiting);
    }
    let Some(step) = s.step_id else {
        return Ok(true);
    };
    Ok(!db.query_row(
        "SELECT EXISTS(SELECT 1 FROM work WHERE step_id=? AND status IN ('queued','running'))",
        [step.to_string()],
        |r| r.get::<_, bool>(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anonymous_quote_maps_back_to_original_utf8_span() {
        let source = "The αlpha operator verifies every write before commit.";
        let tokens = vec!["αlpha".into()];
        let projected = super::super::probe::redact(source, &tokens);
        assert_eq!(
            original_quote(&projected, source, &tokens).as_deref(),
            Some(source)
        );
        assert!(original_quote("An invented bypass quotation", source, &tokens).is_none());
    }
    #[test]
    fn projection_redacts_all_prose_without_changing_vector_ids() {
        let p = ContestPreflight {
            contest_id: ContestId::new(),
            kind: ContestKind::BlowItUp,
            plan_id: PlanId::new(),
            expires_at: chrono::Utc::now(),
            estimate: DebateEstimate {
                estimated_usd: 0.0,
                reserved_usd: 0.0,
                remaining_usd: 0.0,
                model_calls: 1,
            },
            participants: vec![],
            confirmation_required: false,
            warning: None,
            settings: json!({"tokens":["alpha","fixture/model"]}),
        };
        let v = blind(
            json!({"target":"alpha uses fixture/model","vectors":[{"vectorId":"alpha","title":"alpha fails","targetQuote":"alpha uses fixture/model","exploit":"Ask alpha for data","blastRadius":"alpha data"}]}),
            &p,
        );
        assert_eq!(v["vectors"][0]["vectorId"], "alpha");
        for field in ["title", "targetQuote", "exploit", "blastRadius"] {
            assert!(!v["vectors"][0][field].as_str().unwrap().contains("alpha"));
        }
        assert!(!v["target"].as_str().unwrap().contains("fixture/model"));
    }
}
