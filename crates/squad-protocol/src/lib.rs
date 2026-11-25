//! Squad wire types: envelopes, work, errors, config and the command parser.

pub mod config;
pub use config::{Nick, ReasoningEffort, SquadConfig};
/// Compatibility path; shared values live in [`constants`].
pub use constants as limits;

pub mod errors;
pub mod identities;
pub mod parser;
pub use errors::ErrorCode;
pub mod contest;
pub mod requests;

pub mod capabilities;
pub mod constants;
pub mod cursor;
pub mod debate;
pub mod envelopes;
pub mod validation;
pub mod verdict;
pub mod work;
pub use capabilities::*;
pub use contest::*;
pub use cursor::*;
pub use debate::*;
pub use envelopes::*;
pub use identities::*;
pub use parser::{
    ExportScope, ParseContext, ParsedInput, RecordSummary, SubmissionMeta, VariableName,
    VariableUpdate, parse,
};
pub use requests::*;
pub use work::*;
pub type MemberNick = Nick;
pub type DateTime = chrono::DateTime<chrono::Utc>;

pub mod tournament;
pub use tournament::*;
