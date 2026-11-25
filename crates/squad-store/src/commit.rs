//! The receipt gate precedes mutable resolution. All effects run under BEGIN IMMEDIATE.
use crate::{
    Result, Store, StoreError,
    error::{decode, encode, invalid},
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use squad_protocol::*;
#[derive(Debug)]
pub struct CommitOutcome {
    pub result: RequestResult,
    pub events: Vec<CommittedEnvelope>,
    pub duplicate: bool,
}
pub struct CommitGroup {
    pub accepted_at: DateTime,
    pub epoch: i64,
    start: i64,
    ids: Vec<EventId>,
}
impl CommitGroup {
    pub fn push(&mut self, tx: &Transaction<'_>, envelope: ProducerEnvelope) -> Result<Cursor> {
        envelope
            .validate()
            .map_err(|_| invalid("Invalid envelope"))?;
        if !envelope.content.is_durable() {
            return Err(invalid("Ephemeral event cannot be committed"));
        }
        let seq = self.start + self.ids.len() as i64;
        let content = serde_json::to_value(&envelope.content)?;
        tx.execute("INSERT INTO events(seq,id,room_id,epoch,version,kind,correlation_id,causation_id,sender,recipients,payload,created_at,accepted_at,commit_start_seq,commit_index,commit_size) VALUES (?1,?2,'main',?3,1,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)", params![seq,envelope.base.id.to_string(),self.epoch,content["kind"].as_str(),envelope.base.correlation_id.to_string(),envelope.base.causation_id.map(|id|id.to_string()),envelope.base.sender.to_string(),encode(&envelope.base.recipients)?,encode(&content["payload"])?,envelope.base.created_at.to_rfc3339(),self.accepted_at.to_rfc3339(),self.start,self.ids.len() as i64,self.ids.len() as i64+1])?;
        self.ids.push(envelope.base.id);
        Ok(Cursor::new(seq).expect("positive sequence"))
    }
    pub fn accepted(&self, request_id: EventId) -> RequestResult {
        RequestResult::Accepted {
            request_id,
            event_ids: self.ids.clone(),
            cursor: self.ids.last().map(|_| {
                Cursor::new(self.start + self.ids.len() as i64 - 1).expect("positive sequence")
            }),
        }
    }
}
pub fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
impl Store {
    pub fn commit(
        &self,
        request_id: EventId,
        producer: String,
        digest: String,
        operation: impl FnOnce(&Transaction<'_>, &mut CommitGroup) -> Result<RequestResult>
        + Send
        + 'static,
    ) -> Result<CommitOutcome> {
        self.call(move |db| commit(db, request_id, &producer, &digest, operation))
    }
}
pub fn commit(
    db: &mut Connection,
    request_id: EventId,
    producer: &str,
    digest: &str,
    operation: impl FnOnce(&Transaction<'_>, &mut CommitGroup) -> Result<RequestResult>,
) -> Result<CommitOutcome> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let receipt: Option<(String,String,String)> = tx.query_row("SELECT producer_identity,normalized_digest,result_json FROM receipts WHERE request_id=?",[request_id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    if let Some((identity, old, result)) = receipt {
        if old != digest || producer != identity {
            return Err(StoreError::Domain(
                ErrorCode::IDEMPOTENCY_CONFLICT,
                "Submission identity already has different content",
            ));
        }
        return Ok(CommitOutcome {
            result: decode(result)?,
            events: Vec::new(),
            duplicate: true,
        });
    }
    let (epoch, start) = tx.query_row("SELECT epoch,next_event_seq FROM room_meta", [], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    let mut group = CommitGroup {
        accepted_at: chrono::Utc::now(),
        epoch,
        start,
        ids: Vec::new(),
    };
    let result = operation(&tx, &mut group)?;
    let size = group.ids.len() as i64;
    tx.execute(
        "UPDATE events SET commit_size=? WHERE commit_start_seq=?",
        params![size, start],
    )?;
    tx.execute("UPDATE room_meta SET next_event_seq=?", [start + size])?;
    tx.execute(
        "INSERT INTO receipts VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            request_id.to_string(),
            group.epoch,
            producer,
            digest,
            encode(&result)?,
            if size == 0 {
                None
            } else {
                Some(start + size - 1)
            }
        ],
    )?;
    let events = (start..start + size)
        .map(|seq| crate::events::at(&tx, seq))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;
    Ok(CommitOutcome {
        result,
        events,
        duplicate: false,
    })
}
