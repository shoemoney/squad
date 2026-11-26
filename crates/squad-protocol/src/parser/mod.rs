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
pub use context::{ParseContext, RecordSummary, parse, restore_pending_human, validate_context};
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
thread_local! { static ENABLE_NEW_MODES: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
fn mode_enabled(kind: crate::ContestKind) -> bool {
    #[cfg(test)]
    if let Some(enabled) = ENABLE_NEW_MODES.with(|v| v.get()) {
        return enabled;
    }
    kind.is_built()
}
#[cfg(test)]
fn with_new_modes<T>(f: impl FnOnce() -> T) -> T {
    with_mode_enablement(true, f)
}
#[cfg(test)]
fn with_mode_enablement<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset {
        fn drop(&mut self) {
            ENABLE_NEW_MODES.with(|v| v.set(self.0));
        }
    }
    let _reset = Reset(ENABLE_NEW_MODES.with(|v| v.replace(Some(enabled))));
    f()
}

#[cfg(test)]
mod new_modes_tests;

pub fn help_examples() -> Vec<&'static str> {
    let mut examples = HELP_EXAMPLES.to_vec();
    if crate::forge::ENABLED {
        examples.push("@all $forge brief");
    }
    if crate::forecast::ENABLED {
        examples.push("@all $forecast brief");
    }
    if crate::decide::ENABLED {
        examples.push("@all $decide criterion\n--\nA\nB");
    }
    if crate::negotiate::ENABLED {
        examples.push("@all $negotiate brief");
    }
    if crate::blow_it_up::ENABLED {
        examples.push("@all $blow_it_up brief");
    }
    if crate::readback::ENABLED {
        examples.push("@all $readback brief");
    }
    if crate::debugging::ENABLED {
        examples.push("@all $debug brief");
    }
    if crate::trace::ENABLED {
        examples.push("@all $trace question\n--\nsource");
    }
    if crate::rank::ENABLED {
        examples.push("@all $rank criterion\n--\nA\nB\nC");
    }
    if crate::tune::ENABLED {
        examples.push("@all $tune prompt\n--\ntask description");
    }
    if crate::bass_ackwards_prd::ENABLED {
        examples.push("@all $bass_ackwards_prd end result");
    }
    examples
}
