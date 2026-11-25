use super::validate_context::selected_plan;
use super::{
    errors::Rule,
    lexer::{Token, TokenKind, tokenize},
    *,
};
use crate::Nick;

fn whole(raw: &str) -> Span {
    Span {
        start: 0,
        end: raw.len(),
    }
}
fn variable(name: &str, span: Span) -> Result<VariableName, ParseError> {
    VariableName::from_name(name)
        .ok_or_else(|| Rule::UnknownVariable.error(span, &[("name", name)]))
}
fn value(name: &VariableName, value: &str, span: Span) -> Result<u8, ParseError> {
    let (max, rule) = match name {
        VariableName::Rounds => (20, Rule::Rounds),
        VariableName::GrillRounds => (10, Rule::GrillRounds),
        VariableName::CensusPasses => (5, Rule::CensusPasses),
    };
    if !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && let Ok(n) = value.parse::<u8>()
        && (1..=max).contains(&n)
    {
        return Ok(n);
    }
    Err(rule.error(span, &[]))
}

/// Parse human text. All external facts are supplied by the caller; no I/O occurs.
pub(crate) fn parse_snapshot(raw: &str, ctx: &ParserSnapshot) -> ParseResult {
    if raw.trim().is_empty() {
        return Err(Rule::Empty.error(whole(raw), &[]));
    }
    if raw.len() > 32_768 {
        return Err(Rule::TooLarge.error(whole(raw), &[]));
    }
    if raw.trim_start().starts_with('/') {
        let action = slash(raw, ctx)?;
        attachments(
            raw,
            ctx,
            matches!(action, SlashAction::Continue { .. }),
            false,
            false,
        )?;
        return Ok(ParsedInput::Slash {
            raw: raw.to_owned(),
            action,
        });
    }
    let tokens = tokenize(raw)?;
    let mut command = None;
    for token in &tokens {
        if let TokenKind::Command(name) = &token.kind {
            if command.is_some() {
                return Err(Rule::MultipleCommands.error(token.span.clone(), &[]));
            }
            let verb = Verb::from_name(name)
                .ok_or_else(|| Rule::UnknownCommand.error(token.span.clone(), &[("verb", name)]))?;
            if verb.reserved() {
                return Err(Rule::Reserved.error(token.span.clone(), &[("verb", name)]));
            }
            command = Some(verb);
        }
    }
    // Category 3 errors compete by their original token positions.
    let conflict = (command == Some(Verb::Census) && !ctx.metadata.quote.is_empty())
        .then(|| {
            tokens.iter().find(|t| match &t.kind {
                TokenKind::Text => !raw[t.span.start..t.span.end].trim().is_empty(),
                TokenKind::Protected
                | TokenKind::Escape
                | TokenKind::Variable(_)
                | TokenKind::Separator => true,
                _ => false,
            })
        })
        .flatten();
    let mut vars = ctx.variables.clone();
    let mut updates: Vec<VariableUpdate> = Vec::new();
    let assignments: Result<(), ParseError> = (|| {
        for token in &tokens {
            match &token.kind {
                TokenKind::Variable(name) => {
                    variable(name, token.span.clone())?;
                }
                TokenKind::Assignment { name, value: v } => {
                    let name = variable(name, token.span.clone())?;
                    let v = value(&name, v, token.span.clone())?;
                    name.set(&mut vars, v);
                    updates.retain(|u| u.name != name);
                    updates.push(VariableUpdate { name, value: v });
                }
                _ => {}
            }
        }
        Ok(())
    })();
    if let Err(error) = assignments {
        if conflict.is_some_and(|t| t.span.start < error.span.start) {
            return Err(Rule::CensusConflict.error(whole(raw), &[]));
        }
        return Err(error);
    }
    if conflict.is_some() {
        return Err(Rule::CensusConflict.error(whole(raw), &[]));
    }
    let control = !updates.is_empty()
        && tokens.iter().all(|t| {
            matches!(t.kind, TokenKind::Assignment { .. })
                || matches!(t.kind, TokenKind::Text)
                    && raw[t.span.start..t.span.end].trim().is_empty()
        });
    if control {
        attachments(raw, ctx, false, false, false)?;
        return Ok(ParsedInput::Assignment {
            raw: raw.to_owned(),
            updates,
        });
    }
    let has_all = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Mention(n) if n == "all"));
    let has_judge = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Mention(n) if n == "judge"));
    let mut explicit = Vec::new();
    for t in &tokens {
        if let TokenKind::Mention(n) = &t.kind
            && n != "all"
            && n != "judge"
            && !explicit.contains(n)
        {
            explicit.push(n.clone());
        }
    }
    let clean = render(raw, &tokens, &vars, false);
    let question_first = match command {
        Some(Verb::Debate) => has_all || explicit.len() >= 3,
        Some(Verb::Grill) => has_all || explicit.len() >= 2 && !clean.is_empty(),
        _ => false,
    };
    attachments(
        raw,
        ctx,
        true,
        command == Some(Verb::Debate),
        question_first,
    )?;
    let resolved = resolve_recipients(raw, &tokens, &ctx.roster);
    let illegal_judge = has_judge
        && (command == Some(Verb::Grill)
            || command.is_none()
                && (ctx.metadata.quote.is_empty() || has_all || !explicit.is_empty()));
    if illegal_judge {
        let token = tokens
            .iter()
            .find(|t| matches!(&t.kind, TokenKind::Mention(n) if n == "judge"))
            .unwrap();
        if let Err(error) = &resolved {
            let standalone_override =
                command.is_none() && error.message == Rule::AllMixed.template();
            if !standalone_override && error.span.start < token.span.start {
                return Err(error.clone());
            }
        }
        return Err(if command == Some(Verb::Grill) {
            Rule::GrillJudge
        } else {
            Rule::JudgeQuote
        }
        .error(token.span.clone(), &[]));
    }
    let recipients = resolved?;
    let pick_candidate = command.is_none()
        && ctx.pending_pick.is_some()
        && ctx.metadata.quote.is_empty()
        && ctx.metadata.reply_to.is_empty()
        && tokens.iter().all(|t| {
            matches!(&t.kind, TokenKind::Mention(n) if n != "all" && n != "judge" && n != "me")
                || matches!(t.kind, TokenKind::Text)
                    && raw[t.span.start..t.span.end].trim().is_empty()
        });
    if pick_candidate {
        let pending = ctx.pending_pick.as_ref().unwrap();
        validate_pick(raw, &recipients, pending)?;
        return validate_snapshot(
            &ParsedInput::ContestPick {
                raw: raw.to_owned(),
                contest_id: pending.contest_id,
                nicks: recipients,
            },
            ctx,
        );
    }
    let input = ConversationalInput {
        raw: raw.to_owned(),
        quote: ctx.metadata.quote.clone(),
        variable_updates: updates,
    };
    let fail = |rule: Rule| rule.error(whole(raw), &[]);
    let parsed = match command {
        None if has_judge => {
            if clean.is_empty() {
                return Err(fail(Rule::BodyRequired));
            }
            ParsedInput::Judge {
                input,
                recipients: [JudgeRecipient::Judge],
                question: clean,
            }
        }
        None => {
            if recipients.is_empty() {
                return Err(fail(Rule::RecipientRequired));
            }
            if clean.is_empty() {
                return Err(fail(Rule::BodyRequired));
            }
            ParsedInput::Message {
                input,
                recipients,
                body: clean,
            }
        }
        Some(Verb::Debate | Verb::Grill) if question_first => {
            let debate = command == Some(Verb::Debate);
            if !debate && has_judge {
                return Err(fail(Rule::GrillJudge));
            }
            if recipients.len() < 2 {
                return Err(fail(if debate {
                    Rule::DebateCandidates
                } else {
                    Rule::GrillCandidates
                }));
            }
            if clean.is_empty() {
                return Err(fail(if debate {
                    Rule::DebateQuestion
                } else {
                    Rule::GrillQuestion
                }));
            }
            ParsedInput::QuestionFirst {
                input,
                mode: if debate {
                    QuestionFirstMode::Debate
                } else {
                    QuestionFirstMode::Grill
                },
                candidates: recipients,
                question: render(raw, &tokens, &vars, debate),
                rounds: if debate {
                    vars.rounds
                } else {
                    vars.grill_rounds
                },
                judge_requested: debate,
            }
        }
        Some(Verb::Debate) => {
            if recipients.len() != 2 {
                return Err(fail(Rule::DebatersRequired));
            }
            ParsedInput::Debate {
                input,
                debaters: [recipients[0].clone(), recipients[1].clone()],
                rounds: vars.rounds,
                instruction: render(raw, &tokens, &vars, true),
                judge_requested: has_judge,
                reply_to: ctx.metadata.reply_to.clone(),
            }
        }
        Some(Verb::Grill) => {
            if has_judge {
                return Err(fail(Rule::GrillJudge));
            }
            if recipients.is_empty() {
                return Err(fail(Rule::GrillSelector));
            }
            let leader = recipients[0].clone();
            let grillers = if recipients.len() == 1 {
                ctx.roster
                    .iter()
                    .filter(|n| **n != leader)
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                recipients[1..].to_vec()
            };
            if grillers.is_empty() {
                return Err(fail(Rule::GrillCount));
            }
            ParsedInput::Grill {
                input,
                leader,
                grillers,
                rounds: vars.grill_rounds,
                focus: clean,
                plan_selection: None,
            }
        }
        Some(Verb::Tournament) => {
            if recipients.len() < 3 {
                return Err(fail(Rule::TournamentCount));
            }
            if clean.is_empty() {
                return Err(fail(Rule::TournamentPrompt));
            }
            ParsedInput::Tournament {
                input,
                entrants: recipients,
                prompt: clean,
            }
        }
        Some(Verb::Census | Verb::Crosscheck) => {
            let census = command == Some(Verb::Census);
            let shortcut = !has_all && recipients.len() == 1;
            let participants = if shortcut {
                ctx.roster.clone()
            } else {
                recipients.clone()
            };
            if participants.len() < 3 {
                return Err(fail(if census {
                    Rule::CensusCount
                } else {
                    Rule::CrosscheckCount
                }));
            }
            let mut brief = None;
            let plan = if !ctx.metadata.quote.is_empty() {
                if !census {
                    brief = nonempty(clean.clone());
                }
                // Eligibility is category 5; retain a placeholder selection until reference checks.
                PlanSource::Quoted {
                    event_id: selected_plan(ctx).unwrap_or(ctx.metadata.quote[0]),
                }
            } else if shortcut && (!census || clean.is_empty()) {
                if !census {
                    brief = nonempty(clean.clone());
                }
                PlanSource::LatestReply {
                    nick: recipients[0].clone(),
                }
            } else {
                let mut plan_text = clean;
                if !census
                    && let Some(separator) = tokens
                        .iter()
                        .find(|t| matches!(t.kind, TokenKind::Separator))
                {
                    let before = tokens
                        .iter()
                        .filter(|t| t.span.end <= separator.span.start)
                        .cloned()
                        .collect::<Vec<_>>();
                    let after = tokens
                        .iter()
                        .filter(|t| t.span.start >= separator.span.end)
                        .cloned()
                        .collect::<Vec<_>>();
                    plan_text = render(raw, &before, &vars, false);
                    brief = nonempty(render(raw, &after, &vars, false));
                }
                if plan_text.is_empty() {
                    return Err(fail(if census {
                        Rule::CensusMissing
                    } else {
                        Rule::CrosscheckMissing
                    }));
                }
                PlanSource::Pasted { text: plan_text }
            };
            if census {
                ParsedInput::Census {
                    input,
                    participants,
                    plan,
                    passes: vars.census_passes,
                }
            } else {
                ParsedInput::Crosscheck {
                    input,
                    participants,
                    plan,
                    goal_constraints: brief,
                }
            }
        }
        Some(Verb::Survey) => {
            if recipients.is_empty() {
                return Err(fail(Rule::SurveyCount));
            }
            ParsedInput::Survey {
                input,
                participants: recipients,
                source: if clean.is_empty() {
                    SurveySource::LatestParallelAnswers
                } else {
                    SurveySource::Question { text: clean }
                },
            }
        }
        Some(Verb::Probe) => {
            if recipients.len() < 4 {
                return Err(fail(Rule::ProbeCount));
            }
            if clean.is_empty() {
                return Err(fail(Rule::ProbeBrief));
            }
            ParsedInput::Probe {
                input,
                participants: recipients,
                brief: clean,
            }
        }
        _ => unreachable!("commands were validated"),
    };
    validate_snapshot(&parsed, ctx)
}
fn nonempty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}
fn attachments(
    raw: &str,
    ctx: &ParserSnapshot,
    conversational: bool,
    debate: bool,
    question_first: bool,
) -> Result<(), ParseError> {
    let fail = |r: Rule| r.error(whole(raw), &[]);
    if ctx.metadata.quote.len() > 12 {
        return Err(fail(Rule::TooManyQuotes));
    }
    if !conversational && !ctx.metadata.quote.is_empty() {
        return Err(fail(Rule::ControlQuotes));
    }
    if !ctx.metadata.reply_to.is_empty() {
        if question_first {
            return Err(fail(Rule::QuestionSelection));
        }
        if !debate {
            return Err(fail(Rule::SelectionOutside));
        }
        if ctx.metadata.reply_to.len() > 2 {
            return Err(fail(Rule::TooManySelections));
        }
    }
    Ok(())
}

/// Expand selectors in roster order; explicit mentions preserve first occurrence.
pub fn resolve_recipients(
    raw: &str,
    tokens: &[Token],
    roster: &[Nick],
) -> Result<Vec<Nick>, ParseError> {
    let mut recipients = Vec::new();
    let mut all_seen = false;
    let mut explicit_seen = false;
    let mut all_span = None;
    for t in tokens {
        match &t.kind {
            TokenKind::Mention(name) if name == "judge" => {}
            TokenKind::Mention(name) if name == "me" => {
                return Err(Rule::Me.error(t.span.clone(), &[]));
            }
            TokenKind::Mention(name) if name == "all" => {
                if explicit_seen || all_seen {
                    return Err(Rule::AllMixed.error(t.span.clone(), &[]));
                }
                all_seen = true;
                all_span = Some(t.span.clone());
                if roster.is_empty() {
                    return Err(Rule::AllEmpty.error(t.span.clone(), &[]));
                }
                for n in roster {
                    if !recipients.contains(n) {
                        recipients.push(n.clone());
                    }
                }
            }
            TokenKind::Mention(name) | TokenKind::Exclusion(name) => {
                let nick = Nick::new(name)
                    .ok()
                    .filter(|n| roster.contains(n))
                    .ok_or_else(|| {
                        Rule::UnknownRecipient.error(t.span.clone(), &[("nick", name)])
                    })?;
                if matches!(t.kind, TokenKind::Exclusion(_)) {
                    recipients.retain(|n| *n != nick);
                    if recipients.is_empty() {
                        return Err(Rule::AllEmpty
                            .error(all_span.clone().unwrap_or_else(|| t.span.clone()), &[]));
                    }
                } else {
                    if all_seen {
                        return Err(Rule::AllMixed.error(t.span.clone(), &[]));
                    }
                    explicit_seen = true;
                    if !recipients.contains(&nick) {
                        recipients.push(nick);
                    }
                }
            }
            _ => {}
        }
    }
    if all_seen && recipients.is_empty() {
        return Err(Rule::AllEmpty.error(all_span.unwrap_or_else(|| whole(raw)), &[]));
    }
    Ok(recipients)
}

fn render(raw: &str, tokens: &[Token], vars: &RoomVariables, keep_judge: bool) -> String {
    let range = tokens.first().map_or(0, |t| t.span.start)..tokens.last().map_or(0, |t| t.span.end);
    let protected: Vec<_> = tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::Protected))
        .map(|t| &t.span)
        .collect();
    let mut removed: Vec<(usize, usize, bool)> = Vec::new();
    for t in tokens
        .iter()
        .filter(|t| t.span.start >= range.start && t.span.end <= range.end)
    {
        let remove = matches!(
            t.kind,
            TokenKind::Command(_) | TokenKind::Assignment { .. } | TokenKind::Exclusion(_)
        ) || matches!(&t.kind,TokenKind::Mention(n) if n!="judge" || !keep_judge);
        if remove {
            let mut start = t.span.start;
            let mut end = t.span.end;
            while start > range.start {
                let c = raw[..start].chars().next_back().unwrap();
                if !c.is_whitespace()
                    || protected
                        .iter()
                        .any(|span| span.start < start && span.end >= start)
                {
                    break;
                }
                if c == '\n'
                    && protected.iter().any(|span| {
                        span.end == start - 1 && raw[span.start..span.end].starts_with("```")
                    })
                {
                    break;
                }
                start -= c.len_utf8();
            }
            while end < range.end {
                let c = raw[end..].chars().next().unwrap();
                if !c.is_whitespace()
                    || protected
                        .iter()
                        .any(|span| span.start <= end && span.end > end)
                {
                    break;
                }
                end += c.len_utf8();
            }
            let left_whitespace =
                start < t.span.start || (start > range.start && raw[..start].ends_with('\n'));
            let ws = left_whitespace && end > t.span.end;
            // Only whitespace joined across a removal is normalized. A one-sided
            // newline or indentation remains ordinary text.
            if !ws {
                start = t.span.start;
                end = t.span.end;
            }
            if let Some(last) = removed.last_mut().filter(|last| last.1 >= start) {
                last.1 = last.1.max(end);
                last.2 |= ws;
            } else {
                removed.push((start, end, ws));
            }
        }
    }
    let mut out = String::new();
    let mut protected_end = 0;
    let mut i = range.start;
    let mut edits = removed.into_iter().peekable();
    let mut ts = tokens
        .iter()
        .filter(|t| t.span.start >= range.start)
        .peekable();
    while i < range.end {
        if edits.peek().is_some_and(|e| e.0 == i) {
            let (_, end, ws) = edits.next().unwrap();
            if ws && !out.ends_with('\n') {
                out.push(' ');
            }
            i = end;
            continue;
        }
        while ts.peek().is_some_and(|t| t.span.start < i) {
            ts.next();
        }
        if let Some(t) = ts.peek().filter(|t| t.span.start == i) {
            match &t.kind {
                TokenKind::Variable(n) => {
                    out.push_str(&VariableName::from_name(n).unwrap().value(vars).to_string());
                    i = t.span.end;
                    continue;
                }
                TokenKind::Escape => {
                    out.push_str(&raw[i + 1..t.span.end]);
                    i = t.span.end;
                    continue;
                }
                TokenKind::Protected => {
                    out.push_str(&raw[i..t.span.end]);
                    protected_end = out.len();
                    i = t.span.end;
                    continue;
                }
                _ => {}
            }
        }
        let c = raw[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    let leading = out.len() - out.trim_start().len();
    let end = out.trim_end().len().max(protected_end);
    out[leading.min(end)..end].to_owned()
}
const SET_USAGE: &str = "/set ROUNDS <1-20> | /set GRILL_ROUNDS <1-10> | /set CENSUS_PASSES <1-5>";
const EXPORT_USAGE: &str =
    "/export room | /export debate|grill|tournament|census|crosscheck|survey|probe [id]";
fn slash(raw: &str, ctx: &ParserSnapshot) -> Result<SlashAction, ParseError> {
    let mut words = raw.split_whitespace();
    let name = &words.next().unwrap()[1..];
    let args = words.collect::<Vec<_>>();
    let usage = |text: &str| Rule::Usage.error(whole(raw), &[("usage", text)]);
    let mut offset = raw.find('/').unwrap() + name.len() + 1;
    let spans = args
        .iter()
        .map(|arg| {
            let start = offset + raw[offset..].find(arg).unwrap();
            offset = start + arg.len();
            Span { start, end: offset }
        })
        .collect::<Vec<_>>();
    let arg_span = |index: usize| spans[index].clone();
    let invalid_id = |argument: &str, value: &str| {
        Rule::InvalidIdentifier.error(
            arg_span(if argument == "id" { 1 } else { 0 }),
            &[("argument", argument), ("value", value)],
        )
    };
    Ok(match name {
        "set" => {
            if args.len() != 2 {
                return Err(usage(SET_USAGE));
            }
            let variable = variable(args[0], arg_span(0))?;
            let value = value(&variable, args[1], arg_span(1))?;
            SlashAction::Set { variable, value }
        }
        "stop" => {
            if args.len() > 1 {
                return Err(usage("/stop [@nick]"));
            }
            let nick = if let Some(arg) = args.first() {
                let name = arg
                    .strip_prefix('@')
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| usage("/stop [@nick]"))?;
                attachments(raw, ctx, false, false, false)?;
                Some(
                    Nick::new(name)
                        .ok()
                        .filter(|n| ctx.roster.contains(n))
                        .ok_or_else(|| {
                            Rule::UnknownRecipient.error(arg_span(0), &[("nick", name)])
                        })?,
                )
            } else {
                None
            };
            SlashAction::Stop { nick }
        }
        "judge" => {
            if args.len() > 1 {
                return Err(usage("/judge [debateId]"));
            }
            SlashAction::Judge {
                debate_id: args
                    .first()
                    .map(|v| v.parse().map_err(|_| invalid_id("debateId", v)))
                    .transpose()?,
            }
        }
        "regen" => {
            if args.len() > 1 {
                return Err(usage("/regen [messageId]"));
            }
            SlashAction::Regen {
                message_id: args
                    .first()
                    .map(|v| v.parse().map_err(|_| invalid_id("messageId", v)))
                    .transpose()?,
            }
        }
        "continue" => {
            let tail = raw.trim_start().strip_prefix("/continue").unwrap().trim();
            let plan = if tail.is_empty() {
                None
            } else {
                let text: String = serde_json::from_str(tail).map_err(|_| usage(CONTINUE_USAGE))?;
                if text.trim().is_empty() || !ctx.metadata.quote.is_empty() {
                    return Err(usage(CONTINUE_USAGE));
                }
                Some(text)
            };
            SlashAction::Continue { plan }
        }
        "accept" => {
            let separator = args
                .iter()
                .position(|a| *a == "--")
                .filter(|i| *i > 0 && *i + 1 < args.len())
                .ok_or_else(|| usage(ACCEPT_USAGE))?;
            // Shape (including duplicate cells) precedes identifier values.
            let mut unique = std::collections::HashSet::new();
            if args[..separator].iter().any(|v| !unique.insert(*v)) {
                return Err(usage(ACCEPT_USAGE));
            }
            let mut cell_ids = Vec::new();
            for (i, arg) in args[..separator].iter().enumerate() {
                let id = arg.parse().map_err(|_| {
                    Rule::InvalidIdentifier
                        .error(arg_span(i), &[("argument", "cellId"), ("value", arg)])
                })?;
                if cell_ids.contains(&id) {
                    return Err(usage(ACCEPT_USAGE));
                }
                cell_ids.push(id);
            }
            SlashAction::Accept {
                cell_ids,
                reason: raw[spans[separator + 1].start..].to_owned(),
            }
        }
        "export" => {
            if args.is_empty() || args.len() > 2 {
                return Err(usage(EXPORT_USAGE));
            }
            let scope = match args[0] {
                "room" => ExportScope::Room,
                "debate" => ExportScope::Debate,
                "grill" => ExportScope::Grill,
                "tournament" => ExportScope::Tournament,
                "census" => ExportScope::Census,
                "crosscheck" => ExportScope::Crosscheck,
                "survey" => ExportScope::Survey,
                "probe" => ExportScope::Probe,
                _ => return Err(usage(EXPORT_USAGE)),
            };
            if scope == ExportScope::Room && args.len() == 2 {
                return Err(usage(EXPORT_USAGE));
            }
            let id = args
                .get(1)
                .map(|v| {
                    v.parse::<crate::ContestId>()
                        .map_err(|_| invalid_id("id", v))
                })
                .transpose()?;
            SlashAction::Export { scope, id }
        }
        "clear" | "who" | "help" => {
            if !args.is_empty() {
                return Err(usage(&format!("/{name}")));
            }
            match name {
                "clear" => SlashAction::Clear,
                "who" => SlashAction::Who,
                _ => SlashAction::Help,
            }
        }
        _ => return Err(Rule::UnknownSlash.error(whole(raw), &[("name", name)])),
    })
}

/// Shared help content: syntax examples only, with no execution or I/O.
pub const CONTINUE_USAGE: &str = "/continue [\"<plan>\"]";
pub const ACCEPT_USAGE: &str = "/accept <cellId…> -- <reason>";

pub const HELP_EXAMPLES: &[&str] = &[
    "@claude Hello",
    "@all -grok Suggest a use case",
    "$ROUNDS=5 $GRILL_ROUNDS=3 $CENSUS_PASSES=2",
    "/set ROUNDS 5",
    "/set GRILL_ROUNDS 3",
    "/set CENSUS_PASSES 2",
    "@grok @gemini $debate Compare these @judge",
    "@all $debate Design a backup plan",
    "@all $tournament Design a useful CLI",
    "@grok $grill",
    "@all $grill Design a backup plan",
    "@all $census Plan text",
    "@all $crosscheck Plan text\n--\nGoal and constraints",
    "@all $survey Which rollout?",
    "@all $probe Requests stall; inspect logs.",
    "@grok @gemini $debate",
    "/continue",
    "/continue \"Updated plan\"",
    "/accept 550e8400-e29b-41d4-a716-446655440000 -- Accepted risk",
    "/export census",
    "/export crosscheck",
    "/export survey",
    "/export probe",
];
