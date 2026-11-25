//! Human control requests needed by the parser milestone.
//! The bus milestone extends this registry with the remaining request variants.
use crate::{ContestId, EventId, Nick};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Request {
    ContestPick {
        request_id: EventId,
        contest_id: ContestId,
        nicks: Vec<Nick>,
    },
}
