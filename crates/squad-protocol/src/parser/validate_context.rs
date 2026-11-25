//! Pure authoritative-reference validation. Call again with fresh summaries at admission.
use super::{errors::Rule, *};
use crate::{ContestKind, EventId, MessageId, Nick};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordKind {
    MemberReply {
        author: Nick,
        to_me: bool,
        question: MessageId,
    },
    GrillPlan,
    Verdict,
    Other,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSummary {
    pub id: EventId,
    pub kind: RecordKind,
    /// False for failed, superseded, cancelled, or otherwise ineligible source records.
    pub eligible: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParallelAnswerSet {
    pub question: MessageId,
    pub answers: Vec<EventId>,
}
#[derive(Debug, Clone, Default)]
pub struct ParseContext {
    pub roster: Vec<Nick>,
    pub variables: RoomVariables,
    pub metadata: SubmissionMetadata,
    pub pending_pick: Option<PendingPick>,
    /// Oldest to newest; only summaries, never executable source text.
    pub records: Vec<RecordSummary>,
    /// One stored batch, never a union of unrelated latest replies.
    pub latest_parallel_answers: Option<ParallelAnswerSet>,
    pub active_contest: Option<ContestKind>,
}
impl ParseContext {
    pub fn record(&self, id: EventId) -> Option<&RecordSummary> {
        self.records.iter().find(|r| r.id == id)
    }
    pub fn has_latest_reply(&self, nick: &Nick) -> bool {
        self.records.iter().rev().any(|r| reply_from(r, nick))
    }
}
fn reply_from(record: &RecordSummary, nick: &Nick) -> bool {
    record.eligible
        && matches!(&record.kind, RecordKind::MemberReply {author, to_me: true, ..} if author == nick)
}
fn eligible_plan(record: &RecordSummary) -> bool {
    record.eligible
        && matches!(
            record.kind,
            RecordKind::MemberReply { to_me: true, .. } | RecordKind::GrillPlan
        )
}

pub fn validate_pick(raw: &str, nicks: &[Nick], pending: &PendingPick) -> Result<(), ParseError> {
    let span = Span {
        start: 0,
        end: raw.len(),
    };
    let (count, rule) = match pending.mode {
        QuestionFirstMode::Debate => (2, Rule::DebatePick),
        QuestionFirstMode::Grill => (1, Rule::GrillPick),
    };
    let mut distinct = Vec::new();
    for nick in nicks {
        if !distinct.contains(nick) {
            distinct.push(nick.clone());
        }
    }
    if distinct.len() != count {
        let eligible = pending
            .eligible
            .iter()
            .map(|n| format!("@{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(rule.error(span, &[("eligible", &eligible)]));
    }
    for nick in nicks {
        if !pending.eligible.contains(nick) {
            return Err(Rule::PickIneligible.error(
                lexer::tokenize(raw)?
                    .iter()
                    .find(|t| matches!(&t.kind, lexer::TokenKind::Mention(n) if n == nick.as_str()))
                    .map_or(span, |t| t.span.clone()),
                &[("nick", nick.as_str())],
            ));
        }
    }
    Ok(())
}

/// Revalidate a parsed submission against an authoritative room snapshot.
/// Returns a fresh AST so failure cannot partially mutate the caller's staged data.
pub fn validate_context(parsed: &ParsedInput, ctx: &ParseContext) -> ParseResult {
    let mut parsed = parsed.clone();
    let raw = parsed.raw().to_owned();
    let span = Span {
        start: 0,
        end: raw.len(),
    };
    let members: Vec<&Nick> = match &parsed {
        ParsedInput::Message { recipients, .. } => recipients.iter().collect(),
        ParsedInput::Debate { debaters, .. } => debaters.iter().collect(),
        ParsedInput::Tournament { entrants, .. } => entrants.iter().collect(),
        ParsedInput::Grill {
            leader, grillers, ..
        } => std::iter::once(leader).chain(grillers.iter()).collect(),
        ParsedInput::QuestionFirst { candidates, .. } => candidates.iter().collect(),
        ParsedInput::Census { participants, .. }
        | ParsedInput::Crosscheck { participants, .. }
        | ParsedInput::Survey { participants, .. }
        | ParsedInput::Probe { participants, .. } => participants.iter().collect(),
        _ => Vec::new(),
    };
    for member in members {
        if !ctx.roster.contains(member) {
            return Err(Rule::UnknownRecipient.error(span.clone(), &[("nick", member.as_str())]));
        }
    }
    let quotes = parsed
        .conversational()
        .map_or(&[][..], |i| i.quote.as_slice());
    for id in quotes {
        if ctx.record(*id).is_none() {
            return Err(Rule::QuoteUnavailable.error(span.clone(), &[("id", &id.to_string())]));
        }
    }
    let census = matches!(parsed, ParsedInput::Census { .. });
    match &mut parsed {
        ParsedInput::ContestPick {
            contest_id, nicks, ..
        } => {
            let pending = ctx
                .pending_pick
                .as_ref()
                .filter(|p| p.contest_id == *contest_id)
                .ok_or_else(|| Rule::StalePick.error(span.clone(), &[]))?;
            validate_pick(&raw, nicks, pending)?;
        }
        ParsedInput::Debate {
            debaters, reply_to, ..
        } => {
            if reply_to.len() > 2 {
                return Err(Rule::TooManySelections.error(span.clone(), &[]));
            }
            let mut topic = None;
            let mut selected_authors = Vec::new();
            for id in reply_to.iter() {
                let fallback = debaters
                    .iter()
                    .find(|n| !selected_authors.contains(n))
                    .unwrap_or(&debaters[0]);
                let record = ctx.record(*id);
                let author = record
                    .and_then(|r| match &r.kind {
                        RecordKind::MemberReply {
                            author,
                            to_me: true,
                            ..
                        } if r.eligible
                            && debaters.contains(author)
                            && !selected_authors.contains(&author) =>
                        {
                            Some(author)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| {
                        Rule::SolutionInvalid.error(span.clone(), &[("nick", fallback.as_str())])
                    })?;
                selected_authors.push(author);
                if let Some(RecordSummary {
                    kind: RecordKind::MemberReply { question, .. },
                    ..
                }) = record
                {
                    if topic.is_some_and(|t| t != *question) {
                        return Err(Rule::TopicMismatch.error(span.clone(), &[]));
                    }
                    topic = Some(*question);
                }
            }
        }
        ParsedInput::Grill {
            input,
            leader,
            plan_selection,
            ..
        } => {
            if !input.quote.is_empty() {
                let selected = input
                    .quote
                    .iter()
                    .filter_map(|id| ctx.record(*id))
                    .filter(|r| reply_from(r, leader))
                    .collect::<Vec<_>>();
                if selected.len() != 1 {
                    return Err(
                        Rule::GrillSelection.error(span.clone(), &[("nick", leader.as_str())])
                    );
                }
                *plan_selection = Some(selected[0].id);
            } else if !ctx.has_latest_reply(leader) {
                return Err(Rule::GrillMissing.error(span.clone(), &[("nick", leader.as_str())]));
            }
        }
        ParsedInput::Census { .. } | ParsedInput::Crosscheck { .. } => {
            validate_plan(&parsed, ctx, census, span.clone())?;
        }
        ParsedInput::Survey {
            participants,
            source: SurveySource::LatestParallelAnswers,
            ..
        } if !ctx
            .latest_parallel_answers
            .as_ref()
            .is_some_and(|b| b.answers.iter().any(|id| ctx.record(*id).is_some_and(|r|
                r.eligible && matches!(&r.kind, RecordKind::MemberReply { author, to_me: true, question }
                    if *question == b.question && participants.contains(author))))) =>
        {
            return Err(Rule::SurveyMissing.error(span.clone(), &[]));
        }
        _ => {}
    }
    if parsed.is_contest()
        && let Some(kind) = ctx.active_contest.or_else(|| {
            ctx.pending_pick.as_ref().map(|p| match p.mode {
                QuestionFirstMode::Debate => ContestKind::Debate,
                QuestionFirstMode::Grill => ContestKind::Grill,
            })
        })
    {
        let requested = match parsed {
            ParsedInput::Debate { .. }
            | ParsedInput::QuestionFirst {
                mode: QuestionFirstMode::Debate,
                ..
            } => ContestKind::Debate,
            ParsedInput::Grill { .. }
            | ParsedInput::QuestionFirst {
                mode: QuestionFirstMode::Grill,
                ..
            } => ContestKind::Grill,
            ParsedInput::Tournament { .. } => ContestKind::Tournament,
            ParsedInput::Census { .. } => ContestKind::Census,
            ParsedInput::Crosscheck { .. } => ContestKind::Crosscheck,
            ParsedInput::Survey { .. } => ContestKind::Survey,
            ParsedInput::Probe { .. } => ContestKind::Probe,
            _ => unreachable!("contest checked above"),
        };
        return Err(errors::contest_busy(requested, kind, span));
    }
    Ok(parsed)
}
fn validate_plan(
    parsed: &ParsedInput,
    ctx: &ParseContext,
    census: bool,
    span: Span,
) -> Result<(), ParseError> {
    let (input, plan) = match parsed {
        ParsedInput::Census { input, plan, .. } | ParsedInput::Crosscheck { input, plan, .. } => {
            (input, plan)
        }
        _ => unreachable!(),
    };
    if !input.quote.is_empty() {
        let selected = input
            .quote
            .iter()
            .filter_map(|id| ctx.record(*id))
            .filter(|r| eligible_plan(r))
            .collect::<Vec<_>>();
        let valid = selected.len() == 1
            && matches!(plan, PlanSource::Quoted {event_id} if *event_id == selected[0].id);
        if !valid {
            return Err(if census {
                Rule::CensusSelection
            } else {
                Rule::CrosscheckSelection
            }
            .error(span, &[]));
        }
    } else if let PlanSource::LatestReply { nick } = plan
        && !ctx.has_latest_reply(nick)
    {
        return Err(if census {
            Rule::CensusLatest
        } else {
            Rule::CrosscheckLatest
        }
        .error(span, &[("nick", nick.as_str())]));
    }
    Ok(())
}

pub(crate) fn selected_plan(ctx: &ParseContext) -> Option<EventId> {
    let plans = ctx
        .metadata
        .quote
        .iter()
        .filter_map(|id| ctx.record(*id))
        .filter(|r| eligible_plan(r))
        .collect::<Vec<_>>();
    (plans.len() == 1).then(|| plans[0].id)
}
