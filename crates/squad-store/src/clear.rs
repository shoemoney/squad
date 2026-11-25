//! Clear conversational data in foreign-key order, retaining billing and monotonic counters.
use crate::Result;
use rusqlite::Connection;
pub fn content(db: &Connection) -> Result<()> {
    db.execute_batch("UPDATE spend_daily SET reserved_micros=reserved_micros-COALESCE((SELECT SUM(amount_micros) FROM reservations WHERE reservations.day=spend_daily.day AND attempt IS NULL AND unresolved=0),0); DELETE FROM reservations WHERE attempt IS NULL AND unresolved=0;")?;
    db.execute_batch("DELETE FROM deferred_history; DELETE FROM superseded; DELETE FROM round_scores; DELETE FROM judge_repairs; DELETE FROM mode_artifacts; DELETE FROM contest_assignments; DELETE FROM contest_answers; DELETE FROM contest_picks; DELETE FROM work; DELETE FROM contest_steps; DELETE FROM contest_private_maps; DELETE FROM case_events; DELETE FROM cases; DELETE FROM contest_events; DELETE FROM contests; DELETE FROM history; DELETE FROM receipts; DELETE FROM scores; UPDATE vars SET updated_seq=NULL,value=CASE name WHEN 'ROUNDS' THEN 10 WHEN 'GRILL_ROUNDS' THEN 2 ELSE 2 END; DELETE FROM events; UPDATE room_meta SET epoch=epoch+1,replay_floor=next_event_seq;")?;
    Ok(())
}
