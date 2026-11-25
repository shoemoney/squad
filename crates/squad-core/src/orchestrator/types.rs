//! Frozen plan records and the router boundary. No executor or storage authority lives here.
use serde::{Deserialize, Serialize};
use squad_protocol::*;
use std::collections::BTreeMap;

/// Same hooks as router/contests.rs. The adapter is supplied at integration.
/// Implementations must commit before publishing/waking workers, deduplicate by
/// submission ID, and reject stale versions before allocating work or spending.
pub trait ContestRouter {
    fn produce(
        &mut self,
        envelope: ProducerEnvelope,
    ) -> squad_store::Result<squad_store::commit::CommitOutcome>;
    fn transition(
        &mut self,
        envelope_id: EventId,
        submission: ContestTransitionSubmission,
    ) -> squad_store::Result<squad_store::commit::CommitOutcome>;
    fn barrier(
        &mut self,
        request_id: EventId,
        commit: BarrierCommit,
    ) -> squad_store::Result<squad_store::commit::CommitOutcome>;
    fn allocate_work(
        &mut self,
        request_id: EventId,
        contest: ContestId,
        expected_state_version: u64,
        work: Vec<Work>,
    ) -> squad_store::Result<squad_store::commit::CommitOutcome>;
    fn allocate_step(
        &mut self,
        request_id: EventId,
        step: ContestStep,
        projections: Vec<HistoryProjection>,
        work: Vec<Work>,
        private_maps: BTreeMap<String, serde_json::Value>,
    ) -> squad_store::Result<squad_store::commit::CommitOutcome>;
    fn open_debate_case(
        &mut self,
        event: ProducerEnvelope,
        bundle: DebateAdjudicationBundle,
    ) -> squad_store::Result<squad_store::commit::CommitOutcome>;
    fn debate_state(&self, debate_id: DebateId) -> squad_store::Result<DebateState>;
    fn grill_state(&self, contest_id: ContestId) -> squad_store::Result<GrillState>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenModel {
    pub nick: Option<MemberNick>,
    pub resolved_slug: String,
    pub context_tokens: u64,
    pub input_usd_per_million: f64,
    pub output_usd_per_million: f64,
    pub reasoning_tokens: u32,
    pub reasoning_effort: Option<ReasoningEffort>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenPrompts {
    pub pack_version: String,
    pub composition_hashes: BTreeMap<String, String>,
    pub turn_words: u32,
    pub final_turn_words: u32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrozenSources {
    pub topic_message_id: MessageId,
    pub topic: String,
    pub instruction: String,
    /// None means both participants must produce independent openings.
    pub solutions: Option<[DebateSolution; 2]>,
    pub cutoff: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebatePlan {
    pub preflight: DebatePreflight,
    pub sources: FrozenSources,
    pub members: [FrozenModel; 2],
    pub judge: Option<FrozenModel>,
    pub prompts: FrozenPrompts,
    pub script_version: String,
    pub calls: Vec<CallBound>,
    /// Saved for judge-only rollback; old case identities must be retained.
    pub prior_terminal: Option<DebateState>,
}
/// One model-bearing script invocation, rather than another executable work type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallBound {
    pub kind: WorkKind,
    pub round: Option<u32>,
    pub actor: Target,
    pub input_tokens: u64,
    pub visible_tokens: u32,
    pub reasoning_tokens: u32,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub attempts: u8,
    pub deadline_ms: u64,
    pub estimated_usd: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepSpec {
    pub move_name: ContestMove,
    pub round: Option<u32>,
    pub actors: Vec<Target>,
    pub composition: Option<String>,
    pub word_limit: Option<u32>,
    pub attempts: u8,
    pub deadline_ms: u64,
}
/// Persisted execution observations, read by the router adapter. These refer to
/// canonical Work; they do not schedule calls or contain a second input payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptState {
    pub work: Work,
    pub deadline: Option<DateTime>,
    pub phase: AttemptPhase,
    pub repair_used: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttemptPhase {
    Queued,
    Running,
    Failed,
    Accepted(EventId),
    Exhausted,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq)]
pub enum AttemptDecision {
    Wait,
    Wake(WorkId),
    Retry {
        work_id: WorkId,
        next_attempt: u32,
        repair: bool,
    },
    Forfeit(WorkId),
    Unscored(WorkId),
    JudgeFailed(WorkId),
}

/// Transport failures, schema failures and interruptions share one attempt budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    Transport,
    Timeout,
    Interrupted,
    InvalidOutput,
}
