//! Provider accounting may reconcile after completion fencing or room clear.
use rusqlite::Connection;
use squad_protocol::*;
use squad_store::{Result, StoreError};
pub fn reconcile(db: &Connection, owner: &ExecutorKey, record: &UsageRecord) -> Result<()> {
    let owns = match owner {
        ExecutorKey::Member(nick) => {
            record.target == Target::Member(nick.clone())
                && (record.context_id.0 == nick.as_str()
                    || record.contest_id.is_some_and(|contest| {
                        record
                            .context_id
                            .0
                            .starts_with(&format!("contest:{contest}:"))
                            && record.context_id.0.ends_with(&format!(":{nick}"))
                    }))
        }
        ExecutorKey::Case(case) => {
            record.target == Target::Judge
                && record.case_id == Some(*case)
                && record.context_id.0 == format!("judge:{case}")
        }
    };
    if !owns {
        return Err(StoreError::Domain(
            ErrorCode::PRODUCER_FORBIDDEN,
            "Accounting belongs to another executor",
        ));
    }
    squad_store::accounting::reconcile(db, record)
}
