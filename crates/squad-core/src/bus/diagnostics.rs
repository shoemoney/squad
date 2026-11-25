//! Structured metadata only: message bodies, prompts and provider output are redacted.
use squad_protocol::*;
pub fn request(id: EventId, owner: Option<&ExecutorKey>) {
    tracing::debug!(request_id=%id,executor=?owner,"bus.request");
}
pub fn commit(event: &CommittedEnvelope) {
    tracing::debug!(event_id=%event.base.id,correlation_id=%event.base.correlation_id,causation_id=?event.base.causation_id,seq=event.commit.seq.get(),commit_index=event.commit.commit_index,commit_size=event.commit.commit_size,"bus.commit");
}
pub fn mailbox(executor: &ExecutorKey, coalesced: bool) {
    tracing::debug!(?executor, coalesced, "bus.mailbox");
}
