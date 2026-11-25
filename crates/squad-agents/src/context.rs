//! Prompt assembly over a single executor-bound history capability.
use crate::openrouter::ChatMessage;
use squad_protocol::*;
use std::collections::BTreeMap;

/// UTF-8 bytes deliberately overestimate tokens; overhead covers roles/names.
pub const ESTIMATOR: &str = "utf8-bytes-plus-16";
pub fn estimate(messages: &[ChatMessage]) -> u64 {
    messages
        .iter()
        .map(|m| m.content.len() as u64 + m.name.as_ref().map_or(0, |n| n.len() as u64) + 16)
        .sum()
}
#[derive(Debug, Clone, PartialEq)]
pub enum ContextError {
    Reader(ReaderError),
    InvalidWork,
    TooLarge { estimated: u64, allowed: u64 },
}
impl From<ReaderError> for ContextError {
    fn from(error: ReaderError) -> Self {
        Self::Reader(error)
    }
}
/// Complete output reservation includes reasoning, after published-output clamping.
pub fn input_budget(context_limit: u64, output_reservation: u64) -> u64 {
    let safety = 1024.max(context_limit.div_ceil(20));
    context_limit.saturating_sub(output_reservation.saturating_add(safety))
}
#[derive(Clone)]
pub struct MemberPrompt {
    pub system: PromptRef,
    pub roster: Vec<MemberNick>,
    pub user_name: String,
    pub persona: String,
}
impl MemberPrompt {
    pub fn values(&self, grant: &ExecutionGrant) -> Result<BTreeMap<String, String>, ContextError> {
        let ExecutorKey::Member(nick) = &grant.owner else {
            return Err(ContextError::InvalidWork);
        };
        Ok(BTreeMap::from([
            ("nick".into(), nick.to_string()),
            ("model_slug".into(), grant.binding.model.to_string()),
            (
                "member_nicks_json".into(),
                serde_json::to_string(&self.roster).expect("serializable nicks"),
            ),
            (
                "user_name_json".into(),
                serde_json::to_string(if self.user_name.is_empty() {
                    "the user"
                } else {
                    &self.user_name
                })
                .expect("serializable string"),
            ),
            (
                "persona_json".into(),
                serde_json::to_string(&self.persona).expect("serializable string"),
            ),
            ("room_word_limit".into(), constants::ROOM_WORDS.to_string()),
        ]))
    }
}
/// The reader owns supersession, quote snapshots, and trigger-bound projections.
/// This additional boundary check fails closed if a faulty reader returns future rows.
pub async fn read_history(
    context: &AgentContext,
    grant: &ExecutionGrant,
) -> Result<Vec<HistoryEntry>, ContextError> {
    if context.executor != grant.owner
        || !context.executor.owns(&grant.work)
        || grant.work.validate().is_err()
    {
        return Err(ContextError::InvalidWork);
    }
    let history = context.history.for_work(grant.work.work_id).await?;
    if history
        .iter()
        .any(|h| h.position > grant.work.history_through)
        || history.windows(2).any(|w| w[0].position >= w[1].position)
    {
        return Err(ContextError::InvalidWork);
    }
    Ok(history)
}
fn member_message(entry: &HistoryEntry, own: &MemberNick) -> ChatMessage {
    // Names and roles are derived from trusted metadata, never model text.
    let (role, name, notice) = match &entry.meta {
        HistoryMetadata::Attributed {
            author: Actor::Me, ..
        } => ("user", Some("me".into()), false),
        HistoryMetadata::Attributed {
            author: Actor::Member(nick),
            ..
        } if nick == own => ("assistant", Some(nick.to_string()), false),
        HistoryMetadata::Attributed {
            author: Actor::Member(nick),
            ..
        } => ("user", Some(nick.to_string()), false),
        HistoryMetadata::Attributed { .. } => ("user", Some("squad".into()), true),
        HistoryMetadata::Anonymous { .. } => ("user", None, false),
    };
    ChatMessage {
        role: role.into(),
        name,
        content: if notice {
            format!("[squad notice]\n{}", entry.content)
        } else {
            entry.content.clone()
        },
    }
}
/// Preserve all contest inputs. Ordinary history drops oldest whole exchanges,
/// retaining system, current human request, and any trailing material together.
pub async fn member_messages(
    context: &AgentContext,
    grant: &ExecutionGrant,
    prompt: &MemberPrompt,
    context_limit: u64,
    output_reservation: u64,
) -> Result<Vec<ChatMessage>, ContextError> {
    let history = read_history(context, grant).await?;
    if grant.work.kind == WorkKind::Reply
        && grant.work.step_id.is_none()
        && !history.iter().any(|entry| {
            matches!(
                entry.meta,
                HistoryMetadata::Attributed {
                    author: Actor::Me,
                    ..
                }
            )
        })
    {
        return Err(ContextError::InvalidWork);
    }
    let ExecutorKey::Member(nick) = &grant.owner else {
        return Err(ContextError::InvalidWork);
    };
    let system = context
        .render
        .render(&prompt.system, &prompt.values(grant)?)?;
    let mut messages = vec![ChatMessage {
        role: "system".into(),
        name: None,
        content: system,
    }];
    messages.extend(history.iter().map(|h| member_message(h, nick)));
    if let Some(prompt_ref) = &grant.work.prompt_ref {
        let values = context
            .work
            .prompt_values_for_work(grant.work.work_id)
            .await?;
        let instruction = context.render.render(prompt_ref, &values)?;
        messages.push(ChatMessage {
            role: "user".into(),
            name: Some("squad".into()),
            content: instruction,
        });
    }
    let allowed = input_budget(context_limit, output_reservation);
    if grant.work.contest_id.is_none() {
        // A new exchange starts only at an authenticated human request.
        let mut starts: Vec<usize> = history
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                matches!(
                    &h.meta,
                    HistoryMetadata::Attributed {
                        author: Actor::Me,
                        ..
                    }
                )
                .then_some(i + 1)
            })
            .collect();
        while estimate(&messages) > allowed && starts.len() > 1 {
            let remove = starts[1] - 1;
            messages.drain(1..starts[1]);
            starts.remove(0);
            for start in &mut starts {
                *start -= remove;
            }
        }
    }
    enforce_budget(&messages, allowed)?;
    Ok(messages)
}
pub fn enforce_budget(messages: &[ChatMessage], allowed: u64) -> Result<(), ContextError> {
    let estimated = estimate(messages);
    if estimated > allowed {
        Err(ContextError::TooLarge { estimated, allowed })
    } else {
        Ok(())
    }
}
/// Judge templates receive evidence once; no underlying history is appended.
pub fn judge_messages(
    system: String,
    user: String,
    context_limit: u64,
    output_reservation: u64,
) -> Result<Vec<ChatMessage>, ContextError> {
    let messages = vec![
        ChatMessage {
            role: "system".into(),
            name: None,
            content: system,
        },
        ChatMessage {
            role: "user".into(),
            name: Some("squad".into()),
            content: user,
        },
    ];
    enforce_budget(&messages, input_budget(context_limit, output_reservation))?;
    Ok(messages)
}
