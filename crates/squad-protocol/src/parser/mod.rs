//! Deterministic human-input parsing with supplied room and record snapshots.
pub mod errors;
pub mod lexer;
#[allow(clippy::module_inception)]
mod parser;
mod types;
pub mod validate_context;
pub use parser::{HELP_EXAMPLES, SLASH_COMMANDS, resolve_recipients};
mod context;
#[cfg(test)]
use context::parse_fixture;
pub use context::{restore_pending_human, ParseContext, RecordSummary, parse, validate_context};
pub(crate) use context::{
    complete_crosscheck, continuation_plan, validate_control, waiting_census,
};
pub use types::*;
pub use validate_context::validate_pick;
pub(crate) use validate_context::{
    ParallelAnswerSet, ParserSnapshot, RecordKind, SourceSummary, validate_snapshot,
};

#[cfg(test)]
mod lexer_tests;
#[cfg(test)]
mod parser_tests;
#[cfg(test)]
mod round_trip_tests;
#[cfg(test)]
mod validate_context_tests;

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod controls_tests;

#[cfg(test)]
thread_local! { static ENABLE_NEW_MODES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
fn mode_enabled(kind: crate::ContestKind) -> bool {
 #[cfg(test)]
 if ENABLE_NEW_MODES.with(|v| v.get()) { return true; }
 kind.is_built()
}
#[cfg(test)]
fn with_new_modes<T>(f: impl FnOnce() -> T) -> T {
 struct Reset(bool);
 impl Drop for Reset { fn drop(&mut self) { ENABLE_NEW_MODES.with(|v| v.set(self.0)); } }
 let _reset = Reset(ENABLE_NEW_MODES.with(|v| v.replace(true)));
 f()
}
