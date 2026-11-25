//! Protocol-owned frontend snapshots. The parser never loads these records itself.
use super::{errors::Rule, *};
use crate::{
    Actor, CaseId, CellId, ContestId, ContestKind, ContestState, Cursor, EventId, MessageId, Nick,
    Recipient, Text,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct ParseContext {
    pub roster: Vec<Nick>,
    /// Effective values after stored-value/environment precedence; absent values use defaults.
    pub vars: BTreeMap<VariableName, u8>,
    pub pending_pick: Option<PendingPick>,
    pub records: Vec<RecordSummary>,
    pub active_contest: Option<ContestState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordSummary {
    pub event_id: EventId,
    pub message_id: Option<MessageId>,
    pub seq: Cursor,
    pub kind: Text,
    pub author: Actor,
    pub recipients: Vec<Recipient>,
    pub content: String,
    pub in_reply_to_message_id: Option<MessageId>,
    pub contest_id: Option<ContestId>,
    pub case_id: Option<CaseId>,
    pub plan_event_id: Option<EventId>,
    pub grid_event_id: Option<EventId>,
    pub cell_ids: Vec<CellId>,
    pub state_version: Option<u64>,
    pub superseded: bool,
    pub eligible: bool,
}

impl ParseContext {
    pub fn record(&self, id: EventId) -> Option<&RecordSummary> {
        self.records.iter().find(|r| r.event_id == id)
    }

    fn snapshot(&self, meta: &SubmissionMeta) -> ParserSnapshot {
        let mut variables = RoomVariables::default();
        for (name, value) in &self.vars {
            name.set(&mut variables, *value);
        }
        let mut records = self.records.iter().collect::<Vec<_>>();
        records.sort_by_key(|r| r.seq);
        let active_contest = self.active_contest.as_ref().map(|s| match s {
            ContestState::Generic(s) => s.kind,
            ContestState::Selection(s) => s.kind,
            ContestState::Debate(_) => ContestKind::Debate,
            ContestState::Tournament(_) => ContestKind::Tournament,
        });
        // A parallel batch is one human question, never a union of latest replies.
        let latest_question = records
            .iter()
            .rev()
            .find(|r| {
                r.kind.as_str() == "user.message"
                    && r.author == Actor::Me
                    && r.eligible
                    && !r.superseded
                    && r.recipients
                        .iter()
                        .filter(|r| matches!(r, Recipient::Member(_)))
                        .count()
                        > 1
                    && r.message_id.is_some_and(|question| {
                        records.iter().any(|answer| {
                            answer.kind.as_str() == "message.final"
                                && answer.in_reply_to_message_id == Some(question)
                                && answer.eligible
                                && !answer.superseded
                                && matches!(answer.author, Actor::Member(_))
                                && answer.recipients.contains(&Recipient::Me)
                        })
                    })
            })
            .and_then(|r| r.message_id);
        let latest_parallel_answers = latest_question.map(|question| ParallelAnswerSet {
            question,
            answers: records
                .iter()
                .filter(|r| r.in_reply_to_message_id == Some(question))
                .map(|r| r.event_id)
                .collect(),
        });
        ParserSnapshot {
            roster: self.roster.clone(),
            variables,
            metadata: meta.clone(),
            pending_pick: self.pending_pick.clone(),
            active_contest,
            latest_parallel_answers,
            records: records
                .into_iter()
                .map(|r| SourceSummary {
                    id: r.event_id,
                    kind: match (r.kind.as_str(), &r.author, r.in_reply_to_message_id) {
                        ("message.final", Actor::Member(author), Some(question)) => {
                            RecordKind::MemberReply {
                                author: author.clone(),
                                to_me: r.recipients.contains(&Recipient::Me),
                                question,
                            }
                        }
                        ("grill.plan", _, _) => RecordKind::GrillPlan,
                        ("debate.verdict", _, _) => RecordKind::Verdict,
                        _ => RecordKind::Other,
                    },
                    eligible: r.eligible && !r.superseded,
                })
                .collect(),
        }
    }
}

/// Shared deterministic frontend entry point; submission metadata is separate from room state.
pub fn parse(raw: &str, meta: &SubmissionMeta, ctx: &ParseContext) -> ParseResult {
    let parsed = super::parser::parse_snapshot(raw, &ctx.snapshot(meta))?;
    validate_control(&parsed, meta, ctx)?;
    Ok(parsed)
}

/// Recheck supplied AST references against fresh authoritative summaries.
pub fn validate_context(
    parsed: &ParsedInput,
    meta: &SubmissionMeta,
    ctx: &ParseContext,
) -> ParseResult {
    let parsed = validate_snapshot(parsed, &ctx.snapshot(meta))?;
    validate_control(&parsed, meta, ctx)?;
    Ok(parsed)
}

pub(crate) fn validate_control(
    parsed: &ParsedInput,
    meta: &SubmissionMeta,
    ctx: &ParseContext,
) -> Result<(), ParseError> {
    let span = Span {
        start: 0,
        end: parsed.raw().len(),
    };
    let ParsedInput::Slash { action, .. } = parsed else {
        return Ok(());
    };
    if meta.quote.len() > 12 {
        return Err(Rule::TooManyQuotes.error(span, &[]));
    }
    if !matches!(action, SlashAction::Continue { .. }) && !meta.quote.is_empty() {
        return Err(Rule::ControlQuotes.error(span, &[]));
    }
    if !meta.reply_to.is_empty() {
        return Err(Rule::SelectionOutside.error(span, &[]));
    }
    match action {
        SlashAction::Continue { plan } => {
            if plan
                .as_ref()
                .is_some_and(|p| p.trim().is_empty() || !meta.quote.is_empty())
            {
                return Err(Rule::Usage.error(span, &[("usage", super::parser::CONTINUE_USAGE)]));
            }
            // Resolve quote availability before contextual state, preserving attachment order.
            for id in &meta.quote {
                if ctx.record(*id).is_none() {
                    return Err(
                        Rule::QuoteUnavailable.error(span.clone(), &[("id", &id.to_string())])
                    );
                }
            }
            if !meta.quote.is_empty() && continuation_plan(meta, ctx).is_none() {
                return Err(Rule::ContinueSelection.error(span, &[]));
            }
            waiting_census(ctx).ok_or_else(|| Rule::CensusNotWaiting.error(span, &[]))?;
        }
        SlashAction::Accept { cell_ids, reason } => {
            let mut unique = std::collections::HashSet::new();
            if cell_ids.is_empty()
                || reason.trim().is_empty()
                || cell_ids.iter().any(|id| !unique.insert(id))
            {
                return Err(Rule::Usage.error(span, &[("usage", super::parser::ACCEPT_USAGE)]));
            }
            let record = complete_crosscheck(ctx)
                .ok_or_else(|| Rule::NoCompleteCrosscheck.error(span.clone(), &[]))?;
            let stale_active = ctx
                .active_contest
                .as_ref()
                .is_some_and(|state| match state {
                    ContestState::Generic(s) if Some(s.contest_id) == record.contest_id => {
                        s.kind != ContestKind::Crosscheck
                            || s.status.as_str() != "COMPLETE"
                            || Some(s.state_version) != record.state_version
                    }
                    _ => false,
                });
            let newer_material = ctx.records.iter().any(|r| {
                r.contest_id == record.contest_id
                    && r.seq > record.seq
                    && r.eligible
                    && !r.superseded
                    && match r.kind.as_str() {
                        "crosscheck.plan" => Some(r.event_id) != record.plan_event_id,
                        "crosscheck.grid" => Some(r.event_id) != record.grid_event_id,
                        _ => false,
                    }
            });
            if stale_active
                || newer_material
                || record.plan_event_id.is_none()
                || record.grid_event_id.is_none()
                || record.state_version.is_none()
                || cell_ids.iter().any(|id| !record.cell_ids.contains(id))
            {
                return Err(Rule::AcceptanceStale.error(span, &[]));
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn waiting_census(ctx: &ParseContext) -> Option<(ContestId, u64)> {
    match ctx.active_contest.as_ref()? {
        ContestState::Generic(s)
            if s.kind == ContestKind::Census && s.status.as_str() == "WAITING_FOR_PLAN" =>
        {
            Some((s.contest_id, s.state_version))
        }
        _ => None,
    }
}

pub(crate) fn continuation_plan(meta: &SubmissionMeta, ctx: &ParseContext) -> Option<EventId> {
    if meta.quote.len() != 1 {
        return None;
    }
    ctx.record(meta.quote[0])
        .filter(|r| {
            r.eligible
                && !r.superseded
                && (r.kind.as_str() == "grill.plan"
                    || r.kind.as_str() == "message.final"
                        && matches!(r.author, Actor::Member(_))
                        && r.recipients.contains(&Recipient::Me))
        })
        .map(|r| r.event_id)
}

pub(crate) fn complete_crosscheck(ctx: &ParseContext) -> Option<&RecordSummary> {
    // First select the latest state of each contest, then the latest COMPLETE target.
    ctx.records
        .iter()
        .filter(|r| {
            r.kind.as_str() == "crosscheck.state"
                && r.contest_id.is_some()
                && r.eligible
                && !r.superseded
                && !ctx.records.iter().any(|newer| {
                    newer.kind.as_str() == "crosscheck.state"
                        && newer.contest_id == r.contest_id
                        && newer.seq > r.seq
                })
        })
        .filter(|r| {
            serde_json::from_str::<serde_json::Value>(&r.content)
                .ok()
                .is_some_and(|v| v["status"] == "COMPLETE" || v["state"] == "COMPLETE")
        })
        .max_by_key(|r| r.seq)
}

#[cfg(test)]
pub(super) fn parse_fixture(raw: &str, snapshot: &ParserSnapshot) -> ParseResult {
    // Run every original fixture through the public protocol boundary too.
    let mut ctx = ParseContext {
        roster: snapshot.roster.clone(),
        vars: [
            (VariableName::Rounds, snapshot.variables.rounds),
            (VariableName::GrillRounds, snapshot.variables.grill_rounds),
            (VariableName::CensusPasses, snapshot.variables.census_passes),
        ]
        .into(),
        pending_pick: snapshot.pending_pick.clone(),
        active_contest: snapshot.active_contest.map(|kind| {
            crate::ContestState::Generic(crate::GenericContestState {
                contest_id: crate::ContestId::new(),
                kind,
                state_version: 1,
                status: Text::new("RUNNING").unwrap(),
                step_id: None,
                artifact_event_ids: vec![],
                terminal_reason: None,
            })
        }),
        records: vec![],
    };
    let batch = snapshot.latest_parallel_answers.as_ref();
    if let Some(batch) = batch {
        ctx.records.push(RecordSummary {
            event_id: EventId::new(),
            message_id: Some(batch.question),
            seq: Cursor::new(1).unwrap(),
            kind: Text::new("user.message").unwrap(),
            author: Actor::Me,
            recipients: ctx.roster.iter().cloned().map(Recipient::Member).collect(),
            content: String::new(),
            in_reply_to_message_id: None,
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: vec![],
            state_version: None,
            superseded: false,
            eligible: true,
        });
    }
    for (index, source) in snapshot.records.iter().enumerate() {
        let (kind, author, recipients, question) = match &source.kind {
            RecordKind::MemberReply {
                author,
                to_me,
                question,
            } => (
                "message.final",
                Actor::Member(author.clone()),
                if *to_me { vec![Recipient::Me] } else { vec![] },
                Some(*question),
            ),
            RecordKind::GrillPlan => ("grill.plan", Actor::System, vec![], None),
            RecordKind::Verdict => ("debate.verdict", Actor::Judge, vec![], None),
            RecordKind::Other => ("other", Actor::System, vec![], None),
        };
        // The supplied fixture's explicit batch membership remains authoritative.
        let eligible = source.eligible
            && batch.is_none_or(|b| question != Some(b.question) || b.answers.contains(&source.id));
        ctx.records.push(RecordSummary {
            event_id: source.id,
            message_id: None,
            seq: Cursor::new(index as i64 + 2).unwrap(),
            kind: Text::new(kind).unwrap(),
            author,
            recipients,
            content: String::new(),
            in_reply_to_message_id: question,
            contest_id: None,
            case_id: None,
            plan_event_id: None,
            grid_event_id: None,
            cell_ids: vec![],
            state_version: None,
            superseded: false,
            eligible,
        });
    }
    parse(raw, &snapshot.metadata, &ctx)
}
