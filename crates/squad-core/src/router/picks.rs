//! Question-first collection remains ordinary private reply traffic.
use rusqlite::{OptionalExtension, Transaction, params};
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};

pub fn collect(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    parsed: ParsedInput,
    bindings: &std::collections::BTreeMap<String, ExecutorBinding>,
) -> Result<()> {
    let ParsedInput::QuestionFirst {
        input: conversation,
        mode,
        candidates,
        question,
        rounds,
        ..
    } = parsed
    else {
        return Err(super::debates::invalid());
    };
    let kind = if mode == parser::QuestionFirstMode::Grill {
        ContestKind::Grill
    } else {
        ContestKind::Debate
    };
    if candidates
        .iter()
        .any(|nick| !bindings.contains_key(nick.as_str()))
    {
        return Err(StoreError::Domain(
            ErrorCode::MODEL_UNAVAILABLE,
            "Selected member model is unavailable",
        ));
    }
    let contest = ContestId::new();
    let state = ContestSelectionState {
        contest_id: contest,
        kind,
        state: ContestSelectionStatus::CollectingAnswers,
        state_version: 1,
        question_message_id: input.message_id,
        addressed: candidates.clone(),
        answers: vec![],
        required_picks: if kind == ContestKind::Grill { 1 } else { 2 },
    };
    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contests WHERE active=1)",
        [],
        |r| r.get(0),
    )?;
    if active {
        return Err(StoreError::Domain(
            ErrorCode::DEBATE_BUSY,
            "A debate is already running. Stop it before starting another.",
        ));
    }
    tx.execute("INSERT INTO contests(contest_id,kind,epoch,state_version,state_json,active,pending_request_json) VALUES (?, ?,(SELECT epoch FROM room_meta),1,?,1,?)",params![contest.to_string(),kind.as_str(),serde_json::to_string(&state)?,serde_json::json!({"rounds":rounds}).to_string()])?;
    super::variables::apply(tx, group, id, &conversation.variable_updates)?;
    let quotes = super::references::quotes(tx, &conversation.quote)?;
    let mut event = super::envelope(
        id,
        Actor::Me,
        candidates.iter().cloned().map(Recipient::Member).collect(),
        EnvelopePayload::UserMessage(UserMessage {
            input: input.clone(),
            body: question.clone(),
            resolved_quotes: quotes.clone(),
            debate_id: None,
            case_id: None,
            contest_id: Some(contest),
        }),
    );
    event.base.id = id;
    let seq = group.push(tx, event)?;
    squad_store::contests::associate(tx, contest, kind, seq)?;
    let source = squad_store::events::at(tx, seq.get())?;
    group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestCollecting(ContestCollecting {
                contest_id: contest,
                kind,
                question_message_id: input.message_id,
                question: question.clone(),
                addressed: candidates.clone(),
            }),
        ),
    )?;
    for nick in candidates {
        let work = Work {
            work_id: WorkId::new(),
            message_id: MessageId::new(),
            in_reply_to_message_id: Some(input.message_id),
            kind: WorkKind::Reply,
            target: Target::Member(nick.clone()),
            context_id: ContextId(nick.to_string()),
            trigger_event_id: id,
            history_through: HistoryPosition::new(1).expect("positive"),
            attempt: 1,
            debate_id: None,
            round: None,
            replay: None,
            case_id: None,
            prompt_ref: None,
            max_output_tokens: None,
            contest_id: Some(contest),
            step_id: None,
        };
        squad_store::history::project(
            tx,
            &squad_store::history::attributed(
                &source,
                work.context_id.clone(),
                "ordinary.input",
                HistoryRole::User,
                None,
                super::references::content(&question, &quotes),
            ),
        )?;
        squad_store::work::insert(tx, &work, &bindings[nick.as_str()], None)?;
        tx.execute(
            "INSERT INTO contest_answers(contest_id,nick,work_id,outcome) VALUES (?,?,?,'pending')",
            params![contest.to_string(), nick.as_str(), work.work_id.to_string()],
        )?;
    }
    Ok(())
}
pub fn refresh(tx: &Transaction<'_>, group: &mut CommitGroup, id: EventId) -> Result<()> {
    let contest:Option<String>=tx.query_row("SELECT contest_id FROM contests WHERE active=1 AND json_extract(state_json,'$.state')='COLLECTING_ANSWERS'",[],|r|r.get(0)).optional()?;
    let Some(contest) = contest else {
        return Ok(());
    };
    squad_store::contests::picks::settle_answers(
        tx,
        contest.parse().map_err(|_| StoreError::Unavailable)?,
    )?;
    let pending: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM contest_answers WHERE contest_id=? AND outcome='pending')",
        [&contest],
        |r| r.get(0),
    )?;
    if pending {
        return Ok(());
    }
    let raw: String = tx.query_row(
        "SELECT state_json FROM contests WHERE contest_id=?",
        [&contest],
        |r| r.get(0),
    )?;
    let mut state: ContestSelectionState = serde_json::from_str(&raw)?;
    for nick in &state.addressed {
        let seq: Option<i64> = tx.query_row(
            "SELECT answer_event_seq FROM contest_answers WHERE contest_id=? AND nick=?",
            params![&contest, nick.as_str()],
            |r| r.get(0),
        )?;
        if let Some(seq) = seq {
            let event = squad_store::events::at(tx, seq)?;
            if let EnvelopePayload::MessageFinal(reply) = event.content {
                state.answers.push(ContestAnswer {
                    nick: nick.clone(),
                    message_id: reply.completed.generation.message_id,
                    text: reply.content,
                });
            }
        }
    }
    let enough = state.answers.len() >= 2;
    state.state = if enough {
        ContestSelectionStatus::AwaitingPick
    } else {
        ContestSelectionStatus::Failed
    };
    state.state_version += 1;
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestState(ContestState::Selection(state.clone())),
        ),
    )?;
    squad_store::contests::picks::save_selection(tx, &state, seq)?;
    if enough {
        let eligible = state
            .answers
            .iter()
            .map(|a| a.nick.clone())
            .collect::<Vec<_>>();
        let seq = group.push(
            tx,
            super::envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::ContestPickRequested(ContestPickRequested {
                    contest_id: state.contest_id,
                    kind: state.kind,
                    question_message_id: state.question_message_id,
                    eligible: eligible.clone(),
                    required_picks: state.required_picks,
                    banner: if state.kind == ContestKind::Grill {
                        "CHOOSE WHO GETS GRILLED"
                    } else {
                        "CHOOSE YOUR DEBATE BROS"
                    }
                    .into(),
                }),
            ),
        )?;
        squad_store::contests::picks::request_pick(
            tx,
            state.contest_id,
            seq,
            &eligible,
            state.required_picks,
        )?;
    } else {
        group.push(
            tx,
            super::envelope(
                id,
                Actor::System,
                vec![Recipient::Me],
                EnvelopePayload::Error(super::error(
                    ErrorCode::CONTEST_NOT_ENOUGH_ANSWERS,
                    format!(
                        "Not enough answers to start. {} of {} members answered.",
                        state.answers.len(),
                        state.addressed.len()
                    ),
                )),
            ),
        )?;
    }
    Ok(())
}
pub fn pick(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    contest: ContestId,
    nicks: Vec<MemberNick>,
    judge_model: String,
) -> Result<()> {
    let raw:Option<String>=tx.query_row("SELECT state_json FROM contests WHERE contest_id=? AND active=1 AND json_extract(state_json,'$.state')='AWAITING_PICK'",[contest.to_string()],|r|r.get(0)).optional()?;
    let state: ContestSelectionState = serde_json::from_str(&raw.ok_or(StoreError::Domain(
        ErrorCode::CONTEST_NOT_FOUND,
        "Contest not found",
    ))?)?;
    if nicks.len() != 2
        || nicks[0] == nicks[1]
        || nicks
            .iter()
            .any(|n| !state.answers.iter().any(|a| a.nick == *n))
    {
        return Err(StoreError::Domain(
            ErrorCode::CONTEST_PICK_INVALID,
            "Pick exactly two eligible members.",
        ));
    }
    let sources = nicks
        .iter()
        .map(|n| {
            state
                .answers
                .iter()
                .find(|a| a.nick == *n)
                .expect("eligible")
                .message_id
        })
        .collect();
    let picked = ContestPicked {
        contest_id: contest,
        kind: ContestKind::Debate,
        nicks: nicks.clone(),
        source_message_ids: sources,
    };
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::ContestPicked(picked.clone()),
        ),
    )?;
    squad_store::contests::picks::accept(tx, &picked, seq)?;
    let pending: String = tx.query_row(
        "SELECT pending_request_json FROM contests WHERE contest_id=?",
        [contest.to_string()],
        |r| r.get(0),
    )?;
    let rounds = serde_json::from_str::<serde_json::Value>(&pending)?["rounds"]
        .as_u64()
        .ok_or_else(super::debates::invalid)? as u32;
    // Picking reuses the collection's committed answers. Freeze the same
    // admission shape consumed by direct debates, without emitting the question
    // again or sending another ordinary reply request.
    let question = squad_store::events::by_id(
        tx,
        state
            .question_message_id
            .to_string()
            .parse()
            .map_err(|_| StoreError::Unavailable)?,
    )?
    .ok_or_else(super::debates::invalid)?;
    let EnvelopePayload::UserMessage(question) = question.content else {
        return Err(super::debates::invalid());
    };
    let mut selected = Vec::new();
    for source in &picked.source_message_ids {
        let seq:i64=tx.query_row("SELECT seq FROM events WHERE kind='message.final' AND json_extract(payload,'$.messageId')=?",[source.to_string()],|r|r.get(0))?;
        selected.push(squad_store::events::at(tx, seq)?);
    }
    let parsed = ParsedInput::Debate {
        input: parser::ConversationalInput {
            raw: question.input.text.to_string(),
            quote: vec![],
            variable_updates: vec![],
        },
        debaters: [nicks[0].clone(), nicks[1].clone()],
        rounds: rounds as u8,
        instruction: question.body,
        judge_requested: true,
        reply_to: vec![],
    };
    let request = Request::UserMessage {
        input: question.input,
        parsed,
        preconfirmed: false,
    };
    squad_store::contest_steps::private_map(
        tx,
        contest,
        "admission",
        &serde_json::json!({"request":request,"sources":selected,"quotes":question.resolved_quotes}),
    )?;
    let admitted = DebateState {
        debate_id: contest,
        state: DebateStatus::Preparing,
        debaters: [nicks[0].clone(), nicks[1].clone()],
        round: 1,
        total_rounds: rounds,
        judge_model,
        judge_requested: true,
        state_version: state.state_version + 1,
        terminal_reason: None,
        case_id: None,
        case_ids: vec![],
    };
    super::debates::save(tx, group, id, &admitted, Some(state.state_version))
}
