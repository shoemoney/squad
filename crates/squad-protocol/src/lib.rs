//! Squad wire types: envelopes, work, errors, config and the command parser.

pub mod config;
pub mod limits;
pub use config::{Nick, SquadConfig};

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
pub use parser::{ExportScope, ParsedInput, VariableName, VariableUpdate};
pub use requests::*;
pub use work::*;
pub type MemberNick = Nick;
pub type DateTime = chrono::DateTime<chrono::Utc>;
