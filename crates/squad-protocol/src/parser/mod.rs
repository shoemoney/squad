//! Deterministic human-input parsing with supplied room and record snapshots.
pub mod errors;
pub mod lexer;
#[allow(clippy::module_inception)]
mod parser;
mod types;
pub mod validate_context;
pub use parser::{HELP_EXAMPLES, resolve_recipients};
mod context;
#[cfg(test)]
use context::parse_fixture;
pub use context::{ParseContext, RecordSummary, parse, validate_context};
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
