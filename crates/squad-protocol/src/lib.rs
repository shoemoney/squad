//! Squad wire types: envelopes, work, errors, config and the command parser.

pub mod config;
pub mod limits;
pub use config::{Nick, SquadConfig};

pub mod errors;
pub mod identities;
pub mod parser;
pub use errors::ErrorCode;
pub use identities::{ContestId, DebateId, EventId, MessageId};
pub mod contest;
pub use contest::ContestKind;
pub mod requests;
pub use requests::Request;
