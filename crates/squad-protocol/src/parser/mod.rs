//! Deterministic human-input parsing with supplied room and record snapshots.
pub mod errors;
pub mod lexer;
#[allow(clippy::module_inception)]
mod parser;
mod types;
pub mod validate_context;
pub use parser::{HELP_EXAMPLES, parse, resolve_recipients};
pub use types::*;
pub use validate_context::*;

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
