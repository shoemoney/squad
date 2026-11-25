//! Authoritative grill records and the compact per-step working projection.
use crate::{Result, StoreError};
use rusqlite::{Connection, params};
use squad_protocol::*;
fn invalid() -> StoreError {
    StoreError::Domain(
        ErrorCode::INVALID_STATE_TRANSITION,
        "Invalid grill evidence",
    )
}
pub fn state(db: &Connection, id: ContestId) -> Result<GrillState> {
    let raw:String=db.query_row("SELECT state_json FROM contests WHERE contest_id=? AND kind='grill' AND epoch=(SELECT epoch FROM room_meta)",[id.to_string()],|r|r.get(0))?;
    let state: GrillState = serde_json::from_str(&raw)?;
    state.validate().map_err(|_| invalid())?;
    Ok(state)
}
pub fn setup(db: &Connection, id: ContestId) -> Result<GrillPreflight> {
    let raw:String=db.query_row("SELECT map_json FROM contest_private_maps WHERE contest_id=? AND scope_key='grill.settings'",[id.to_string()],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
pub fn events(db: &Connection, id: ContestId) -> Result<Vec<CommittedEnvelope>> {
    let mut query =
        db.prepare("SELECT event_seq FROM contest_events WHERE contest_id=? ORDER BY event_seq")?;
    query
        .query_map([id.to_string()], |r| r.get::<_, i64>(0))?
        .map(|r| crate::events::at(db, r?))
        .collect()
}
/// Caller must commit the event and this index update in the same transaction.
pub fn retain(db: &Connection, event: &CommittedEnvelope) -> Result<()> {
    let id = match &event.content {
        EnvelopePayload::GrillSetup(v) => v.contest_id,
        EnvelopePayload::GrillQuestion(v) => v.contest_id,
        EnvelopePayload::GrillAnswer(v) => v.contest_id,
        EnvelopePayload::GrillPass(v) => v.contest_id,
        EnvelopePayload::GrillPlan(v) => v.contest_id,
        EnvelopePayload::GrillComplete(v) => v.contest_id,
        _ => return Err(invalid()),
    };
    let frozen = setup(db, id)?;
    let current = state(db, id)?;
    if current.state.is_terminal() {
        return Err(invalid());
    }
    let recipients: Vec<_> = std::iter::once(Recipient::Me)
        .chain(
            frozen
                .setup
                .participants()
                .into_iter()
                .map(Recipient::Member),
        )
        .collect();
    if event.base.recipients != recipients {
        return Err(invalid());
    }
    let author = match &event.content {
        EnvelopePayload::GrillQuestion(v) => Actor::Member(v.griller.clone()),
        EnvelopePayload::GrillAnswer(v) => Actor::Member(v.leader.clone()),
        EnvelopePayload::GrillPlan(v) if v.version > 1 => Actor::Member(v.leader.clone()),
        EnvelopePayload::GrillPass(v) if v.reason == GrillPassReason::Pass => {
            Actor::Member(v.griller.clone())
        }
        _ => Actor::System,
    };
    if event.base.sender != author {
        return Err(invalid());
    }
    let seq = event.commit.seq.get();
    match &event.content {
        EnvelopePayload::GrillSetup(v) if v == &frozen.setup => {}
        EnvelopePayload::GrillPlan(v) => {
            v.validate(frozen.plan_words).map_err(|_| invalid())?;
            let initial = v.version == 1 && v.round == 0 && v.text == frozen.setup.initial_plan;
            if v.leader != current.leader
                || (!initial
                    && (current.state != GrillStatus::Rebuilding
                        || v.version != current.plan_version + 1
                        || v.round != current.round))
            {
                return Err(invalid());
            }
            db.execute(
                "INSERT INTO grill_plans VALUES (?,?,?,?,?)",
                params![id.to_string(), v.version, v.round, seq, v.text],
            )?;
        }
        EnvelopePayload::GrillQuestion(v) => {
            v.validate().map_err(|_| invalid())?;
            if current.state != GrillStatus::Questioning
                || v.round != current.round
                || v.turn != current.turn
                || v.leader != current.leader
                || v.griller != current.griller_order[current.turn as usize]
                || v.plan_version != current.plan_version
            {
                return Err(invalid());
            }
            db.execute("INSERT INTO grill_turns(contest_id,round,turn,griller,angle,plan_version,question_event_seq,state_json) VALUES (?,?,?,?,?,?,?,?)",params![id.to_string(),v.round,v.turn+1,v.griller.to_string(),v.angle,v.plan_version,seq,serde_json::to_string(v)?])?;
        }
        EnvelopePayload::GrillAnswer(v) => {
            if current.state != GrillStatus::Answering
                || v.round != current.round
                || v.turn != current.turn
            {
                return Err(invalid());
            }
            let question = crate::events::by_id(db, v.question_event_id)?.ok_or_else(invalid)?;
            let EnvelopePayload::GrillQuestion(q) = &question.content else {
                return Err(invalid());
            };
            v.validate_against(q, question.base.id, frozen.answer_words)
                .map_err(|_| invalid())?;
            if db.execute("UPDATE grill_turns SET answer_event_seq=? WHERE contest_id=? AND round=? AND turn=? AND question_event_seq=? AND answer_event_seq IS NULL AND pass_event_seq IS NULL",params![seq,id.to_string(),v.round,v.turn+1,question.commit.seq.get()])?!=1 {return Err(invalid());}
        }
        EnvelopePayload::GrillPass(v) => {
            if current.state != GrillStatus::Questioning
                || v.round != current.round
                || v.turn != current.turn
                || v.griller != current.griller_order[current.turn as usize]
            {
                return Err(invalid());
            }
            db.execute("INSERT INTO grill_turns(contest_id,round,turn,griller,angle,plan_version,pass_event_seq,state_json) VALUES (?,?,?,?,?,?,?,?)",params![id.to_string(),v.round,v.turn+1,v.griller.to_string(),"",current.plan_version,seq,serde_json::to_string(v)?])?;
        }
        EnvelopePayload::GrillComplete(v) => {
            let found:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM grill_plans p JOIN events e ON e.seq=p.event_seq WHERE p.contest_id=? AND p.version=? AND e.id=?)",params![id.to_string(),v.final_plan_version,v.final_plan_event_id.to_string()],|r|r.get(0))?;
            if !found {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    crate::contests::associate(db, id, ContestKind::Grill, event.commit.seq)?;
    for nick in frozen.setup.participants() {
        crate::history::project(
            db,
            &crate::history::attributed(
                event,
                ContextId(nick.to_string()),
                "grill.floor",
                HistoryRole::User,
                None,
                serde_json::to_string(&event.content)?,
            ),
        )?;
    }
    Ok(())
}

/// New step contexts receive only the latest plan, current-round exchanges and
/// prior questions. Earlier answers stay in the durable participant histories.
pub fn projection(
    db: &Connection,
    id: ContestId,
    context: ContextId,
) -> Result<Vec<HistoryProjection>> {
    let current = state(db, id)?;
    let mut result = Vec::new();
    for event in events(db, id)? {
        let text=match &event.content{
            EnvelopePayload::GrillSetup(v)=>Some(serde_json::json!({"leader":v.leader,"grillers":v.grillers,"focus":v.focus,"rounds":v.total_rounds}).to_string()),
            EnvelopePayload::GrillPlan(v) if v.version==current.plan_version=>Some(serde_json::to_string(v)?),
            EnvelopePayload::GrillQuestion(v) if v.round<current.round=>Some(serde_json::json!({"alreadyAsked":v.questions}).to_string()),
            EnvelopePayload::GrillQuestion(v) if v.round==current.round=>Some(serde_json::to_string(v)?),
            EnvelopePayload::GrillAnswer(v) if v.round==current.round=>Some(serde_json::to_string(v)?),
            EnvelopePayload::GrillPass(v) if v.round==current.round=>Some(serde_json::to_string(v)?),
            _=>None,
        };
        if let Some(text) = text {
            result.push(crate::history::attributed(
                &event,
                context.clone(),
                "grill.working",
                HistoryRole::User,
                None,
                text,
            ));
        }
    }
    Ok(result)
}

pub fn work_input(db: &Connection, state: &GrillState) -> Result<GrillWorkInput> {
    let frozen = setup(db, state.contest_id)?;
    let mut question = None;
    let mut earlier_questions = vec![];
    for e in events(db, state.contest_id)? {
        if let EnvelopePayload::GrillQuestion(q) = e.content {
            earlier_questions.extend(q.questions.clone());
            if q.round == state.round && q.turn == state.turn {
                question = Some((e.base.id, q));
            }
        }
    }
    Ok(GrillWorkInput {
        state: state.clone(),
        question,
        earlier_questions,
        answer_words: frozen.answer_words,
        plan_words: frozen.plan_words,
    })
}
