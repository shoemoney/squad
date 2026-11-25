//! Shared protocol contract. Wire shapes follow PRD section 02.
use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum ExecutorKey {
    Member(MemberNick),
    Case(CaseId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptRef {
    pub template_id: Text,
    pub pack_version: Text,
    pub template_hash: Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub nick: Actor, // Member or Judge only
    pub requested_slug: Text,
    pub resolved_slug: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roster_order: Option<u32>,
    pub context_length: u64,
    pub pricing: serde_json::Value,
    pub resolved_at: DateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum HistoryRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HistoryMetadata {
    Attributed {
        origin_event_id: EventId,
        origin_seq: Cursor,
        author: Actor,
        projection_key: Text,
    },
    Anonymous {
        anonymous: bool,
    }, // always true; no additional fields
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub position: HistoryPosition,
    pub role: HistoryRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: String,
    pub meta: HistoryMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkRecord {
    pub work_id: WorkId,
    pub kind: WorkKind,
    pub target: Target,
    pub context_id: ContextId,
    pub status: WorkStatus,
    pub attempt: u32,
    pub runnable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<Work>, // absent until historyThrough is frozen
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_until: Option<DateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all_fields = "camelCase")]
pub enum ReaderError {
    Forbidden,
    NotFound,
    NotRunnable,
    StaleWork,
    StoreUnavailable { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionGrant {
    pub work: Work,
    pub owner: ExecutorKey,
    pub fence: u64,
    pub epoch: u64,
    pub lease_until: DateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryProjection {
    pub context_id: ContextId,
    pub origin_event_id: EventId,
    pub projection_key: Text,
    pub role: HistoryRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: String,
    pub meta: HistoryMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BarrierCommit {
    pub contest_id: ContestId,
    pub step_id: StepId,
    pub expected_state_version: u64,
    pub barrier_event: ProducerEnvelope,
    pub projections: Vec<HistoryProjection>,
    pub next_work: Vec<Work>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JudgeRepairData {
    pub attempt: u32,
    pub rejected_output: String,
    pub validation_errors: Vec<String>,
}

pub type ReaderFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, ReaderError>> + Send + 'a>>;
pub type SubmitFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = RequestResult> + Send + 'a>>;
pub trait HistoryReader: Send + Sync {
    fn for_work(&self, work_id: WorkId) -> ReaderFuture<'_, Vec<HistoryEntry>>;
}
pub trait WorkReader: Send + Sync {
    fn unfinished(&self) -> ReaderFuture<'_, Vec<WorkRecord>>;
    fn get(&self, work_id: WorkId) -> ReaderFuture<'_, Option<WorkRecord>>;
    fn repair_for_attempt(
        &self,
        work_id: WorkId,
        attempt: u32,
    ) -> ReaderFuture<'_, Option<JudgeRepairData>>;
}
pub trait ExecutorSubmit: Send + Sync {
    fn submit(&self, request: ExecutorRequest) -> SubmitFuture<'_>;
}
pub trait PromptRenderer: Send + Sync {
    fn render(
        &self,
        prompt: &PromptRef,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<String, ReaderError>;
}
pub struct Wakeup;
pub struct AgentContext {
    pub mailbox: tokio::sync::mpsc::Receiver<Wakeup>,
    pub history: std::sync::Arc<dyn HistoryReader>,
    pub work: std::sync::Arc<dyn WorkReader>,
    pub submit: std::sync::Arc<dyn ExecutorSubmit>,
    pub render: std::sync::Arc<dyn PromptRenderer>,
    pub cancel: tokio_util::sync::CancellationToken,
}
pub type FeedReceiver = tokio::sync::broadcast::Receiver<FeedItem>;
/// Shared bus interface; executors receive only ExecutorSubmit, never this capability.
pub trait Bus: Send + Sync {
    fn send(&self, request: Request) -> SubmitFuture<'_>;
    fn subscribe(&self) -> FeedReceiver;
    fn wake_mailbox(&self, executor: &ExecutorKey);
}

impl ExecutorKey {
    /// Checks target and context syntax together; step assignments additionally require a frozen assignment.
    pub fn owns(&self, work: &Work) -> bool {
        match (self, &work.target) {
            (Self::Member(nick), Target::Member(target)) => {
                nick == target
                    && (work.context_id.0 == nick.as_str()
                        || match (work.contest_id, work.step_id) {
                            (Some(contest), Some(step)) => {
                                work.context_id.0 == format!("contest:{contest}:{step}:{nick}")
                            }
                            _ => false,
                        })
            }
            (Self::Case(case), Target::Judge) => {
                work.case_id == Some(*case) && work.context_id.0 == format!("judge:{case}")
            }
            _ => false,
        }
    }
}
