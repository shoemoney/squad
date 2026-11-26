//! Clear conversational data in foreign-key order, retaining billing and monotonic counters.
use crate::Result;
#[derive(Debug, Clone, Copy)]
pub struct Defaults {
    pub rounds: u8,
    pub grill_rounds: u8,
    pub census_passes: u8,
    pub negotiate_rounds: u8,
    pub readback_rounds: u8,
    pub debug_rounds: u8,
    pub tune_generations: u8,
}
impl Default for Defaults {
    fn default() -> Self {
        Self {
            rounds: 10,
            grill_rounds: 2,
            census_passes: 2,
            negotiate_rounds: 3,
            readback_rounds: 2,
            debug_rounds: 3,
            tune_generations: 3,
        }
    }
}
impl From<&squad_protocol::config::RoomDefaults> for Defaults {
    fn from(defaults: &squad_protocol::config::RoomDefaults) -> Self {
        Self {
            rounds: defaults.rounds,
            grill_rounds: defaults.grill_rounds,
            census_passes: defaults.census_passes,
            negotiate_rounds: defaults.negotiate_rounds,
            readback_rounds: defaults.readback_rounds,
            debug_rounds: defaults.debug_rounds,
            tune_generations: defaults.tune_generations,
        }
    }
}
pub fn content(db: &rusqlite::Transaction<'_>) -> Result<()> {
    content_with_defaults(db, Defaults::default())
}
pub fn content_with_defaults(db: &rusqlite::Transaction<'_>, defaults: Defaults) -> Result<()> {
    if !(1..=20).contains(&defaults.rounds)
        || !(1..=10).contains(&defaults.grill_rounds)
        || !(1..=5).contains(&defaults.census_passes)
        || !(1..=6).contains(&defaults.negotiate_rounds)
        || !(1..=4).contains(&defaults.readback_rounds)
        || !(1..=6).contains(&defaults.debug_rounds)
    {
        return Err(crate::error::invalid("Invalid room variable defaults"));
    }
    // Fence first, even though obsolete work will be removed in this transaction.
    db.execute_batch("UPDATE room_meta SET epoch=epoch+1; UPDATE work SET status='cancelled',runnable=0,lease_fence=lease_fence+1,lease_owner=NULL,lease_until=NULL WHERE status IN ('queued','running');")?;
    db.execute_batch("UPDATE spend_daily SET reserved_micros=reserved_micros-COALESCE((SELECT SUM(amount_micros) FROM reservations WHERE reservations.day=spend_daily.day AND attempt IS NULL AND unresolved=0),0); DELETE FROM reservations WHERE attempt IS NULL AND unresolved=0;")?;
    db.execute_batch("DELETE FROM mode_repairs; DELETE FROM contest_artifacts; DELETE FROM contest_human_steps; DELETE FROM probe_roles; DELETE FROM grill_turns; DELETE FROM grill_plans; DELETE FROM survey_votes; DELETE FROM survey_claims; DELETE FROM survey_sources; DELETE FROM crosscheck_audit_samples; DELETE FROM crosscheck_cells; DELETE FROM census_work_inputs; DELETE FROM census_finders; DELETE FROM census_passes; DELETE FROM census_runs; DELETE FROM tournament_results; DELETE FROM tournament_rounds; DELETE FROM tournament_labels; DELETE FROM deferred_history; DELETE FROM superseded; DELETE FROM round_scores; DELETE FROM judge_repairs; DELETE FROM mode_artifacts; DELETE FROM contest_assignments; DELETE FROM contest_answers; DELETE FROM contest_picks; DELETE FROM work; DELETE FROM contest_steps; DELETE FROM contest_private_maps; DELETE FROM case_events; DELETE FROM cases; DELETE FROM contest_events; DELETE FROM contests; DELETE FROM history; DELETE FROM receipts; DELETE FROM scores; UPDATE vars SET updated_seq=NULL; DELETE FROM events; UPDATE room_meta SET replay_floor=next_event_seq;")?;
    for (name, value) in [
        ("ROUNDS", defaults.rounds),
        ("GRILL_ROUNDS", defaults.grill_rounds),
        ("CENSUS_PASSES", defaults.census_passes),
        ("NEGOTIATE_ROUNDS", defaults.negotiate_rounds),
        ("READBACK_ROUNDS", defaults.readback_rounds),
        ("DEBUG_ROUNDS", defaults.debug_rounds),
        ("TUNE_GENERATIONS", defaults.tune_generations),
    ] {
        db.execute(
            "UPDATE vars SET value=? WHERE name=?",
            rusqlite::params![value, name],
        )?;
    }
    Ok(())
}
