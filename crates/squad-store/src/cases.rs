use crate::{
    Result,
    error::{encode, invalid},
};
use rusqlite::{Connection, params};
use squad_protocol::*;
pub fn open(db: &Connection, opened: &CaseOpened, seq: Cursor) -> Result<()> {
    let contest = match &opened.material {
        CaseMaterial::Debate { setup, .. } => Some(setup.debate_id.to_string()),
        _ => opened
            .contest_id
            .or(opened.debate_id)
            .map(|id| id.to_string()),
    };
    let prior = if let Some(contest) = &contest {
        let raw: Option<String> = db.query_row(
            "SELECT pending_request_json FROM contests WHERE contest_id=?",
            [contest],
            |r| r.get(0),
        )?;
        raw.map(|r| serde_json::from_str::<serde_json::Value>(&r))
            .transpose()?
            .and_then(|v| v.get("priorState").cloned())
    } else {
        None
    };
    db.execute("INSERT INTO cases(case_id,context_id,epoch,contest_id,judge_model,material_json,opened_event_seq,state_json) VALUES (?1,?2,(SELECT epoch FROM room_meta),?3,?4,?5,?6,?7)",params![opened.case_id.to_string(),format!("judge:{}",opened.case_id),contest,opened.judge_model.as_str(),encode(&opened.material)?,seq.get(),serde_json::json!({"judgeInvocationId":opened.judge_invocation_id,"priorTerminalStatus":prior.as_ref().and_then(|v|v.get("state")),"priorState":prior}).to_string()])?;
    associate(db, opened.case_id, seq)
}
pub fn associate(db: &Connection, id: CaseId, seq: Cursor) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO case_events VALUES (?,?)",
        params![id.to_string(), seq.get()],
    )?;
    Ok(())
}
pub fn seal(db: &Connection, id: CaseId, seq: Cursor) -> Result<()> {
    if db.execute(
        "UPDATE cases SET decision_event_seq=? WHERE case_id=? AND decision_event_seq IS NULL",
        params![seq.get(), id.to_string()],
    )? != 1
    {
        return Err(invalid("Case already sealed"));
    }
    associate(db, id, seq)
}

/// Reopen exactly the sealed case explicitly selected by a verdict/ruling quote.
/// Original material and decision remain immutable. Returns false for ordinary
/// material quotes so the caller can allocate a fresh standalone case instead.
pub fn followup(
    tx: &rusqlite::Transaction<'_>,
    group: &mut crate::commit::CommitGroup,
    id: EventId,
    input: UserMessageInput,
    question: String,
    quotes: Vec<ResolvedQuote>,
    binding: &ExecutorBinding,
) -> Result<bool> {
    use rusqlite::OptionalExtension;
    let mut selected = None;
    for quote in &quotes {
        let source = crate::events::by_id(tx, quote.event_id)?
            .ok_or_else(|| invalid("Missing case quote"))?;
        let case = match source.content {
            EnvelopePayload::Verdict(v) => Some(v.case_id),
            EnvelopePayload::CaseRuling(r) => Some(r.case_id),
            _ => None,
        };
        if let Some(case) = case {
            if selected.is_some_and(|old| old != case) {
                return Err(crate::StoreError::Domain(
                    ErrorCode::CASE_REFERENCE_AMBIGUOUS,
                    "References select different cases",
                ));
            }
            selected = Some(case);
        }
    }
    let Some(case) = selected else {
        return Ok(false);
    };
    let sealed:Option<bool>=tx.query_row("SELECT decision_event_seq IS NOT NULL FROM cases WHERE case_id=? AND epoch=(SELECT epoch FROM room_meta)",[case.to_string()],|r|r.get(0)).optional()?;
    if sealed != Some(true) {
        return Err(crate::StoreError::Domain(
            ErrorCode::CASE_NOT_FOUND,
            "Case not found",
        ));
    }
    let accepted_at = group.accepted_at;
    let make = |content| ProducerEnvelope {
        base: EnvelopeBase {
            version: 1,
            id: EventId::new(),
            room_id: MainRoom::Main,
            correlation_id: id,
            causation_id: Some(id),
            sender: Actor::System,
            recipients: vec![Recipient::Me],
            created_at: accepted_at,
        },
        content,
    };
    let mut user = make(EnvelopePayload::UserMessage(UserMessage {
        input,
        body: question.clone(),
        resolved_quotes: quotes.clone(),
        debate_id: None,
        case_id: Some(case),
        contest_id: None,
    }));
    user.base.id = id;
    user.base.sender = Actor::Me;
    user.base.recipients = vec![Recipient::Judge];
    let seq = group.push(tx, user)?;
    associate(tx, case, seq)?;
    let source = crate::events::at(tx, seq.get())?;
    let work_id = WorkId::new();
    let invocation = EventId::new();
    let seq = group.push(
        tx,
        make(EnvelopePayload::CaseFollowup(CaseFollowup {
            case_id: case,
            user_event_id: id,
            judge_invocation_id: invocation,
            work_id,
        })),
    )?;
    associate(tx, case, seq)?;
    let mut content = question;
    for quote in quotes {
        content.push_str(&format!("\n\n> {}: {}", quote.author, quote.text));
    }
    let context = ContextId(format!("judge:{case}"));
    let boundary = crate::history::project(
        tx,
        &crate::history::attributed(
            &source,
            context.clone(),
            "case.followup",
            HistoryRole::User,
            None,
            content,
        ),
    )?
    .ok_or(crate::StoreError::Unavailable)?;
    let work = Work {
        work_id,
        message_id: MessageId::new(),
        in_reply_to_message_id: None,
        kind: WorkKind::Judge,
        target: Target::Judge,
        context_id: context,
        trigger_event_id: id,
        history_through: boundary,
        attempt: 1,
        debate_id: None,
        round: None,
        replay: None,
        case_id: Some(case),
        prompt_ref: None,
        max_output_tokens: Some(constants::FINAL_JUDGE_VISIBLE_TOKENS),
        contest_id: None,
        step_id: None,
    };
    crate::work::insert(tx, &work, binding, None)?;
    Ok(true)
}

/// Follow-up rulings append to a sealed case without replacing its decision.
pub fn accept_ruling(db: &Connection, work: &Work, seq: Cursor) -> Result<()> {
    let followup:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM case_events c JOIN events e ON e.seq=c.event_seq WHERE c.case_id=? AND e.kind='case.followup' AND json_extract(e.payload,'$.userEventId')=?)",params![work.case_id.map(|id|id.to_string()),work.trigger_event_id.to_string()],|r|r.get(0))?;
    let case = work.case_id.ok_or_else(|| invalid("Ruling has no case"))?;
    if followup {
        associate(db, case, seq)
    } else {
        seal(db, case, seq)
    }
}

/// Match the invocation frozen when this work's case or follow-up was opened.
pub fn validate_invocation(db: &Connection, work: &Work, payload: &EnvelopePayload) -> Result<()> {
    use rusqlite::OptionalExtension;
    let invocation = match payload {
        EnvelopePayload::Verdict(v) => v.judge_invocation_id,
        EnvelopePayload::CaseRuling(r) => r.judge_invocation_id,
        _ => return Ok(()),
    };
    let followup:Option<String>=db.query_row("SELECT json_extract(e.payload,'$.judgeInvocationId') FROM case_events c JOIN events e ON c.event_seq=e.seq WHERE c.case_id=? AND e.kind='case.followup' AND json_extract(e.payload,'$.workId')=?",params![work.case_id.map(|c|c.to_string()),work.work_id.to_string()],|r|r.get(0)).optional()?;
    let expected = if let Some(id) = followup {
        id
    } else {
        db.query_row(
            "SELECT json_extract(state_json,'$.judgeInvocationId') FROM cases WHERE case_id=?",
            [work.case_id.map(|c| c.to_string())],
            |r| r.get::<_, String>(0),
        )?
    };
    if invocation.to_string() != expected {
        return Err(crate::StoreError::Domain(
            ErrorCode::PRODUCER_FORBIDDEN,
            "Judge invocation does not match case work",
        ));
    }
    Ok(())
}
