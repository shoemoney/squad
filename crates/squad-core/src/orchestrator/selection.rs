//! Question-first selection reuses ordinary answers and makes no provider calls.
use super::types::FrozenSources;
use squad_protocol::*;
use std::collections::BTreeSet;

pub fn finish_collection(
    state: &ContestSelectionState,
    resolved: &[MemberNick],
) -> Result<Option<ContestPickRequested>, ErrorCode> {
    if state.state != ContestSelectionStatus::CollectingAnswers {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    if !state.addressed.iter().all(|n| resolved.contains(n)) {
        return Ok(None);
    }
    let mut seen = BTreeSet::new();
    let eligible: Vec<_> = state
        .answers
        .iter()
        .filter(|a| {
            state.addressed.contains(&a.nick)
                && !a.text.trim().is_empty()
                && seen.insert(a.nick.clone())
        })
        .map(|a| a.nick.clone())
        .collect();
    if eligible.len() < usize::from(state.required_picks) {
        return Err(ErrorCode::CONTEST_NOT_ENOUGH_ANSWERS);
    }
    Ok(Some(ContestPickRequested {
        contest_id: state.contest_id,
        kind: state.kind,
        question_message_id: state.question_message_id,
        eligible,
        required_picks: state.required_picks,
        banner: match state.kind {
            ContestKind::Debate => "CHOOSE YOUR DEBATE BROS",
            ContestKind::Grill => "CHOOSE WHO GETS GRILLED",
            _ => return Err(ErrorCode::COMMAND_NOT_IMPLEMENTED),
        }
        .into(),
    }))
}
pub fn pick(state: &ContestSelectionState, request: &Request) -> Result<ContestPicked, ErrorCode> {
    let Request::ContestPick {
        contest_id, nicks, ..
    } = request
    else {
        return Err(ErrorCode::CONTEST_PICK_INVALID);
    };
    if state.state != ContestSelectionStatus::AwaitingPick
        || *contest_id != state.contest_id
        || nicks.len() != usize::from(state.required_picks)
        || nicks.iter().collect::<BTreeSet<_>>().len() != nicks.len()
    {
        return Err(ErrorCode::CONTEST_PICK_INVALID);
    }
    let mut ids = Vec::new();
    for nick in nicks {
        let answer = state
            .answers
            .iter()
            .find(|a| {
                &a.nick == nick && state.addressed.contains(nick) && !a.text.trim().is_empty()
            })
            .ok_or(ErrorCode::CONTEST_PICK_INVALID)?;
        ids.push(answer.message_id);
    }
    Ok(ContestPicked {
        contest_id: *contest_id,
        kind: state.kind,
        nicks: nicks.clone(),
        source_message_ids: ids,
    })
}
/// The caller sets judge_requested=true. There is no opening round and the
/// collection's ordinary spend is excluded from execution preflight.
pub fn debate_sources(
    state: &ContestSelectionState,
    picked: &ContestPicked,
    question: &str,
    cutoff: u64,
) -> Result<FrozenSources, ErrorCode> {
    if state.kind != ContestKind::Debate
        || picked.kind != ContestKind::Debate
        || picked.contest_id != state.contest_id
        || picked.nicks.len() != 2
        || picked.source_message_ids.len() != 2
        || picked.nicks[0] == picked.nicks[1]
        || question.trim().is_empty()
    {
        return Err(ErrorCode::CONTEST_PICK_INVALID);
    }
    let mut solutions = Vec::new();
    for (nick, id) in picked.nicks.iter().zip(&picked.source_message_ids) {
        let a = state
            .answers
            .iter()
            .find(|a| &a.nick == nick && a.message_id == *id)
            .ok_or(ErrorCode::CONTEST_PICK_INVALID)?;
        solutions.push(DebateSolution {
            nick: a.nick.clone(),
            message_id: a.message_id,
            text: a.text.clone(),
        });
    }
    Ok(FrozenSources {
        topic_message_id: state.question_message_id,
        topic: question.into(),
        instruction: String::new(),
        solutions: Some(
            solutions
                .try_into()
                .map_err(|_| ErrorCode::CONTEST_PICK_INVALID)?,
        ),
        cutoff,
    })
}
