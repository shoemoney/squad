//! Standalone judging allocates a fresh case and never member reply work.
use rusqlite::Transaction;
use squad_protocol::*;
use squad_store::{Result, StoreError, commit::CommitGroup};
pub fn standalone(
    tx: &Transaction<'_>,
    group: &mut CommitGroup,
    id: EventId,
    input: UserMessageInput,
    question: String,
    quotes: Vec<ResolvedQuote>,
    binding: &ExecutorBinding,
) -> Result<()> {
    if quotes.is_empty() {
        return Err(StoreError::Domain(
            ErrorCode::JUDGE_QUOTE_REQUIRED,
            "Judge requires quoted material",
        ));
    }
    let case = CaseId::new();
    let work_id = WorkId::new();
    let invocation = EventId::new();
    let mut user = super::envelope(
        id,
        Actor::Me,
        vec![Recipient::Judge],
        EnvelopePayload::UserMessage(UserMessage {
            input,
            body: question.clone(),
            resolved_quotes: quotes.clone(),
            debate_id: None,
            case_id: Some(case),
            contest_id: None,
        }),
    );
    user.base.id = id;
    let user_seq = group.push(tx, user)?;
    let source = squad_store::events::at(tx, user_seq.get())?;
    let opened = CaseOpened {
        case_id: case,
        judge_invocation_id: invocation,
        work_id: Some(work_id),
        judge_model: binding.model.clone(),
        material: CaseMaterial::Standalone {
            messages: quotes.clone(),
            question: Text::new(question.clone())
                .map_err(|_| StoreError::Domain(ErrorCode::INVALID_COMMAND, "Question is empty"))?,
        },
        debate_id: None,
        contest_id: None,
    };
    let seq = group.push(
        tx,
        super::envelope(
            id,
            Actor::System,
            vec![Recipient::Me],
            EnvelopePayload::CaseOpened(opened.clone()),
        ),
    )?;
    squad_store::cases::open(tx, &opened, seq)?;
    squad_store::cases::associate(tx, case, user_seq)?;
    let context = ContextId(format!("judge:{case}"));
    let boundary = squad_store::history::project(
        tx,
        &squad_store::history::attributed(
            &source,
            context.clone(),
            "case.material",
            HistoryRole::User,
            None,
            super::references::content(&question, &quotes),
        ),
    )?
    .ok_or(StoreError::Unavailable)?;
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
    squad_store::work::insert(tx, &work, binding, None)
}
