//! Registry derived exclusively from Foundations configuration.
use crate::openrouter::{
    ChatRequest, DeadlineClass, ExecutionPolicy, ReasoningEffort as WireEffort,
};
use crate::resolver::Model;
use squad_protocol::config::{AgentConfig, ConfigError, ReasoningEffort, SquadConfig};
use squad_protocol::constants as limits;

pub const DEFAULT_MODELS: [(&str, &str); 6] = [
    ("claude", "~anthropic/claude-opus-latest"),
    ("fable", "~anthropic/claude-fable-latest"),
    ("openai", "~openai/gpt-sol-latest"),
    ("gemini", "google/gemini-4-neon"),
    ("grok", "~x-ai/grok-latest"),
    ("muse", "meta/muse-wonder"),
];

#[derive(Clone, Copy, Debug)]
pub enum TaskKind {
    Reply,
    DebateTurn,
    Plan,
    Judge,
    TournamentJudge,
    RoundJudge,
    CensusJudge,
    CrosscheckJudge,
    SurveyJudge,
    ProbeJudge,
}

pub struct Registry<'a> {
    config: &'a SquadConfig,
}
impl<'a> Registry<'a> {
    pub fn new(config: &'a SquadConfig) -> Result<Self, ConfigError> {
        if !(limits::MIN_MEMBERS..=limits::MAX_MEMBERS).contains(&config.agents.len()) {
            return Err(ConfigError::MemberCount);
        }
        let mut seen = std::collections::BTreeSet::new();
        for member in &config.agents {
            if !seen.insert(&member.nick) {
                return Err(ConfigError::DuplicateNick(member.nick.to_string()));
            }
        }
        Ok(Self { config })
    }
    pub fn members(&self) -> &[AgentConfig] {
        &self.config.agents
    }
    /// Build a single-call request from the router's frozen dispatch binding.
    /// The worker, rather than the transport, admits each ordinary retry.
    pub fn work_request(
        &self,
        member: Option<&AgentConfig>,
        model: &Model,
        grant: &squad_protocol::ExecutionGrant,
    ) -> Result<ChatRequest, crate::openrouter::TransportError> {
        use squad_protocol::{ExecutorKey, WorkKind};
        if model.id != grant.binding.model.as_str()
            || grant.work.validate().is_err()
            || !grant.owner.owns(&grant.work)
            || match &grant.owner {
                ExecutorKey::Member(nick) => member.is_none_or(|m| m.nick != *nick),
                ExecutorKey::Case(_) => member.is_some(),
            }
        {
            return Err(crate::openrouter::TransportError::InvalidModel);
        }
        let (kind, deadline) = match grant.work.kind {
            WorkKind::Reply => (TaskKind::Reply, DeadlineClass::Ordinary),
            WorkKind::DebateTurn => (TaskKind::DebateTurn, DeadlineClass::DebateTurn),
            WorkKind::Judge => (TaskKind::Judge, DeadlineClass::FinalJudge),
            WorkKind::JudgeTournament => {
                (TaskKind::TournamentJudge, DeadlineClass::TournamentJudge)
            }
            WorkKind::JudgeRound => (TaskKind::RoundJudge, DeadlineClass::RoundJudge),
        };
        let mut request = self.request(member, model, kind)?;
        request.reasoning_effort = grant.binding.effort;
        request.visible_ceiling = grant
            .work
            .max_output_tokens
            .unwrap_or(request.visible_ceiling);
        request.policy = ExecutionPolicy::SingleAttempt(deadline);
        request.body()?;
        Ok(request)
    }
    /// Builds the transport request with the startup-pinned concrete identity.
    /// Context assembly supplies messages and strict schemas afterwards.
    pub fn request(
        &self,
        member: Option<&AgentConfig>,
        model: &Model,
        kind: TaskKind,
    ) -> Result<ChatRequest, crate::openrouter::TransportError> {
        let (effort, explicit, visible, policy) = match kind {
            TaskKind::Reply | TaskKind::DebateTurn | TaskKind::Plan => {
                let effort = member
                    .and_then(|m| m.reasoning_effort)
                    .unwrap_or(self.config.defaults.reasoning_effort);
                let explicit = member.is_some_and(|m| m.reasoning_effort.is_some())
                    || self.config.defaults.reasoning_effort_explicit;
                let (visible, policy) = match kind {
                    TaskKind::Reply => {
                        (limits::REPLY_VISIBLE_TOKENS, ExecutionPolicy::OrdinaryReply)
                    }
                    TaskKind::Plan => (
                        limits::PLAN_VISIBLE_TOKENS,
                        ExecutionPolicy::SingleAttempt(DeadlineClass::DebateTurn),
                    ),
                    _ => (
                        limits::DEBATE_TURN_VISIBLE_TOKENS,
                        ExecutionPolicy::SingleAttempt(DeadlineClass::DebateTurn),
                    ),
                };
                (effort, explicit, visible, policy)
            }
            TaskKind::RoundJudge => (
                ReasoningEffort::Low,
                false,
                limits::ROUND_JUDGE_VISIBLE_TOKENS,
                ExecutionPolicy::SingleAttempt(DeadlineClass::RoundJudge),
            ),
            TaskKind::CensusJudge
            | TaskKind::CrosscheckJudge
            | TaskKind::SurveyJudge
            | TaskKind::ProbeJudge => (
                ReasoningEffort::Medium,
                false,
                4096,
                ExecutionPolicy::SingleAttempt(DeadlineClass::FinalJudge),
            ),
            TaskKind::Judge | TaskKind::TournamentJudge => (
                self.config.judge.reasoning_effort,
                self.config.judge.reasoning_effort_explicit,
                limits::FINAL_JUDGE_VISIBLE_TOKENS,
                ExecutionPolicy::SingleAttempt(if matches!(kind, TaskKind::TournamentJudge) {
                    DeadlineClass::TournamentJudge
                } else {
                    DeadlineClass::FinalJudge
                }),
            ),
        };
        let request = ChatRequest {
            model: model.id.clone(),
            messages: Vec::new(),
            visible_ceiling: visible,
            published_output_limit: model.output_limit,
            supports_reasoning: model.supports_reasoning(),
            reasoning_effort: match effort {
                ReasoningEffort::Low => WireEffort::Low,
                ReasoningEffort::Medium => WireEffort::Medium,
                ReasoningEffort::High => WireEffort::High,
            },
            reasoning_explicit: explicit,
            response_format: None,
            policy,
        };
        request.body()?;
        Ok(request)
    }
}
