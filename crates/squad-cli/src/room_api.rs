//! Temporary integration seam until RoomHandle is available.
//!
//! The adapter must open OneShot with dispatch disabled for commit_only. It must
//! never start work and then cancel it to implement --no-wait. Progress must be
//! computed from durable request/work/contest identities, including accounting,
//! never inferred from a quiet feed or presence alone.
use serde::Serialize;
use squad_protocol::{parser::ParseContext, *};
use std::future::Future;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct RoomError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}
impl std::fmt::Display for RoomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RoomError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    AwaitingHuman,
    Running,
    Succeeded,
    Failed,
}

/// Mirror of RoomHandle; the final three projection hooks bridge the temporary
/// absence of RoomState and its authoritative request progress projection.
pub trait RoomApi {
    type State: Serialize;
    fn submit(&self, request: Request) -> impl Future<Output = RequestResult>;
    fn submit_commit_only(&self, request: Request) -> impl Future<Output = RequestResult> {
        self.submit(request)
    }
    fn subscribe(&self) -> broadcast::Receiver<FeedItem>;
    fn page_events(
        &self,
        before: Option<Cursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<CommittedEnvelope>, RoomError>>;
    fn state(&self) -> impl Future<Output = Self::State>;
    fn export(
        &self,
        scope: ExportScope,
        id: Option<ContestId>,
    ) -> impl Future<Output = Result<String, RoomError>>;
    fn shutdown(&self) -> impl Future<Output = Result<(), RoomError>>;
    fn parse_context(state: &Self::State) -> ParseContext;
    fn progress(state: &Self::State, request: EventId) -> Progress;
    /// Includes causally related work, contest, accounting and supersession events.
    fn relevant(state: &Self::State, request: EventId, event: &CommittedEnvelope) -> bool;
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenPolicy {
    pub mode: OpenMode,
    pub commit_only: bool,
}
impl OpenPolicy {
    pub fn interactive() -> Self {
        Self {
            mode: OpenMode::Interactive,
            commit_only: false,
        }
    }
    pub fn send(no_wait: bool) -> Self {
        Self {
            mode: OpenMode::OneShot,
            commit_only: no_wait,
        }
    }
}
