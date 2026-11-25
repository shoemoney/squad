//! Selection over an authorized, cutoff-bounded ordinary reply snapshot.
use super::types::FrozenSources;
use squad_protocol::*;

#[derive(Debug, Clone)]
pub struct SourceReply {
    pub solution: DebateSolution,
    pub topic_message_id: MessageId,
    pub topic: String,
    pub accepted_order: u64,
    pub completed: bool,
    pub superseded: bool,
    pub ordinary_to_human: bool,
}
impl SourceReply {
    fn eligible(&self, nick: &MemberNick, cutoff: u64) -> bool {
        self.solution.nick == *nick
            && self.accepted_order <= cutoff
            && self.completed
            && !self.superseded
            && self.ordinary_to_human
            && !self.solution.text.trim().is_empty()
    }
}
pub fn select(
    debaters: &[MemberNick; 2],
    replies: &[SourceReply],
    explicit: &[MessageId],
    cutoff: u64,
    command_id: MessageId,
    instruction: &str,
) -> Result<FrozenSources, ErrorCode> {
    if debaters[0] == debaters[1]
        || explicit.len() > 2
        || (explicit.len() == 2 && explicit[0] == explicit[1])
    {
        return Err(ErrorCode::DEBATE_SOURCE_MISSING);
    }
    // Every explicit reference must be eligible and belong to exactly one debater.
    let mut selected: [Option<&SourceReply>; 2] = [None, None];
    for id in explicit {
        let source = replies
            .iter()
            .find(|r| r.solution.message_id == *id)
            .ok_or(ErrorCode::DEBATE_SOURCE_MISSING)?;
        let index = debaters
            .iter()
            .position(|n| source.eligible(n, cutoff))
            .ok_or(ErrorCode::DEBATE_SOURCE_MISSING)?;
        if selected[index].replace(source).is_some() {
            return Err(ErrorCode::DEBATE_SOURCE_MISSING);
        }
    }
    for (index, nick) in debaters.iter().enumerate() {
        if selected[index].is_none() {
            selected[index] = replies
                .iter()
                .filter(|r| r.eligible(nick, cutoff))
                .max_by_key(|r| r.accepted_order);
        }
    }
    if let [Some(a), Some(b)] = selected
        && a.topic_message_id != b.topic_message_id
    {
        return Err(ErrorCode::DEBATE_TOPIC_MISMATCH);
    }
    let source = selected.iter().flatten().next().copied();
    let (topic_message_id, topic) = source
        .map(|s| (s.topic_message_id, s.topic.clone()))
        .unwrap_or((command_id, instruction.to_owned()));
    if topic.trim().is_empty() {
        return Err(ErrorCode::DEBATE_SOURCE_MISSING);
    }
    let solutions = match selected {
        [Some(a), Some(b)] => Some([a.solution.clone(), b.solution.clone()]),
        _ => None,
    };
    Ok(FrozenSources {
        topic_message_id,
        topic,
        instruction: instruction.into(),
        solutions,
        cutoff,
    })
}
