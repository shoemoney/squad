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
pub mod crosscheck;
pub use crosscheck::*;
pub mod requests;

pub mod capabilities;
pub mod constants;
pub mod cursor;
pub mod debate;
pub mod envelopes;
pub mod probe;
#[cfg(feature = "mode-survey")]
pub mod survey;
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
pub use probe::*;
pub use requests::*;
#[cfg(feature = "mode-survey")]
pub use survey::*;
pub use work::*;
pub type MemberNick = Nick;
pub type DateTime = chrono::DateTime<chrono::Utc>;

pub mod tournament;
pub use tournament::*;
pub mod census;
pub use census::*;
pub mod grill;
pub use grill::*;

pub mod forge;
pub use forge::{ForgeState, ForgeStatus, ForgeTransition, ForgeCaseMaterial};

pub mod forecast;
pub use forecast::{ForecastState, ForecastStatus, ForecastTransition, ForecastCaseMaterial};

pub mod decide;
pub use decide::{DecideState, DecideStatus, DecideTransition, DecideCaseMaterial};

pub mod negotiate;
pub use negotiate::{NegotiateState, NegotiateStatus, NegotiateTransition, NegotiateCaseMaterial};

pub mod blow_it_up;
pub use blow_it_up::{BlowItUpState, BlowItUpStatus, BlowItUpTransition, BlowItUpCaseMaterial};

pub mod readback;
pub use readback::{ReadbackState, ReadbackStatus, ReadbackTransition, ReadbackCaseMaterial};

pub mod debugging;
pub use debugging::{DebuggingState, DebuggingStatus, DebuggingTransition, DebuggingCaseMaterial};

pub mod trace;
pub use trace::{TraceState, TraceStatus, TraceTransition, TraceCaseMaterial};

pub mod rank;
pub use rank::{RankState, RankStatus, RankTransition, RankCaseMaterial};
